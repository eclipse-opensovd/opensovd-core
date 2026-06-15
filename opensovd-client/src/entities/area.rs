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

use opensovd_models::discovery::Entities;

use crate::client::Client;
use crate::error::Result;

/// A reference to a specific area.
pub struct Area<'a> {
    pub(crate) client: &'a Client,
    pub(crate) id: String,
}

impl Area<'_> {
    /// List entities contained in this area.
    pub async fn contains(&self) -> Result<Entities> {
        self.client
            .get(&format!("/areas/{}/contains", self.id), &[])
            .await
    }
}
