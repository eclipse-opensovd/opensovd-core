// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::{mock_server, send};
use hyper::Method;

/// The path below the API version, as error messages name it.
fn api_path(path: &str) -> &str {
    path.trim_start_matches("/sovd/v1/")
}

#[tokio::test]
async fn routes_follow_the_entity_kind() {
    let server = mock_server().await;

    for path in [
        "/sovd/v1/areas/powertrain/data",
        "/sovd/v1/areas/powertrain/faults",
        "/sovd/v1/areas/powertrain/subcomponents",
        "/sovd/v1/components/ecu/subareas",
        "/sovd/v1/components/ecu/contains",
        "/sovd/v1/apps/engine_control/hosts",
        "/sovd/v1/apps/engine_control/subcomponents",
        "/sovd/v1/components/ecu/subcomponents/injector/unknown",
        "/sovd/v1/components/nope/unknown",
        "/sovd/v1/unknown/x/data",
        "/sovd/v1/apps//status",
        "/sovd/v1/functions//depends-on",
        "/sovd/v1/components//unknown",
        "/sovd/v1/components//subcomponents/injector",
        "/sovd/v1/components/ecu/subcomponents//data",
        "/sovd/v1/apps/%2E%2E/status",
        "/sovd/v1/areas/powertrain/subareas/%2E/contains",
        "/sovd/v1/components/ecu/docs",
        "/sovd/v1/components/ecu/%FF",
        "/sovd/v1/components/nope/%FF",
    ] {
        let (status, json) = send(&server, Method::GET, path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(json["error_code"], "vendor-specific", "{path}");
        assert_eq!(json["vendor_code"], "resource-not-found", "{path}");
        assert_eq!(
            json["message"],
            format!("Resource not found: {}", api_path(path)),
            "{path}"
        );
    }
}
