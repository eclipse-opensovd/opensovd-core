// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::{any::Any, collections::HashMap};

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
type Model2Update =
    Arc<dyn Fn(&dyn Any) -> Result<Arc<dyn UpdateDescriptor>, UpdateError> + Send + Sync>;
type Feedback2Model =
    Arc<dyn Fn(&dyn UpdateFeedback) -> Result<Arc<dyn Any>, UpdateError> + Send + Sync>;

struct UpdatesInner {
    provider: Option<Arc<dyn UpdateProvider>>,
    available: Vec<Arc<dyn UpdateDescriptor>>,
    feedback: HashMap<String, (FeedbackSender, FeedbackReceiver)>,
    // active is used to ensure that only one update is executed at a time.
    // before an update is executed, it is set to the ID of that update.
    // check_and_set_active is used to check if the active update is still in
    // progress, and to set a new active update if none is active.
    active: Option<String>,
    // model2update stores the method UpdateModel -> UpdateImpl from the
    // UpdateDescriptor trait as a workaround since the actual UpdateImpl type
    // implementing it is erased from Updates.
    model2update: Model2Update,
    feedback2model: Feedback2Model,
}

#[derive(Clone)]
pub struct Updates {
    inner: Arc<RwLock<UpdatesInner>>,
}

impl Updates {
    #[must_use]
    pub fn new<UpdateImpl, UpdateModel, FeedbackImpl, FeedbackModel>(
        provider: impl UpdateProvider + 'static,
    ) -> Self
    where
        UpdateImpl: UpdateDescriptor + FromModel<UpdateModel> + 'static,
        FeedbackImpl: ToModel<FeedbackModel> + 'static,
        UpdateModel: 'static,
        FeedbackModel: 'static,
    {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                provider: Some(Arc::new(provider)),
                available: Vec::new(),
                feedback: HashMap::new(),
                active: None,
                model2update: Arc::new(|model| {
                    let model = model
                        .downcast_ref::<UpdateModel>()
                        .ok_or(UpdateError::IncompatibleModel)?;
                    Ok(Arc::new(UpdateImpl::from_model(model)))
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

    /// Adds an update to the list of available updates.
    ///
    /// # Errors
    ///
    /// Returns `UpdateProviderNotConfigured` if no update provider is
    /// configured.
    pub async fn push<UpdateModel: Any>(
        &self,
        update: &UpdateModel,
    ) -> Result<String, UpdateError> {
        let mut inner = self.inner.write().await;
        let item = (inner.model2update)(update)?;
        let id = item.id().clone();
        if inner.available.iter().any(|u| u.id() == id) {
            // the update id already exists
            return Err(UpdateError::UpdateIdConflict(id));
        }
        inner.available.push(item);
        Ok(id)
    }

    pub async fn remove(&self, update_package_id: &str) {
        let mut inner = self.inner.write().await;
        inner.available.retain(|u| u.id() != update_package_id);
        // also remove the feedback channel if it exists
        inner.feedback.remove(update_package_id);
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

    async fn feedback_sender(
        &self,
        update_package_id: &str,
    ) -> watch::Sender<Option<Arc<dyn UpdateFeedback>>> {
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
    async fn provider(&self) -> Option<Arc<dyn UpdateProvider>> {
        self.inner.read().await.provider.clone()
    }

    /// Executes the update for the given update package ID.
    ///
    /// # Errors
    /// Returns an error if the update cannot be found, if the update provider
    /// is not configured, or if it conflicts another active update.
    pub async fn execute(&self, update_package_id: &str) -> Result<(), UpdateError> {
        let update = self
            .find(update_package_id)
            .await
            .ok_or_else(|| UpdateError::UpdateNotFound(update_package_id.to_string()))?;

        let feedback = self.feedback_sender(update_package_id).await;
        self.check_and_set_active(update_package_id, UpdateError::UpdateExecutionInProgress)
            .await?;

        self.provider()
            .await
            .ok_or(UpdateError::UpdateProviderNotConfigured)?
            .execute(&*update, feedback)?;
        Ok(())
    }

    /// Prepares the update for the given update package ID.
    ///
    /// # Errors
    /// Returns an error if the update cannot be found or if the update provider
    /// is not configured.
    pub async fn prepare(&self, update_package_id: &str) -> Result<(), UpdateError> {
        let update = self
            .find(update_package_id)
            .await
            .ok_or_else(|| UpdateError::UpdateNotFound(update_package_id.to_string()))?;

        let feedback = self.feedback_sender(update_package_id).await;
        self.check_and_set_active(update_package_id, UpdateError::UpdatePreparationInProgress)
            .await?;

        self.provider()
            .await
            .ok_or(UpdateError::UpdateProviderNotConfigured)?
            .prepare(&*update, feedback)?;
        Ok(())
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

        let feedback = self.feedback_sender(update_package_id).await;
        self.check_and_set_active(update_package_id, UpdateError::UpdateProcessInProgress)
            .await?;

        self.provider()
            .await
            .ok_or(UpdateError::UpdateProviderNotConfigured)?
            .automated(&*update, feedback)?;
        Ok(())
    }

    /// Checks if an update is currently active and sets the given update as active if not.
    /// If another update is currently active, an `UpdateExecutionInProgress` error is returned.
    async fn check_and_set_active(
        &self,
        update_package_id: &str,
        error: impl FnOnce(String) -> UpdateError,
    ) -> Result<(), UpdateError> {
        let mut inner = self.inner.write().await;
        if let Some(active) = &inner.active
            && let Some((_, rx)) = inner.feedback.get(active)
        {
            if let Some(feedback) = rx.borrow().as_ref()
                && feedback.status() == Status::Completed
            {
                // if the feedback status is completed, it means the update is
                // no longer in progress, so we can continue and set a new
                // active update.
            } else {
                return Err(error(active.clone()));
            }
        }
        inner.active = Some(update_package_id.to_string());
        Ok(())
    }
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(UpdatesInner {
                active: None,
                model2update: Arc::new(|_model| Err(UpdateError::UpdateProviderNotConfigured)),
                feedback2model: Arc::new(|_feedback| Err(UpdateError::UpdateProviderNotConfigured)),
                provider: None,
                available: Vec::new(),
                feedback: HashMap::new(),
            })),
        }
    }
}

pub trait FromModel<Model> {
    fn from_model(model: &Model) -> Self
    where
        Self: Sized;
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
