// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, watch};

#[derive(Debug, thiserror::Error, Clone)]
pub enum UpdateError {
    #[error("creating response failed: {0}")]
    ResponseFailed(String),
    #[error("The package cannot be installed automatically")]
    AutomatedUpdateNotSupported,
    #[error("An update is already in preparation")]
    UpdateExecutionInProgress(String),
    #[error("UpdateProvider is not configured")]
    UpdateProviderNotConfigured,
    #[error("Update provider error: {0}")]
    ProviderError(String),
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

type FeedbackSender<FeedbackModel> = watch::Sender<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>>;
type FeedbackReceiver<FeedbackModel> =
    watch::Receiver<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>>;
type Model2Update<UpdateModel> =
    Arc<dyn Fn(&UpdateModel) -> Arc<dyn UpdateDescriptor<UpdateModel>> + Send + Sync>;

struct UpdatesInner<UpdateModel, FeedbackModel> {
    provider: Option<Arc<dyn UpdateProvider<UpdateModel, FeedbackModel>>>,
    available: Vec<Arc<dyn UpdateDescriptor<UpdateModel>>>,
    feedback: HashMap<
        String,
        (
            FeedbackSender<FeedbackModel>,
            FeedbackReceiver<FeedbackModel>,
        ),
    >,
    // model2update stores the method UpdateModel -> UpdateImpl from the
    // UpdateDescriptor trait as a workaround since the actual UpdateImpl type
    // implementing it is erased from Updates.
    model2update: Model2Update<UpdateModel>,
}

#[derive(Clone)]
pub struct Updates<UpdateModel, FeedbackModel> {
    inner: Arc<RwLock<UpdatesInner<UpdateModel, FeedbackModel>>>,
}

impl<UpdateModel: 'static, FeedbackModel: 'static> Updates<UpdateModel, FeedbackModel> {
    #[must_use]
    pub fn new<UpdateImpl, FeedbackImpl, Provider>(provider: Provider) -> Self
    where
        Provider: UpdateProvider<UpdateModel, FeedbackModel> + 'static,
        UpdateImpl: UpdateDescriptor<UpdateModel> + 'static,
        FeedbackImpl: UpdateFeedback<FeedbackModel> + 'static,
    {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                provider: Some(Arc::new(provider)),
                available: Vec::new(),
                feedback: HashMap::new(),
                model2update: Arc::new(|model| Arc::new(UpdateImpl::from_model(model))),
            })),
        }
    }

    pub async fn push(&self, update: &UpdateModel) {
        let mut inner = self.inner.write().await;
        let item = (inner.model2update)(update);
        inner.available.push(item);
    }

    pub async fn remove(&self, update_package_id: &str) {
        let mut inner = self.inner.write().await;
        inner.available.retain(|u| u.id() != update_package_id);
        // also remove the feedback channel if it exists
        inner.feedback.remove(update_package_id);
    }

    #[must_use]
    pub async fn available(&self) -> Vec<Arc<dyn UpdateDescriptor<UpdateModel>>> {
        self.inner.read().await.available.clone()
    }

    #[must_use]
    pub async fn find(
        &self,
        update_package_id: &str,
    ) -> Option<Arc<dyn UpdateDescriptor<UpdateModel>>> {
        self.inner
            .read()
            .await
            .available
            .iter()
            .find(|u| u.id() == update_package_id)
            .cloned()
    }

    pub async fn feedback_sender(
        &self,
        update_package_id: &str,
    ) -> watch::Sender<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>> {
        let mut inner = self.inner.write().await;
        if let Some((tx, _rx)) = inner.feedback.get(update_package_id) {
            tx.clone()
        } else {
            let (tx, rx) = watch::channel(None);
            inner
                .feedback
                .insert(update_package_id.to_string(), (tx.clone(), rx));
            tx
        }
    }

    #[must_use]
    pub async fn feedback(
        &self,
        update_package_id: &str,
    ) -> Option<Arc<dyn UpdateFeedback<FeedbackModel>>> {
        self.inner
            .read()
            .await
            .feedback
            .get(update_package_id)
            .and_then(|(_, rx)| rx.borrow().clone())
    }

    pub async fn all_feedback(
        &self,
    ) -> Vec<(String, Option<Arc<dyn UpdateFeedback<FeedbackModel>>>)> {
        self.inner
            .read()
            .await
            .feedback
            .iter()
            .map(|(id, (_tx, rx))| (id.clone(), rx.borrow().clone()))
            .collect::<Vec<_>>()
    }

    #[must_use]
    pub async fn provider(&self) -> Option<Arc<dyn UpdateProvider<UpdateModel, FeedbackModel>>> {
        self.inner.read().await.provider.clone()
    }
}

impl<UpdateModel, FeedbackModel> Default for Updates<UpdateModel, FeedbackModel> {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                model2update: Arc::new(|_model| panic!("no UpdateProvider configured")),
                provider: None,
                available: Vec::new(),
                feedback: HashMap::new(),
            })),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveUpdate {
    pub id: String,
    pub phase: Phase,
    pub progress: Option<u8>,
}

pub trait UpdateDescriptor<Model>: std::fmt::Debug + Sync + Send {
    fn as_any(&self) -> &dyn std::any::Any;
    fn from_model(model: &Model) -> Self
    where
        Self: Sized;

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

pub trait UpdateFeedback<Model>: std::fmt::Debug + Sync + Send {
    fn to_model(&self) -> Model;

    fn phase(&self) -> Phase;
    fn status(&self) -> Status;
    fn progress(&self) -> Option<u8> {
        None
    }
    fn subprogress(&self) -> Option<Vec<Box<dyn UpdateFeedback<Model>>>> {
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
pub trait UpdateProvider<UpdateModel: 'static, FeedbackModel: 'static>:
    Send + Sync + 'static
{
    /// Start prepare phase of an update. This may involve downloading necessary files,
    /// verifying integrity, and performing any other preparatory steps required before
    /// the update can be executed.
    ///
    /// # Errors
    /// todo
    fn prepare(
        &self,
        update: &dyn UpdateDescriptor<UpdateModel>,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>>,
    ) -> Result<(), UpdateError>;
    /// Start execute phase of an update. This will perform the actual update process,
    ///
    /// # Errors
    /// todo
    fn execute(
        &self,
        update: &dyn UpdateDescriptor<UpdateModel>,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>>,
    ) -> Result<(), UpdateError>;
    /// Start unattended update process. This will perform the prepare and execute phases of an update
    ///
    /// # Errors
    /// todo
    fn automated(
        &self,
        update: &dyn UpdateDescriptor<UpdateModel>,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback<FeedbackModel>>>>,
    ) -> Result<(), UpdateError> {
        self.prepare(update, feedback.clone())?;
        self.execute(update, feedback)
    }
}
