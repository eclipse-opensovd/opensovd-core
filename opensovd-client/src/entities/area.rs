// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::client::Client;
use crate::error::Result;
use crate::models::{DefaultModels, Models};

/// A reference to a specific area.
pub struct Area<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) id: String,
}

impl<M: Models> Area<'_, M> {
    /// List entities contained in this area.
    pub async fn contains(&self) -> Result<M::EntitiesResponse> {
        self.client
            .get(&format!("/areas/{}/contains", self.id), &[])
            .await
    }
}
