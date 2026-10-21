// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use opensovd_core::{
    FromModel, ToModel, UpdateDescriptor, UpdateError, UpdateFeedback, UpdateProvider,
};
use opensovd_models::updates::{UpdateDetail, UpdateStatus};
use tokio::{sync::watch, time::sleep};

#[derive(Debug)]
pub struct MockUpdate {
    detail: UpdateDetail,
}

impl UpdateDescriptor for MockUpdate {
    fn id(&self) -> String {
        self.detail.id.clone().unwrap_or("mock-update".to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_name(&self) -> String {
        self.detail.update_name.clone()
    }

    fn size(&self) -> u64 {
        self.detail.size
    }
}

impl FromModel<UpdateDetail> for MockUpdate {
    fn from_model(update_detail: &UpdateDetail) -> Self {
        Self {
            detail: update_detail.clone(),
        }
    }
}

#[derive(Debug)]
pub struct MockFeedback {
    phase: opensovd_core::Phase,
    status: opensovd_core::Status,
}

impl UpdateFeedback for MockFeedback {
    fn phase(&self) -> opensovd_core::Phase {
        self.phase.clone()
    }

    fn status(&self) -> opensovd_core::Status {
        self.status.clone()
    }
}

impl ToModel<UpdateStatus> for MockFeedback {
    fn to_model(update_feedback: &dyn UpdateFeedback) -> UpdateStatus {
        UpdateStatus {
            phase: match update_feedback.phase() {
                opensovd_core::Phase::Prepare => opensovd_models::updates::Phase::Prepare,
                opensovd_core::Phase::Execute => opensovd_models::updates::Phase::Execute,
            },
            status: match update_feedback.status() {
                opensovd_core::Status::Pending => opensovd_models::updates::Status::Pending,
                opensovd_core::Status::InProgress => opensovd_models::updates::Status::InProgress,
                opensovd_core::Status::Completed => opensovd_models::updates::Status::Completed,
                opensovd_core::Status::Failed(_) => opensovd_models::updates::Status::Failed,
            },
            progress: update_feedback.progress(),
            subprogress: None,
            step: update_feedback.step(),
            step_translation_id: update_feedback.step_translation_id(),
            error: None,
        }
    }
}

#[derive(Debug)]
pub struct MockUpdateProvider;

impl UpdateProvider for MockUpdateProvider {
    fn prepare(
        &self,
        _update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<std::sync::Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        tokio::spawn(async move {
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Prepare,
                    status: opensovd_core::Status::Pending,
                })))
                .ok();
            sleep(Duration::from_secs(1)).await;
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Prepare,
                    status: opensovd_core::Status::Completed,
                })))
                .ok();
        });
        Ok(())
    }

    fn execute(
        &self,
        _update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<std::sync::Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        tokio::spawn(async move {
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Execute,
                    status: opensovd_core::Status::Pending,
                })))
                .ok();
            sleep(Duration::from_secs(1)).await;
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Execute,
                    status: opensovd_core::Status::Completed,
                })))
                .ok();
        });
        Ok(())
    }

    fn automated(
        &self,
        _update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<std::sync::Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        tokio::spawn(async move {
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Prepare,
                    status: opensovd_core::Status::Pending,
                })))
                .ok();
            sleep(Duration::from_secs(1)).await;
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Prepare,
                    status: opensovd_core::Status::Completed,
                })))
                .ok();
            sleep(Duration::from_secs(1)).await;
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Execute,
                    status: opensovd_core::Status::Pending,
                })))
                .ok();
            sleep(Duration::from_secs(1)).await;
            feedback
                .send(Some(std::sync::Arc::new(MockFeedback {
                    phase: opensovd_core::Phase::Execute,
                    status: opensovd_core::Status::Completed,
                })))
                .ok();
        });
        Ok(())
    }
}
