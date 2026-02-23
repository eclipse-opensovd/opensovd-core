// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
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
