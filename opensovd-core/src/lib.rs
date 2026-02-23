// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Core types for SOVD topology and data access.

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod data;
mod discovery;
mod entity;
mod topology;

pub use data::{
    CategoryInfo, Data, DataError, DataFilter, DataProvider, GroupInfo, Metadata, TagInfo,
};
pub use discovery::{DiscoveryError, DiscoveryProvider, DiscoveryStream};
pub use entity::{App, Area, Component, EntityCollection, EntityKind, EntityRef};
pub use topology::{Topology, TopologyError, TopologyEvent, TopologyReadGuard, TopologyWriteGuard};
