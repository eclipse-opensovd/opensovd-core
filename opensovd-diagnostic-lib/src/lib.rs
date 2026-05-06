// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Diagnostic Library
//!
//! A reusable library for HPC applications to expose diagnostic data via HTTP API.
//! This library provides a standardized interface for applications to expose their
//! diagnostic data, which can then be consumed by SOVD servers or visualization tools.

mod data;
mod error;
mod provider;
pub mod registration;
mod server;
pub mod streaming;

pub use data::{ApiInfo, DataCategory, DataItem, DataValue, HealthResponse, WriteDataRequest};
pub use error::DiagnosticError;
pub use provider::{AppInfo, DataProvider};
pub use registration::{AppEndpoint, AppRegistrar, HttpRegistrar};
pub use server::DiagnosticServer;
pub use streaming::{SseStream, StreamConfig, StreamEvent, StreamingDataProvider};

/// Result type for diagnostic operations
pub type Result<T> = std::result::Result<T, DiagnosticError>;
