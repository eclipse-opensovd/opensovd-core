// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Core types for SOVD topology and data access.

mod bulkdata;
mod data;
mod entity;
mod log;
mod topology;
mod updates;

pub use bulkdata::{
    BulkData, BulkDataError, BulkDataMetadata, BulkDataProvider, CategoryFilter,
    DeletedBulkDataItem,
};
pub use data::{
    CategoryInfo, Data, DataError, DataFilter, DataProvider, DataScope, GroupInfo, Metadata,
    TagInfo,
};
pub use entity::{App, Area, Component, EntityKind, EntityRef};
pub use log::{
    LogConfiguration, LogContext, LogEntry, LogError, LogFilter, LogProvider, LogResult,
    LogSeverity, LogStream,
};
pub use topology::{Topology, TopologyError, TopologyEvent, TopologyReadGuard, TopologyWriteGuard};
pub use updates::{
    ActiveUpdate, FromModel, Phase, Status, ToModel, UpdateDescriptor, UpdateError, UpdateFeedback,
    UpdateProvider, Updates,
};
