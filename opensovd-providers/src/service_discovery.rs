// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Transport-agnostic lifecycle traits for SOVD service discovery.

use opensovd_core::DiscoveryProvider;

/// Starts a transport-specific mechanism for discovering SOVD services.
///
/// Implement this trait for discovery mechanisms such as mDNS, IPC, or D-Bus.
/// The resulting session owns the transport lifecycle and supplies the gateway
/// with a standard [`DiscoveryProvider`].
pub trait ServiceDiscovery {
    /// The running session returned by this discovery mechanism.
    type Session: ServiceDiscoverySession;

    /// The error returned when the discovery mechanism cannot start.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Starts service discovery.
    ///
    /// # Errors
    ///
    /// Returns a transport-specific error when service discovery cannot start.
    fn start(&self) -> Result<Self::Session, Self::Error>;
}

/// A running service discovery mechanism.
pub trait ServiceDiscoverySession {
    /// The error returned while stopping this discovery mechanism.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the provider that supplies topology updates to the gateway.
    fn discovery_provider(&self) -> Box<dyn DiscoveryProvider>;

    /// Stops service discovery and releases any transport resources.
    ///
    /// # Errors
    ///
    /// Returns a transport-specific error when service discovery cannot stop cleanly.
    fn shutdown(&self) -> Result<(), Self::Error>;
}