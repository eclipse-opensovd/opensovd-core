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

//! Core types for SOVD topology and data access.

#![cfg_attr(all(test, coverage_nightly), feature(coverage_attribute))]

mod data;
mod discovery;
mod entity;
mod topology;

pub use data::{
    CategoryInfo, Data, DataError, DataFilter, DataProvider, DataScope, GroupInfo, Metadata,
    TagInfo,
};
pub use discovery::{DiscoveryError, DiscoveryProvider, DiscoveryStream};
pub use entity::{App, Area, Component, EntityCollection, EntityKind, EntityRef};
pub use topology::{Topology, TopologyError, TopologyEvent, TopologyReadGuard, TopologyWriteGuard};
