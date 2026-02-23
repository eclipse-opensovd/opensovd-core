// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used)]

use mock_http_connector::Connector;
use opensovd_client::Client;

pub fn mock_client(connector: Connector) -> Client {
    Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .connector(connector)
        .build()
        .expect("valid test client")
}
