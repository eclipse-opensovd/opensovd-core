// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
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
