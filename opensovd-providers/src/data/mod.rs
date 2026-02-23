// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data provider building blocks.
//!
//! This module provides traits and implementations for constructing SOVD data providers.

use serde::{Deserialize, Serialize};

mod builder;
mod constant;
mod resource;

pub use builder::{BuildError, BuiltDataProvider, DataProviderBuilder};
pub use constant::{Constant, ConstantError};
pub use resource::{DataResource, ReadableDataResource, WriteableDataResource};

/// SOVD value envelope that wraps a scalar `T` in `{"value": <T>}`.
///
/// Use this instead of raw scalars so the JSON representation always
/// conforms to the SOVD object envelope required by ISO 17978-3.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Value<T> {
    /// The wrapped value.
    pub value: T,
}

impl<T> Value<T> {
    /// Create a new `Value` wrapping the given inner value.
    pub fn new(value: T) -> Self {
        Self { value }
    }
}
