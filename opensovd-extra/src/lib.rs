// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(feature = "unit")]
mod unit;

#[cfg(feature = "unit")]
pub use unit::{PhysicalDimension, Unit};

#[cfg(feature = "auth")]
pub mod auth;

#[cfg(feature = "auth")]
pub use auth::{JwtAlgorithm, JwtAuthenticator, RegorusAuthorizer};
