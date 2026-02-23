// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![allow(clippy::missing_errors_doc)]

mod client;
mod data;
pub mod entities;
mod error;
mod list;
#[cfg(unix)]
mod unix;

pub use client::{BuilderError, Client, ClientBuilder};
pub use error::{Error, Result};
pub use opensovd_models::Response;
