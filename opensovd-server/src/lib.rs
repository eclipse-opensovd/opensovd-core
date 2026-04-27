// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod auth;
mod body;
mod connect_info;
mod routes;
mod schema;
mod server;
#[cfg(feature = "tls")]
pub mod tls;

pub use ::http::request::Parts;
pub use auth::{
    AllowAll, AuthError, AuthenticationLayer, Authenticator, AuthorizationLayer, Authorizer,
    Identity, NoAuth,
};
pub use body::Body;
#[cfg(unix)]
pub use connect_info::UdsConnectInfo;
pub use connect_info::{ConnectInfo, TcpConnectInfo};
pub use opensovd_core::{DataProvider, Topology};
pub use opensovd_models::version::VendorInfo;
pub use schema::JsonSchema;
pub use server::{BuilderError, Listener, Server, ServerBuilder};
#[cfg(feature = "tls")]
pub use tls::{TlsConfig, TlsConfigError, TlsListener};
