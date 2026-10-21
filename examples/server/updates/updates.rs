// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::expect_used, clippy::indexing_slicing)]

//! Update example with a JSON payload stored in bulk-data.
//!
//! Uploads a JSON payload to an in-memory bulk-data store under
//! `bulk-data://updates/firmware-v1`, registers an update that references it,
//! and resolves the payload during `prepare`/`execute` — printing each message
//! with a 5-second delay to simulate work.
//!
//! Run with: `cargo run -p opensovd-examples-server --example updates`

use std::sync::Arc;

use futures::StreamExt as _;
use opensovd_core::{
    App, Component, FromModel, Phase, Status, UpdateDescriptor, UpdateError, UpdateFeedback,
    UpdateProvider,
};
use opensovd_core::{BulkDataProvider, ToModel};
use opensovd_mocks::InMemoryBulkDataProvider;
use opensovd_models::updates::{
    Phase as ModelPhase, Status as ModelStatus, UpdateDetail, UpdateStatus,
};
use opensovd_server::{Server, Topology};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::sync::watch;
use tokio::time::{Duration, sleep};

const COMPONENT_ID: &str = "update-host";
const APP_ID: &str = "update-example";

#[derive(Debug, Deserialize)]
struct UpdatePayload {
    prepare: Vec<String>,
    execute: Vec<String>,
}

#[derive(Debug)]
enum Payload {
    Unresolved(String),
    Downloaded(Arc<UpdatePayload>),
}

// ── Update descriptor ────────────────────────────────────────────────────────

#[derive(Debug)]
struct Update {
    id: String,
    name: String,
    size: usize,
    payload: Arc<RwLock<Vec<Payload>>>,
}

impl FromModel<UpdateDetail> for Update {
    fn from_model(model: &UpdateDetail) -> Self {
        let mut payload = Vec::new();
        for target in &model.targets {
            payload.push(Payload::Unresolved(target.file.clone()));
        }

        Update {
            id: model
                .id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: model.update_name.clone(),
            #[expect(clippy::cast_possible_truncation)]
            size: model.size as usize,
            payload: Arc::new(RwLock::new(payload)),
        }
    }
}

impl UpdateDescriptor for Update {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn update_name(&self) -> String {
        self.name.clone()
    }

    fn size(&self) -> u64 {
        self.size as u64
    }
}

// ── Feedback ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Feedback {
    phase: Phase,
    status: Status,
    progress: Option<u8>,
    step: Option<String>,
}

impl UpdateFeedback for Feedback {
    fn phase(&self) -> Phase {
        self.phase.clone()
    }

    fn status(&self) -> Status {
        self.status.clone()
    }

    fn progress(&self) -> Option<u8> {
        self.progress
    }

    fn step(&self) -> Option<String> {
        self.step.clone()
    }
}

impl ToModel<UpdateStatus> for Feedback {
    fn to_model(feedback: &dyn UpdateFeedback) -> UpdateStatus {
        let phase = match feedback.phase() {
            Phase::Prepare => ModelPhase::Prepare,
            Phase::Execute => ModelPhase::Execute,
        };
        let status = match &feedback.status() {
            Status::Pending => ModelStatus::Pending,
            Status::InProgress => ModelStatus::InProgress,
            Status::Failed(_) => ModelStatus::Failed,
            Status::Completed => ModelStatus::Completed,
        };
        UpdateStatus {
            phase,
            status,
            progress: feedback.progress(),
            step: feedback.step(),
            subprogress: None,
            step_translation_id: None,
            error: None,
        }
    }
}

// ── Provider ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Provider {
    store: Arc<InMemoryBulkDataProvider>,
}

impl Provider {
    /// Strips `bulk-data://` and splits on the first `/` into (category, id).
    fn parse_url(url: &str) -> (&str, &str) {
        let path = url.strip_prefix("bulk-data://").unwrap_or(url);
        path.split_once('/').unwrap_or((path, ""))
    }

    #[expect(clippy::print_stdout, clippy::arithmetic_side_effects)]
    fn run_phase(
        phase: Phase,
        messages: Vec<String>,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) {
        tokio::spawn(async move {
            let total = messages.len();
            for (i, msg) in messages.iter().enumerate() {
                println!("{msg}");
                let _ = feedback.send(Some(Arc::new(Feedback {
                    phase: phase.clone(),
                    status: Status::InProgress,
                    #[expect(clippy::cast_possible_truncation)]
                    progress: Some((i * 100 / total) as u8),
                    step: Some(msg.clone()),
                })));
                sleep(Duration::from_secs(5)).await;
            }
            let _ = feedback.send(Some(Arc::new(Feedback {
                phase: phase.clone(),
                status: Status::Completed,
                progress: Some(100),
                step: None,
            })));
        });
    }

    #[expect(clippy::arithmetic_side_effects)]
    async fn fetch_payload(
        store: &InMemoryBulkDataProvider,
        url: &str,
        remaining: &mut usize,
    ) -> Result<UpdatePayload, UpdateError> {
        let (category, id) = Self::parse_url(url);
        let mut bulk = store
            .bulk_download(category, id)
            .await
            .map_err(|e| UpdateError::ProviderError(e.to_string()))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = bulk.data.next().await {
            let chunk = chunk.map_err(|e| UpdateError::ProviderError(e.to_string()))?;
            if chunk.len() >= *remaining {
                bytes.extend_from_slice(&chunk[..*remaining]);
                break;
            }
            bytes.extend_from_slice(&chunk);
            *remaining -= chunk.len();
        }
        serde_json::from_slice(&bytes).map_err(|e| UpdateError::ProviderError(e.to_string()))
    }
}

impl UpdateProvider for Provider {
    fn prepare(
        &self,
        update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        let update = update
            .as_any()
            .downcast_ref::<Update>()
            .ok_or_else(|| UpdateError::ProviderError("unexpected update type".into()))?;
        let mut remaining_size = usize::try_from(update.size())
            .map_err(|_| UpdateError::ProviderError("update size too large".into()))?;
        let payload = Arc::clone(&update.payload);
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            for payload in payload.write().await.iter_mut() {
                if let Payload::Unresolved(url) = payload {
                    match Self::fetch_payload(&store, url.as_str(), &mut remaining_size).await {
                        Ok(fetched) => {
                            let fetched = Arc::new(fetched);
                            *payload = Payload::Downloaded(Arc::clone(&fetched));
                        }
                        Err(e) => {
                            let _ = feedback.send(Some(Arc::new(Feedback {
                                phase: Phase::Prepare,
                                status: Status::Failed(e),
                                progress: None,
                                step: None,
                            })));
                            return;
                        }
                    }
                }
            }

            for payload in payload.read().await.iter() {
                if let Payload::Downloaded(fetched) = payload {
                    let messages = fetched.prepare.clone();
                    Self::run_phase(Phase::Prepare, messages, feedback.clone());
                }
            }

            let _ = feedback.send(Some(Arc::new(Feedback {
                phase: Phase::Prepare,
                status: Status::Completed,
                progress: Some(100),
                step: None,
            })));
        });
        Ok(())
    }

    fn execute(
        &self,
        update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        let update = update
            .as_any()
            .downcast_ref::<Update>()
            .ok_or_else(|| UpdateError::ProviderError("unexpected update type".into()))?;

        let payload = Arc::clone(&update.payload);
        tokio::spawn(async move {
            for payload in payload.read().await.iter() {
                if let Payload::Downloaded(fetched) = payload {
                    let messages = fetched.execute.clone();
                    Self::run_phase(Phase::Execute, messages, feedback.clone());
                }
            }

            let _ = feedback.send(Some(Arc::new(Feedback {
                phase: Phase::Execute,
                status: Status::Completed,
                progress: Some(100),
                step: None,
            })));
        });
        Ok(())
    }

    fn automated(
        &self,
        update: &dyn UpdateDescriptor,
        feedback: watch::Sender<Option<Arc<dyn UpdateFeedback>>>,
    ) -> Result<(), UpdateError> {
        let update = update
            .as_any()
            .downcast_ref::<Update>()
            .ok_or_else(|| UpdateError::ProviderError("unexpected update type".into()))?;
        let mut remaining_size = usize::try_from(update.size())
            .map_err(|_| UpdateError::ProviderError("update size too large".into()))?;
        let payload = Arc::clone(&update.payload);
        let store = Arc::clone(&self.store);

        tokio::spawn(async move {
            for payload in payload.write().await.iter_mut() {
                if let Payload::Unresolved(url) = payload {
                    match Self::fetch_payload(&store, url.as_str(), &mut remaining_size).await {
                        Ok(fetched) => {
                            let fetched = Arc::new(fetched);
                            *payload = Payload::Downloaded(Arc::clone(&fetched));
                        }
                        Err(e) => {
                            let _ = feedback.send(Some(Arc::new(Feedback {
                                phase: Phase::Prepare,
                                status: Status::Failed(e),
                                progress: None,
                                step: None,
                            })));
                            return;
                        }
                    }
                }
            }

            for payload in payload.read().await.iter() {
                if let Payload::Downloaded(fetched) = payload {
                    let messages = fetched.prepare.clone();
                    Self::run_phase(Phase::Prepare, messages, feedback.clone());
                }
            }

            for payload in payload.read().await.iter() {
                if let Payload::Downloaded(fetched) = payload {
                    let messages = fetched.execute.clone();
                    Self::run_phase(Phase::Execute, messages, feedback.clone());
                }
            }

            let _ = feedback.send(Some(Arc::new(Feedback {
                phase: Phase::Execute,
                status: Status::Completed,
                progress: Some(100),
                step: None,
            })));
        });
        Ok(())
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let store = Arc::new(InMemoryBulkDataProvider::default());

    // Register the app with the shared bulk-data store in the topology.
    let component = Component::new(COMPONENT_ID, "Update Host");
    let app = App::new(APP_ID, COMPONENT_ID).with_bulkdata_provider(store.as_ref().clone());

    let topology = Topology::new();
    {
        let mut guard = topology.write().await;
        guard.add_component(component);
        guard.add_app(app);
        guard.add_update_provider::<Update, UpdateDetail, Feedback, UpdateStatus>(Provider {
            store,
        });
    }

    let listener = TcpListener::bind("127.0.0.1:7690").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(opensovd_extra::trace::server_layer())
        .build()?;

    tracing::info!("Server running on http://127.0.0.1:7690/sovd/v1");
    server.serve().await?;
    Ok(())
}
