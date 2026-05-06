// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data streaming support for real-time diagnostic data
//!
//! This module provides Server-Sent Events (SSE) streaming capabilities
//! for applications that need to push real-time data updates to clients.

use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;

/// A stream event containing diagnostic data updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    /// Data item ID
    pub data_id: String,
    
    /// Updated value
    pub value: serde_json::Value,
    
    /// Timestamp (Unix epoch in milliseconds)
    pub timestamp: u64,
    
    /// Optional event type (update, error, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
}

/// Stream configuration
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Update interval in milliseconds
    pub interval_ms: u64,
    
    /// Data IDs to stream
    pub data_ids: Vec<String>,
    
    /// Maximum number of concurrent subscribers
    pub max_subscribers: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            interval_ms: 100, // 10 Hz default
            data_ids: Vec::new(),
            max_subscribers: 100,
        }
    }
}

/// Broadcast channel for streaming events
pub type StreamSender = broadcast::Sender<StreamEvent>;
pub type StreamReceiver = broadcast::Receiver<StreamEvent>;

/// Create a new streaming channel
pub fn create_stream_channel(capacity: usize) -> (StreamSender, StreamReceiver) {
    broadcast::channel(capacity)
}

/// SSE stream wrapper used by `DiagnosticServer` for the `/api/stream` endpoint.
pub struct SseStream {
    receiver: StreamReceiver,
}

impl SseStream {
    pub fn new(receiver: StreamReceiver) -> Self {
        Self { receiver }
    }
}

impl Stream for SseStream {
    type Item = Result<warp::sse::Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.try_recv() {
            Ok(event) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                let sse_event = warp::sse::Event::default()
                    .event(event.event_type.as_deref().unwrap_or("update"))
                    .data(data);
                Poll::Ready(Some(Ok(sse_event)))
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // Register waker for when data is available
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                // Client is too slow, send error event
                let error_event = warp::sse::Event::default()
                    .event("error")
                    .data("stream lagged");
                Poll::Ready(Some(Ok(error_event)))
            }
            Err(broadcast::error::TryRecvError::Closed) => Poll::Ready(None),
        }
    }
}

/// Optional trait for data providers that want to push updates themselves.
///
/// Not required for the built-in `/api/stream` endpoint — that uses
/// `create_periodic_stream` to poll `DataProvider::read_data` automatically.
/// Implement this only if your app needs to drive the stream directly.
#[async_trait::async_trait]
pub trait StreamingDataProvider: Send + Sync {
    /// Start streaming data updates, returning a sender the caller can push events to.
    async fn start_streaming(&self, config: StreamConfig) -> Result<StreamSender, String>;

    /// Return the data IDs this provider can stream.
    async fn streaming_data_ids(&self) -> Vec<String>;
}

/// Helper to create periodic streaming from a data provider
pub async fn create_periodic_stream<F, Fut>(
    data_ids: Vec<String>,
    interval_ms: u64,
    mut fetch_fn: F,
) -> StreamSender
where
    F: FnMut(String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<serde_json::Value, String>> + Send,
{
    let (tx, _rx) = create_stream_channel(1000);
    let tx_clone = tx.clone();
    
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(interval_ms));
        
        loop {
            interval.tick().await;
            
            for data_id in &data_ids {
                match fetch_fn(data_id.clone()).await {
                    Ok(value) => {
                        let event = StreamEvent {
                            data_id: data_id.clone(),
                            value,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            event_type: Some("update".to_string()),
                        };
                        
                        // Ignore send errors (no subscribers)
                        let _ = tx_clone.send(event);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch {} for streaming: {}", data_id, e);
                    }
                }
            }
        }
    });
    
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.interval_ms, 100);
        assert_eq!(config.max_subscribers, 100);
    }

    #[test]
    fn test_stream_event_serialization() {
        let event = StreamEvent {
            data_id: "test.data".to_string(),
            value: serde_json::json!({"temp": 25.5}),
            timestamp: 1234567890,
            event_type: Some("update".to_string()),
        };
        
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("test.data"));
        assert!(json.contains("25.5"));
    }

    #[tokio::test]
    async fn test_create_stream_channel() {
        let (tx, mut rx) = create_stream_channel(10);
        
        let event = StreamEvent {
            data_id: "test".to_string(),
            value: serde_json::json!(42),
            timestamp: 0,
            event_type: None,
        };
        
        tx.send(event.clone()).unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.data_id, "test");
    }
}