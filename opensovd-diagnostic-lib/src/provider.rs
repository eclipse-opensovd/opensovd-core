// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data provider trait for applications to implement

use async_trait::async_trait;
use crate::{DataItem, DataValue, Result};

/// Trait that applications must implement to provide diagnostic data
/// 
/// This trait defines the interface for exposing diagnostic data from an application.
/// Applications implement this trait to provide their specific data items and handle
/// read/write operations.
#[async_trait]
pub trait DataProvider: Send + Sync {
    /// List all available data items
    /// 
    /// Returns a vector of metadata describing all diagnostic data items
    /// that this application exposes.
    async fn list_data(&self) -> Vec<DataItem>;
    
    /// Read a specific data item by ID
    /// 
    /// # Arguments
    /// * `data_id` - The unique identifier of the data item to read
    /// 
    /// # Returns
    /// The current value of the data item, or an error if not found or not readable
    async fn read_data(&self, data_id: &str) -> Result<DataValue>;
    
    /// Write a value to a specific data item
    /// 
    /// # Arguments
    /// * `data_id` - The unique identifier of the data item to write
    /// * `value` - The new value to write
    /// 
    /// # Returns
    /// Ok(()) if successful, or an error if the item is not writable or the value is invalid
    async fn write_data(&self, data_id: &str, value: serde_json::Value) -> Result<()>;
    
    /// Get application information (optional)
    /// 
    /// Returns metadata about the application itself. Default implementation
    /// returns basic information.
    async fn get_app_info(&self) -> AppInfo {
        AppInfo {
            name: "HPC Application".to_string(),
            version: "1.0.0".to_string(),
            description: "Diagnostic-enabled HPC application".to_string(),
        }
    }
}

/// Application information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}