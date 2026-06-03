// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::missing_errors_doc)]
#![doc = include_str!("../README.md")]

mod client;
mod data;
mod discovery;
pub mod entities;
mod error;
mod list;
#[cfg(unix)]
mod unix;

pub use client::{BuilderError, Client, ClientBuilder};
pub use discovery::Discovery;
pub use error::{Error, Result};
pub use opensovd_models::Response;
pub use opensovd_models::version::{SovdInfo, VendorInfo, VersionInfo};
