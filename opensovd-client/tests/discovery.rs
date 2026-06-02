// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used, clippy::unwrap_used)]

use mock_http_connector::Connector;
use opensovd_client::{Client, SovdInfo, VendorInfo};
use serde_json::json;

fn discovery(connector: Connector) -> opensovd_client::Discovery {
    Client::builder()
        .base_uri("http://localhost:7690/sovd")
        .expect("valid URI")
        .connector(connector)
        .discovery()
        .expect("valid discovery client")
}

#[tokio::test]
async fn select_reuses_transport() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1"
            }]})
            .to_string(),
        )
        .unwrap();
    b.expect()
        .with_uri("http://localhost:7690/sovd/v1/components")
        .returning(json!({"items": []}).to_string())
        .unwrap();

    let client = discovery(b.build())
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "1.1")
        .await
        .unwrap();

    // The selected client must reuse the same (mock) transport and hit the advertised base.
    let list = client.list_components().send().await.unwrap();
    assert!(list.data.items.is_empty());
}

#[tokio::test]
async fn select_matches_on_vendor_info() {
    // Exercises the typed vendor_info predicate path of `select`.
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1",
                "vendor_info": {"name": "OpenSOVD", "version": "2.0"}
            }]})
            .to_string(),
        )
        .unwrap();
    b.expect()
        .with_uri("http://localhost:7690/sovd/v1/components")
        .returning(json!({"items": []}).to_string())
        .unwrap();

    let client = discovery(b.build())
        .select(|s: &SovdInfo<VendorInfo>| {
            s.vendor_info.as_ref().is_some_and(|v| v.name == "OpenSOVD")
        })
        .await
        .unwrap();
    assert!(
        client
            .list_components()
            .send()
            .await
            .unwrap()
            .data
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn select_no_match() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1"
            }]})
            .to_string(),
        )
        .unwrap();

    let err = discovery(b.build())
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "2.0")
        .await
        .err()
        .expect("expected an error");
    assert!(
        matches!(err, opensovd_client::Error::NoMatchingVersion),
        "expected NoMatchingVersion, got: {err:?}"
    );
}

#[tokio::test]
async fn versions_lists_instances() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1",
                "vendor_info": {"name": "OpenSOVD", "version": "2.0"}
            }]})
            .to_string(),
        )
        .unwrap();

    let versions: Vec<SovdInfo<VendorInfo>> = discovery(b.build()).versions().await.unwrap();

    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, "1.1");
    let vendor = versions[0]
        .vendor_info
        .as_ref()
        .expect("vendor info present");
    assert_eq!(vendor.name, "OpenSOVD");
    assert_eq!(vendor.version, "2.0");
}

#[tokio::test]
async fn versions_without_vendor() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1"
            }]})
            .to_string(),
        )
        .unwrap();

    let versions: Vec<SovdInfo<VendorInfo>> = discovery(b.build()).versions().await.unwrap();
    assert!(versions[0].vendor_info.is_none());
}

#[tokio::test]
async fn versions_accepts_arbitrary_vendor() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1",
                "vendor_info": {"anything": [1, 2, 3]}
            }]})
            .to_string(),
        )
        .unwrap();

    let versions: Vec<SovdInfo<serde_json::Value>> = discovery(b.build()).versions().await.unwrap();
    assert_eq!(versions[0].vendor_info.as_ref().unwrap()["anything"][2], 3);
}

#[tokio::test]
async fn version_info_error_status() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning((
            http::StatusCode::NOT_FOUND,
            json!({"error_code": "vendor-specific", "message": "not found"}).to_string(),
        ))
        .unwrap();

    let err = discovery(b.build())
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "1.1")
        .await
        .err()
        .expect("expected an error");
    match err {
        opensovd_client::Error::ApiError { status, error } => {
            assert_eq!(status.as_u16(), 404);
            assert!(error.is_some());
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn version_info_is_cached() {
    let mut b = Connector::builder();
    b.expect()
        .times(1)
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1"
            }]})
            .to_string(),
        )
        .unwrap();
    let connector = b.build();

    let disco = discovery(connector.clone());
    // Two reads of the version list must hit /version-info exactly once.
    let _ = disco.versions::<VendorInfo>().await.unwrap();
    let _ = disco
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "1.1")
        .await
        .unwrap();

    connector
        .checkpoint()
        .expect("version-info should be fetched exactly once");
}
