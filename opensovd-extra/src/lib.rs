// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "unit")]
mod unit;

#[cfg(feature = "unit")]
pub use unit::{PhysicalDimension, Unit};

#[cfg(feature = "auth")]
pub mod auth;

#[cfg(feature = "auth")]
pub use auth::{JwtAuthenticator, RegorusAuthorizer};

#[cfg(feature = "federation")]
pub mod federation;

#[cfg(feature = "federation")]
pub use federation::{
    Federation, FederationLayer, ForwardRoute, ServerUri, SharedForwardRoute, Upstream,
};

#[cfg(feature = "http-proxy")]
pub mod http_proxy;

#[cfg(feature = "http-proxy")]
pub use http_proxy::UpstreamProxy;

#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "tls")]
pub use tls::{ClientAuth, ServerTlsConfig, TlsError};

#[cfg(feature = "trace")]
pub mod trace;

#[cfg(feature = "mdns")]
pub mod mdns;

#[cfg(feature = "mdns")]
pub use mdns::{Announcement, BrowseEvent, Browser, DiscoveredServer, MdnsAnnouncer, MdnsError};
