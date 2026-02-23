// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Discovery provider trait for finding remote SOVD servers.
//!
//! Implementors browse the network (e.g. via mDNS-SD), connect to discovered
//! servers, fetch their entities, and yield `(remove, add)` tuples as servers
//! appear and disappear.

use std::pin::Pin;

use futures_core::Stream;

use crate::{EntityCollection, EntityRef};

/// Errors that can occur during discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The underlying transport or protocol failed.
    #[error("discovery transport error: {0}")]
    Transport(String),

    /// A discovered server returned an invalid or unparseable response.
    #[error("invalid response from discovered server: {0}")]
    InvalidResponse(String),

    /// A generic boxed error for provider-specific failures.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// A `Result` alias where the `Err` variant is [`DiscoveryError`].
pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// A pinned, boxed stream of discovery diffs.
///
/// Each item is a `(remove, add)` tuple: entity refs to remove from the
/// topology, followed by new entities to add.
pub type DiscoveryStream =
    Pin<Box<dyn Stream<Item = Result<(Vec<EntityRef>, EntityCollection)>> + Send>>;

/// A provider that discovers remote SOVD servers and yields topology diffs.
///
/// Implementations should return a long-lived stream that emits
/// `(remove, add)` tuples. The first element contains refs of entities to
/// remove; the second contains new entities to add.
#[async_trait::async_trait]
pub trait DiscoveryProvider: Send + Sync {
    /// Start discovery and return a stream of events.
    ///
    /// The returned stream runs until it is dropped or an unrecoverable error
    /// occurs. Transient failures should be logged internally and retried
    /// rather than terminating the stream.
    async fn discover(&self) -> Result<DiscoveryStream>;
}
