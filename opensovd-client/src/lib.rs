// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![expect(clippy::missing_errors_doc)]
#![doc = include_str!("../README.md")]

mod client;
mod data;
mod discovery;
pub mod entities;
mod error;
mod list;
mod log;
#[cfg(unix)]
mod unix;

pub use client::{BuilderError, Client, ClientBuilder};
pub use discovery::Discovery;
pub use error::{Error, Result};
pub use log::LogRequest;
pub use opensovd_models::data::DataCategory;
pub use opensovd_models::log::{LogConfigurationResponse, LogEntries, LogSeverity};
pub use opensovd_models::version::{SovdInfo, VendorInfo, VersionInfo};
pub use opensovd_models::{ErrorDetails, Response};
#[cfg(unix)]
pub use unix::UnixConnector;
