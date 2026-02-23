// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data resource traits for building data providers.

use async_trait::async_trait;
use opensovd_core::DataError;
use schemars::Schema;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Read-only data resource.
///
/// Implement this trait to provide read access to a data value.
#[async_trait]
pub trait ReadableDataResource: Send + Sync + 'static {
    /// The value type returned by [`read()`](Self::read).
    ///
    /// Also used to derive the default JSON schema via [`schemars::JsonSchema`].
    type Value: Serialize + schemars::JsonSchema + Send;

    /// Get the JSON schema for this resource.
    ///
    /// Default implementation uses `schema_for!(Self::Value)`.
    fn schema(&self) -> Schema {
        schemars::schema_for!(Self::Value)
    }

    /// Read the current value of this data resource.
    async fn read(&self) -> Result<Self::Value, DataError>;
}

/// Write-only data resource.
///
/// Implement this trait to provide write access to a data value.
#[async_trait]
pub trait WriteableDataResource: Send + Sync + 'static {
    /// The value type accepted by [`write()`](Self::write).
    ///
    /// Also used to derive the default JSON schema via [`schemars::JsonSchema`].
    type Value: DeserializeOwned + schemars::JsonSchema + Send;

    /// Get the JSON schema for this resource.
    ///
    /// Default implementation uses `schema_for!(Self::Value)`.
    fn schema(&self) -> Schema {
        schemars::schema_for!(Self::Value)
    }

    /// Write a new value to this data resource.
    async fn write(&self, value: &Self::Value) -> Result<(), DataError>;
}

/// Combined read-write data resource (convenience supertrait).
///
/// Anything implementing both [`ReadableDataResource`] and [`WriteableDataResource`]
/// automatically implements this trait via blanket implementation.
pub trait DataResource: ReadableDataResource + WriteableDataResource {}

// Blanket impl: anything implementing both traits automatically implements DataResource
impl<T: ReadableDataResource + WriteableDataResource> DataResource for T {}
