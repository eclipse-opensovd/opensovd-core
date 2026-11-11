// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::num::TryFromIntError;
use std::sync::Arc;
use std::{any::Any, collections::HashMap};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use tokio::sync::{RwLock, watch};

#[derive(Debug, thiserror::Error, Clone)]
pub enum UpdateError {
    #[error("creating response failed: {0}")]
    ResponseFailed(String),
    #[error("The package cannot be installed automatically")]
    AutomatedUpdateNotSupported,
    #[error("An update is already executed")]
    UpdateExecutionInProgress(String),
    #[error("An update is already in preparation")]
    UpdateProcessInProgress(String),
    #[error("An update is already in preparation")]
    UpdatePreparationInProgress(String),
    #[error("UpdateProvider is not configured")]
    UpdateProviderNotConfigured,
    #[error("Update provider error: {0}")]
    ProviderError(String),
    #[error("Update ID conflicts with existing ID: {0}")]
    UpdateIdConflict(String),
    #[error("Incompatible model")]
    IncompatibleModel,
    #[error("Update not found: {0}")]
    UpdateNotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Prepare,
    Execute,
}

#[derive(Debug, Clone, Default)]
pub enum Status {
    #[default]
    Pending,
    InProgress,
    Failed(UpdateError),
    Completed,
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Status::Pending, Status::Pending)
            | (Status::InProgress, Status::InProgress)
            | (Status::Completed, Status::Completed) => true,
            (Status::Failed(e1), Status::Failed(e2)) => e1.to_string() == e2.to_string(),
            _ => false,
        }
    }
}

type FeedbackSender = watch::Sender<Option<Arc<dyn UpdateFeedback>>>;
type FeedbackReceiver = watch::Receiver<Option<Arc<dyn UpdateFeedback>>>;
type Request2Update = Arc<
    dyn Fn(UpdateRequest) -> BoxFuture<'static, Result<Arc<dyn UpdateDescriptor>, UpdateError>>
        + Send
        + Sync,
>;
type Feedback2Model =
    Arc<dyn Fn(&dyn UpdateFeedback) -> Result<Arc<dyn Any>, UpdateError> + Send + Sync>;

struct ActiveUpdate {
    id: String,
    attempt: Arc<()>,
    phase: Phase,
}

pub enum UpdateFeedbackState<Model> {
    Pending(Phase),
    Feedback(Arc<Model>),
}

struct UpdatesInner {
    provider: Option<Arc<dyn UpdateProvider>>,
    available: Vec<Arc<dyn UpdateDescriptor>>,
    feedback: HashMap<String, (FeedbackSender, FeedbackReceiver)>,
    // active is used to ensure that only one update is executed at a time.
    // before an update is executed, it is set to the ID of that update.
    // check_and_set_active is used to check if the active update is still in
    // progress, and to set a new active update if none is active.
    active: Option<ActiveUpdate>,
    // model2update stores the method UpdateModel -> UpdateImpl from the
    // UpdateDescriptor trait as a workaround since the actual UpdateImpl type
    // implementing it is erased from Updates.
    request2update: Request2Update,
    feedback2model: Feedback2Model,
}

#[derive(Clone)]
pub struct Updates {
    inner: Arc<RwLock<UpdatesInner>>,
}

impl Updates {
    #[must_use]
    pub fn new<UpdateImpl, FeedbackImpl, FeedbackModel>(
        provider: impl UpdateProvider + 'static,
    ) -> Self
    where
        UpdateImpl: UpdateDescriptor + FromRequest + 'static,
        FeedbackImpl: ToModel<FeedbackModel> + 'static,
        FeedbackModel: 'static,
    {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                provider: Some(Arc::new(provider)),
                available: Vec::new(),
                feedback: HashMap::new(),
                active: None,
                request2update: Arc::new(|stream| {
                    Box::pin(async move {
                        let update = UpdateImpl::from_request(stream).await?;
                        Ok::<Arc<dyn UpdateDescriptor>, _>(Arc::new(update))
                    })
                }),
                feedback2model: Arc::new(|feedback| {
                    let feedback = feedback
                        .as_any()
                        .downcast_ref::<FeedbackImpl>()
                        .ok_or(UpdateError::IncompatibleModel)?;
                    Ok(Arc::new(FeedbackImpl::to_model(feedback)))
                }),
            })),
        }
    }

    /// Tries to register an update from an incoming request.
    ///
    /// # Errors
    ///
    /// Returns both the same errors as [Self::push] and any errors returned
    /// by [FromRequest::from_request] while handling the request.
    pub async fn push_from_request(&self, request: UpdateRequest) -> Result<String, UpdateError> {
        let mut inner = self.inner.write().await;
        let update = (inner.request2update)(request).await?;
        self.push_impl(&mut inner, update).await
    }

    /// Adds an update to the list of available updates.
    ///
    /// # Errors
    ///
    /// Returns `UpdateProviderNotConfigured` if no update provider is
    /// configured.
    pub async fn push<UpdateImpl: UpdateDescriptor + 'static>(
        &self,
        update: UpdateImpl,
    ) -> Result<String, UpdateError> {
        let mut inner = self.inner.write().await;
        self.push_impl(&mut inner, Arc::new(update)).await
    }

    async fn push_impl(
        &self,
        inner: &mut UpdatesInner,
        update: Arc<dyn UpdateDescriptor>,
    ) -> Result<String, UpdateError> {
        let id = update.id().clone();
        if inner.available.iter().any(|u| u.id() == id) {
            return Err(UpdateError::UpdateIdConflict(id));
        }
        inner.available.push(update);
        Ok(id)
    }

    #[must_use]
    pub async fn remove(&self, update_package_id: &str) -> bool {
        let mut inner = self.inner.write().await;
        if inner
            .active
            .as_ref()
            .is_some_and(|active| active.id == update_package_id)
        {
            let terminal = inner
                .feedback
                .get(update_package_id)
                .is_some_and(|(_, rx)| {
                    rx.borrow().as_ref().is_some_and(|feedback| {
                        matches!(feedback.status(), Status::Completed | Status::Failed(_))
                    })
                });
            if !terminal {
                return false;
            }
            inner.active = None;
        }
        inner.available.retain(|u| u.id() != update_package_id);
        inner.feedback.remove(update_package_id);
        true
    }

    #[must_use]
    pub async fn available(&self) -> Vec<Arc<dyn UpdateDescriptor>> {
        self.inner.read().await.available.clone()
    }

    #[must_use]
    pub async fn find(&self, update_package_id: &str) -> Option<Arc<dyn UpdateDescriptor>> {
        self.inner
            .read()
            .await
            .available
            .iter()
            .find(|u| u.id() == update_package_id)
            .cloned()
    }

    #[must_use]
    pub async fn feedback<FeedbackModel: Any + Clone>(
        &self,
        update_package_id: &str,
    ) -> Option<Arc<FeedbackModel>> {
        #[expect(clippy::clone_on_ref_ptr)]
        let feedback2model = self.inner.read().await.feedback2model.clone();
        self.inner
            .read()
            .await
            .feedback
            .get(update_package_id)
            .and_then(|(_, rx)| {
                if let Some(feedback) = &*rx.borrow()
                    && let Ok(model) = feedback2model(feedback.as_ref())
                    && let Some(model) = model.downcast_ref::<FeedbackModel>()
                {
                    Some(Arc::new(model.clone()))
                } else {
                    None
                }
            })
    }

    #[must_use]
    pub async fn feedback_state<FeedbackModel: Any + Clone>(
        &self,
        update_package_id: &str,
    ) -> Option<UpdateFeedbackState<FeedbackModel>> {
        let inner = self.inner.read().await;
        if let Some((_, receiver)) = inner.feedback.get(update_package_id)
            && let Some(feedback) = receiver.borrow().as_ref()
            && let Ok(model) = (inner.feedback2model)(feedback.as_ref())
            && let Some(model) = model.downcast_ref::<FeedbackModel>()
        {
            return Some(UpdateFeedbackState::Feedback(Arc::new(model.clone())));
        }
        inner
            .active
            .as_ref()
            .filter(|active| active.id == update_package_id)
            .map(|active| UpdateFeedbackState::Pending(active.phase.clone()))
    }

    #[must_use]
    async fn provider(&self) -> Option<Arc<dyn UpdateProvider>> {
        self.inner.read().await.provider.clone()
    }

    /// Executes the update for the given update package ID.
    ///
    /// # Errors
    /// Returns an error if the update cannot be found, if the update provider
    /// is not configured, or if it conflicts another active update.
    pub async fn execute(&self, update_package_id: &str) -> Result<(), UpdateError> {
        self.start_update(
            update_package_id,
            Phase::Execute,
            UpdateError::UpdateExecutionInProgress,
            |provider, update, feedback| provider.execute(update, feedback),
        )
        .await
    }

    /// Prepares the update for the given update package ID.
    ///
    /// # Errors
    /// Returns an error if the update cannot be found or if the update provider
    /// is not configured.
    pub async fn prepare(&self, update_package_id: &str) -> Result<(), UpdateError> {
        self.start_update(
            update_package_id,
            Phase::Prepare,
            UpdateError::UpdatePreparationInProgress,
            |provider, update, feedback| provider.prepare(update, feedback),
        )
        .await
    }

    /// Performs the automated update for the given update package ID.
    ///
    /// # Errors
    /// Returns an error if the update cannot be found, if the update is not
    /// marked as automated, if the update provider is not configured, or if it
    /// conflicts another active update.
    pub async fn automated(&self, update_package_id: &str) -> Result<(), UpdateError> {
        let update = self
            .find(update_package_id)
            .await
            .ok_or_else(|| UpdateError::UpdateNotFound(update_package_id.to_string()))?;

        if update.automated() != Some(true) {
            return Err(UpdateError::AutomatedUpdateNotSupported);
        }

        self.start_update(
            update_package_id,
            Phase::Prepare,
            UpdateError::UpdateProcessInProgress,
            |provider, update, feedback| provider.automated(update, feedback),
        )
        .await
    }

    async fn start_update(
        &self,
        update_package_id: &str,
        phase: Phase,
        conflict: impl FnOnce(String) -> UpdateError,
        start: impl FnOnce(
            &dyn UpdateProvider,
            &dyn UpdateDescriptor,
            FeedbackSender,
        ) -> Result<(), UpdateError>,
    ) -> Result<(), UpdateError> {
        let update = self
            .find(update_package_id)
            .await
            .ok_or_else(|| UpdateError::UpdateNotFound(update_package_id.to_string()))?;
        let provider = self
            .provider()
            .await
            .ok_or(UpdateError::UpdateProviderNotConfigured)?;
        let (attempt, feedback, previous_feedback) = self
            .check_and_set_active(update_package_id, phase, conflict)
            .await?;

        if let Err(error) = start(provider.as_ref(), update.as_ref(), feedback) {
            self.release_active(&attempt, previous_feedback).await;
            return Err(error);
        }

        Ok(())
    }

    async fn release_active(
        &self,
        attempt: &Arc<()>,
        previous_feedback: Option<Arc<dyn UpdateFeedback>>,
    ) {
        let mut inner = self.inner.write().await;
        if inner
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(&active.attempt, attempt))
        {
            if let Some(previous_feedback) = previous_feedback {
                let id = inner.active.as_ref().map(|active| active.id.clone());
                if let Some(id) = id
                    && inner
                        .feedback
                        .get(&id)
                        .is_some_and(|(_, receiver)| receiver.borrow().is_none())
                {
                    let (sender, receiver) = watch::channel(Some(previous_feedback));
                    inner.feedback.insert(id, (sender, receiver));
                }
            }
            inner.active = None;
        }
    }

    /// Checks if an update is currently active and sets the given update as active if not.
    /// If another update is currently active, an `UpdateExecutionInProgress` error is returned.
    async fn check_and_set_active(
        &self,
        update_package_id: &str,
        phase: Phase,
        error: impl FnOnce(String) -> UpdateError,
    ) -> Result<(Arc<()>, FeedbackSender, Option<Arc<dyn UpdateFeedback>>), UpdateError> {
        let mut inner = self.inner.write().await;
        if !inner
            .available
            .iter()
            .any(|update| update.id() == update_package_id)
        {
            return Err(UpdateError::UpdateNotFound(update_package_id.to_string()));
        }
        if let Some(active) = &inner.active {
            let terminal = inner.feedback.get(&active.id).is_some_and(|(_, rx)| {
                rx.borrow().as_ref().is_some_and(|feedback| {
                    matches!(feedback.status(), Status::Completed | Status::Failed(_))
                })
            });
            if !terminal {
                return Err(error(active.id.clone()));
            }
        }
        let previous_feedback = inner
            .feedback
            .get(update_package_id)
            .and_then(|(_, receiver)| {
                receiver.borrow().as_ref().and_then(|feedback| {
                    matches!(feedback.status(), Status::Completed | Status::Failed(_))
                        .then(|| Arc::clone(feedback))
                })
            });
        let (feedback, receiver) = watch::channel(None);
        inner
            .feedback
            .insert(update_package_id.to_string(), (feedback.clone(), receiver));
        let attempt = Arc::new(());
        inner.active = Some(ActiveUpdate {
            id: update_package_id.to_string(),
            attempt: Arc::clone(&attempt),
            phase,
        });
        Ok((attempt, feedback, previous_feedback))
    }
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                active: None,
                request2update: Arc::new(|_stream| {
                    Box::pin(async { Err(UpdateError::UpdateProviderNotConfigured) })
                }),
                feedback2model: Arc::new(|_feedback| Err(UpdateError::UpdateProviderNotConfigured)),
                provider: None,
                available: Vec::new(),
                feedback: HashMap::new(),
            })),
        }
    }
}

pub struct UpdateRequest {
    pub stream: BoxStream<'static, Result<Bytes, Box<dyn std::error::Error + Send + Sync>>>,
    pub content_type: String,
    pub content_length: u64,
}

impl UpdateRequest {
    /// Attempts to collect the entire request body into a `BytesMut` buffer.
    ///
    /// # Errors
    ///
    /// Return an error when the request returns an error.
    pub async fn try_collect(mut self) -> Result<BytesMut, UpdateError> {
        let max_size: usize = self
            .content_length
            .try_into()
            .map_err(|e: TryFromIntError| UpdateError::ProviderError(e.to_string()))?;
        let mut received_size = 0;

        let mut collected = BytesMut::new();
        while received_size < max_size {
            let Some(chunk) = self.stream.next().await else {
                break;
            };
            let chunk = chunk.map_err(|e| UpdateError::ProviderError(e.to_string()))?;

            let bytes_to_take = chunk.len().min(max_size - received_size);
            collected.extend_from_slice(&chunk[..bytes_to_take]);
            received_size += bytes_to_take;
        }
        Ok(collected)
    }

    /// Attempts to parse the request body as JSON into the specified type.
    ///
    /// # Errors
    /// Returns an error if the content type is not `application/json` or if parsing fails.
    pub async fn try_json<T: serde::de::DeserializeOwned>(self) -> Result<T, UpdateError> {
        if self.content_type != "application/json" {
            return Err(UpdateError::ProviderError(
                "Invalid content type".to_string(),
            ));
        }
        let bytes = self.try_collect().await?;
        serde_json::from_slice(&bytes).map_err(|e| UpdateError::ProviderError(e.to_string()))
    }
}

#[async_trait::async_trait]
pub trait FromRequest {
    async fn from_request(data: UpdateRequest) -> Result<Self, UpdateError>
    where
        Self: Sized + 'static;
}

pub trait ToModel<Model> {
    fn to_model(&self) -> Model;
}

pub trait UpdateDescriptor: std::fmt::Debug + Sync + Send {
    fn as_any(&self) -> &dyn Any;

    fn id(&self) -> String;
    fn update_name(&self) -> String;
    fn size(&self) -> u64;

    fn automated(&self) -> Option<bool> {
        None
    }
    fn origin(&self) -> Option<Vec<String>> {
        None
    }
    fn update_translation_id(&self) -> Option<String> {
        None
    }
    fn notes(&self) -> Option<String> {
        None
    }
    fn notes_translation_id(&self) -> Option<String> {
        None
    }
    fn user_activity(&self) -> Option<String> {
        None
    }
    fn user_activity_translation_id(&self) -> Option<String> {
        None
    }
    fn preconditions(&self) -> Option<String> {
        None
    }
    fn preconditions_translation_id(&self) -> Option<String> {
        None
    }
    fn execution_conditions(&self) -> Option<String> {
        None
    }
    fn duration(&self) -> Option<u64> {
        None
    }
    fn updated_components(&self) -> Option<Vec<String>> {
        None
    }
    fn affected_components(&self) -> Option<Vec<String>> {
        None
    }
}

pub trait UpdateFeedback: std::fmt::Debug + Sync + Send {
    fn as_any(&self) -> &dyn Any;

    fn phase(&self) -> Phase;
    fn status(&self) -> Status;
    fn progress(&self) -> Option<u8> {
        None
    }
    fn subprogress(&self) -> Option<Vec<Box<dyn UpdateFeedback>>> {
        None
    }
    fn step(&self) -> Option<String> {
        None
    }
    fn step_translation_id(&self) -> Option<String> {
        None
    }
}

// UpdateProvider is the central trait that is implemented by the updater.
// The UpdateModel and FeedbackModel types are the deserialized JSON messages
// from the SOVD server. The internal representation for updates and feedback
// are defined by the updater but must implement the UpdateDescriptor and
// UpdateFeedback traits.
// Pending updates are managed by the SOVD server and the updater is called any
// time an update should be prepared or executed.
pub trait UpdateProvider: Send + Sync + 'static {
    /// Start prepare phase of an update. This may involve downloading necessary files,
    /// verifying integrity, and performing any other preparatory steps required before
    /// the update can be executed.
    ///
    /// # Errors
    /// todo
    fn prepare(
        &self,
        update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError>;
    /// Start execute phase of an update. This will perform the actual update process,
    ///
    /// # Errors
    /// todo
    fn execute(
        &self,
        update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError>;
    /// Start unattended update process. This will perform the prepare and execute phases of an update
    ///
    /// # Errors
    /// todo
    fn automated(
        &self,
        _update: &dyn UpdateDescriptor,
        _feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError>;
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::Notify;

    use super::*;

    #[derive(Debug, Clone)]
    struct TestUpdate {
        id: String,
    }

    #[async_trait::async_trait]
    impl FromRequest for TestUpdate {
        async fn from_request(_req: UpdateRequest) -> Result<Self, UpdateError> {
            Ok(TestUpdate {
                id: "test".to_string(),
            })
        }
    }

    impl UpdateDescriptor for TestUpdate {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn id(&self) -> String {
            self.id.clone()
        }

        fn update_name(&self) -> String {
            self.id.clone()
        }

        fn size(&self) -> u64 {
            0
        }
    }

    #[derive(Debug)]
    struct TestFeedback {
        phase: Phase,
        status: Status,
    }

    impl UpdateFeedback for TestFeedback {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn phase(&self) -> Phase {
            self.phase.clone()
        }

        fn status(&self) -> Status {
            self.status.clone()
        }
    }

    impl ToModel<()> for TestFeedback {
        fn to_model(&self) {}
    }

    #[derive(Clone, Copy)]
    enum TestProviderBehavior {
        FailFirstStart,
        ReportFailure,
        FailThenDelayRetry,
        FailSecondStart { completed: bool },
    }

    struct TestProvider {
        behavior: TestProviderBehavior,
        starts: AtomicUsize,
        retry_started: Arc<Notify>,
        continue_retry: Arc<Notify>,
    }

    impl TestProvider {
        fn start(&self, phase: Phase, feedback: &FeedbackSender) -> Result<(), UpdateError> {
            let start_number = self.starts.fetch_add(1, Ordering::SeqCst);
            if matches!(self.behavior, TestProviderBehavior::FailFirstStart) && start_number == 0 {
                return Err(UpdateError::ProviderError("start failed".to_string()));
            }
            if matches!(self.behavior, TestProviderBehavior::FailSecondStart { .. })
                && start_number == 1
            {
                return Err(UpdateError::ProviderError("retry failed".to_string()));
            }

            if matches!(self.behavior, TestProviderBehavior::FailThenDelayRetry) {
                if start_number == 0 {
                    return feedback
                        .send(Some(Arc::new(TestFeedback {
                            phase,
                            status: Status::Failed(UpdateError::ProviderError(
                                "update failed".to_string(),
                            )),
                        })))
                        .map_err(|_| {
                            UpdateError::ProviderError("feedback receiver closed".to_string())
                        });
                }

                let feedback = feedback.clone();
                let retry_started = Arc::clone(&self.retry_started);
                let continue_retry = Arc::clone(&self.continue_retry);
                tokio::spawn(async move {
                    retry_started.notify_one();
                    continue_retry.notified().await;
                    let _ = feedback.send(Some(Arc::new(TestFeedback {
                        phase,
                        status: Status::InProgress,
                    })));
                });
                return Ok(());
            }

            let status = match self.behavior {
                TestProviderBehavior::FailFirstStart
                | TestProviderBehavior::FailSecondStart { completed: true } => Status::Completed,
                TestProviderBehavior::ReportFailure => {
                    Status::Failed(UpdateError::ProviderError("update failed".to_string()))
                }
                TestProviderBehavior::FailThenDelayRetry => Status::Pending,
                TestProviderBehavior::FailSecondStart { completed: false } => {
                    Status::Failed(UpdateError::ProviderError("first failed".to_string()))
                }
            };
            feedback
                .send(Some(Arc::new(TestFeedback { phase, status })))
                .map_err(|_| UpdateError::ProviderError("feedback receiver closed".to_string()))
        }
    }

    impl UpdateProvider for TestProvider {
        fn prepare(
            &self,
            _update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.start(Phase::Prepare, &feedback)
        }

        fn execute(
            &self,
            _update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.start(Phase::Execute, &feedback)
        }

        fn automated(
            &self,
            _update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.start(Phase::Prepare, &feedback)
        }
    }

    async fn updates_with(behavior: TestProviderBehavior) -> Updates {
        updates_with_provider(TestProvider {
            behavior,
            starts: AtomicUsize::new(0),
            retry_started: Arc::new(Notify::new()),
            continue_retry: Arc::new(Notify::new()),
        })
        .await
    }

    async fn updates_with_provider(provider: impl UpdateProvider) -> Updates {
        let updates = Updates::new::<TestUpdate, TestFeedback, _>(provider);
        assert!(
            updates
                .push(TestUpdate {
                    id: "first".to_string(),
                })
                .await
                .is_ok()
        );
        assert!(
            updates
                .push(TestUpdate {
                    id: "second".to_string(),
                })
                .await
                .is_ok()
        );
        updates
    }

    async fn updates_with_delayed_retry() -> (Updates, Arc<Notify>, Arc<Notify>) {
        let retry_started = Arc::new(Notify::new());
        let continue_retry = Arc::new(Notify::new());
        let updates = updates_with_provider(TestProvider {
            behavior: TestProviderBehavior::FailThenDelayRetry,
            starts: AtomicUsize::new(0),
            retry_started: Arc::clone(&retry_started),
            continue_retry: Arc::clone(&continue_retry),
        })
        .await;
        (updates, retry_started, continue_retry)
    }

    #[tokio::test]
    async fn releases_active_slot_when_provider_fails_to_start() {
        let updates = updates_with(TestProviderBehavior::FailFirstStart).await;

        assert!(matches!(
            updates.execute("first").await,
            Err(UpdateError::ProviderError(_))
        ));
        assert!(updates.execute("second").await.is_ok());
    }

    #[tokio::test]
    async fn failed_retry_preserves_previous_terminal_feedback() {
        for completed in [false, true] {
            let updates = updates_with(TestProviderBehavior::FailSecondStart { completed }).await;
            assert!(updates.execute("first").await.is_ok());
            let previous_status = {
                let inner = updates.inner.read().await;
                inner.feedback.get("first").and_then(|(_, receiver)| {
                    receiver.borrow().as_ref().map(|feedback| feedback.status())
                })
            };
            assert!(matches!(
                updates.execute("first").await,
                Err(UpdateError::ProviderError(_))
            ));
            assert!(matches!(
                updates.feedback_state::<()>("first").await,
                Some(UpdateFeedbackState::Feedback(_))
            ));
            let restored_status = {
                let inner = updates.inner.read().await;
                inner.feedback.get("first").and_then(|(_, receiver)| {
                    receiver.borrow().as_ref().map(|feedback| feedback.status())
                })
            };
            assert_eq!(restored_status, previous_status);
            assert!(updates.execute("second").await.is_ok());
        }
    }

    #[tokio::test]
    async fn releases_active_slot_after_failed_feedback() {
        let updates = updates_with(TestProviderBehavior::ReportFailure).await;

        assert!(updates.execute("first").await.is_ok());
        assert!(updates.execute("second").await.is_ok());
    }

    #[tokio::test]
    async fn retrying_failed_update_keeps_competing_updates_blocked() {
        let (updates, retry_started, continue_retry) = updates_with_delayed_retry().await;

        assert!(updates.execute("first").await.is_ok());
        assert!(updates.execute("first").await.is_ok());
        retry_started.notified().await;

        assert!(matches!(
            updates.feedback_state::<()>("first").await,
            Some(UpdateFeedbackState::Pending(Phase::Execute))
        ));

        assert!(matches!(
            updates.execute("second").await,
            Err(UpdateError::UpdateExecutionInProgress(id)) if id == "first"
        ));

        assert!(!updates.remove("first").await);
        assert!(updates.find("first").await.is_some());

        continue_retry.notify_one();
    }

    struct StaleFeedbackProvider {
        starts: AtomicUsize,
        old_sender: Arc<Mutex<Option<FeedbackSender>>>,
    }

    impl UpdateProvider for StaleFeedbackProvider {
        fn prepare(
            &self,
            update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.execute(update, feedback)
        }

        fn execute(
            &self,
            _update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            if self.starts.fetch_add(1, Ordering::SeqCst) == 0 {
                *self
                    .old_sender
                    .lock()
                    .map_err(|error| UpdateError::ProviderError(error.to_string()))? =
                    Some(feedback.clone());
                feedback
                    .send(Some(Arc::new(TestFeedback {
                        phase: Phase::Execute,
                        status: Status::Failed(UpdateError::ProviderError(
                            "first failed".to_string(),
                        )),
                    })))
                    .map_err(|_| {
                        UpdateError::ProviderError("feedback receiver closed".to_string())
                    })?;
            }
            Ok(())
        }

        fn automated(
            &self,
            update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.execute(update, feedback)
        }
    }

    #[tokio::test]
    async fn stale_feedback_cannot_release_retry() {
        let old_sender = Arc::new(Mutex::new(None));
        let updates = updates_with_provider(StaleFeedbackProvider {
            starts: AtomicUsize::new(0),
            old_sender: Arc::clone(&old_sender),
        })
        .await;

        assert!(updates.execute("first").await.is_ok());
        assert!(updates.execute("first").await.is_ok());
        let old_sender = old_sender.lock().ok().and_then(|mut sender| sender.take());
        assert!(old_sender.is_some_and(|sender| {
            sender
                .send(Some(Arc::new(TestFeedback {
                    phase: Phase::Execute,
                    status: Status::Completed,
                })))
                .is_err()
        }));
        assert!(matches!(
            updates.feedback_state::<()>("first").await,
            Some(UpdateFeedbackState::Pending(Phase::Execute))
        ));
        assert!(matches!(
            updates.execute("second").await,
            Err(UpdateError::UpdateExecutionInProgress(id)) if id == "first"
        ));
    }

    #[tokio::test]
    async fn cannot_reserve_removed_update() {
        let updates = updates_with(TestProviderBehavior::ReportFailure).await;

        assert!(updates.remove("first").await);
        assert!(matches!(
            updates.check_and_set_active("first", Phase::Prepare, UpdateError::UpdatePreparationInProgress).await,
            Err(UpdateError::UpdateNotFound(id)) if id == "first"
        ));
    }

    struct RetryRaceProvider {
        starts: AtomicUsize,
        first_started: Arc<Notify>,
        finish_first: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl UpdateProvider for RetryRaceProvider {
        fn prepare(
            &self,
            update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.execute(update, feedback)
        }

        fn execute(
            &self,
            _update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            if self.starts.fetch_add(1, Ordering::SeqCst) == 0 {
                feedback
                    .send(Some(Arc::new(TestFeedback {
                        phase: Phase::Execute,
                        status: Status::Failed(UpdateError::ProviderError(
                            "first failed".to_string(),
                        )),
                    })))
                    .map_err(|_| {
                        UpdateError::ProviderError("feedback receiver closed".to_string())
                    })?;
                self.first_started.notify_one();
                self.finish_first
                    .lock()
                    .map_err(|error| UpdateError::ProviderError(error.to_string()))?
                    .recv()
                    .map_err(|error| UpdateError::ProviderError(error.to_string()))?;
                return Err(UpdateError::ProviderError("first failed".to_string()));
            }
            Ok(())
        }

        fn automated(
            &self,
            update: &dyn UpdateDescriptor,
            feedback: FeedbackSender,
        ) -> Result<(), UpdateError> {
            self.execute(update, feedback)
        }
    }

    #[tokio::test]
    async fn old_attempt_cleanup_does_not_release_retry() {
        let first_started = Arc::new(Notify::new());
        let (finish_first, first_can_finish) = std::sync::mpsc::channel();
        let retry_updates = updates_with_provider(RetryRaceProvider {
            starts: AtomicUsize::new(0),
            first_started: Arc::clone(&first_started),
            finish_first: Mutex::new(first_can_finish),
        })
        .await;

        let first_updates = retry_updates.clone();
        let first_call = tokio::task::spawn_blocking(move || {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .map(|runtime| runtime.block_on(first_updates.execute("first")))
        });
        first_started.notified().await;

        assert!(retry_updates.execute("first").await.is_ok());
        assert!(matches!(
            retry_updates.feedback_state::<()>("first").await,
            Some(UpdateFeedbackState::Pending(Phase::Execute))
        ));
        assert!(finish_first.send(()).is_ok());
        assert!(matches!(
            first_call.await,
            Ok(Ok(Err(UpdateError::ProviderError(_))))
        ));
        assert!(matches!(
            retry_updates.execute("second").await,
            Err(UpdateError::UpdateExecutionInProgress(id)) if id == "first"
        ));
    }
}
