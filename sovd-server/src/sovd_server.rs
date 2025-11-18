/*
* Copyright (c) 2025 The Contributors to Eclipse OpenSOVD (see CONTRIBUTORS)
*
* See the NOTICE file(s) distributed with this work for additional
* information regarding copyright ownership.
*
* This program and the accompanying materials are made available under the
* terms of the Apache License Version 2.0 which is available at
* https://www.apache.org/licenses/LICENSE-2.0
*
* SPDX-License-Identifier: Apache-2.0
*/

//! Main library entry point for sovd_interfaces implementation.

#![allow(unused_imports)]

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt, future};
use hyper::body::Bytes;
use hyper::service::Service;
use hyper::{Body, Request, Response, header};
use log::{error, info, warn};
use openssl::ssl::{Ssl, SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod};
use regex::Regex;
use serde_json::Value;
use serde_json::error::Category;
use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use swagger::EmptyContext;
use swagger::auth::MakeAllowAllAuthenticator;
use swagger::{Has, XSpanIdString};
use tokio::net::TcpListener;

use hyper::header::HeaderMap;
use hyper::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    HeaderValue,
};
use hyper::server::conn::AddrIncoming;
use serde_json::Error as SerdeError;
use serde_json::Map;
use serde_json::Value as JsonValue;
use serde_json::to_value;
use serde_json::{Number, json}; // Import for Number and JSON
use std::collections::BTreeMap; // Import for BTreeMap
use std::convert::Infallible;
use std::env;
use std::fs::File;
use std::io::ErrorKind;
use std::io::Write;
use std::str::FromStr;
use std::str::from_utf8;
use tokio::task;
use tokio::task::JoinHandle;
use tokio_openssl::SslStream;
use tokio::signal;
use axum_extra::extract::Host;

use openapi::models::*;

use crate::server_config::ServerConfig;

// Import the required modules
use sovd_handlers::IDENT_DATA_RESPONSE;
use sovd_handlers::create_entity_collection_response;
use sovd_handlers::filter_by_writable;
use sovd_handlers::find_processes;
use sovd_handlers::find_single_process;
use sovd_handlers::group_by_writability;
use sovd_handlers::prepare_data_response;

use sovd_handlers::extract_name_and_replace_dashes;
use sovd_handlers::extract_response_data_from_json_to_response;
use sovd_handlers::find_entity_by_name;
use sovd_handlers::gateway_request;
use sovd_handlers::get_cpu_usage;
use sovd_handlers::get_disk_io;
use sovd_handlers::get_first_part_after_dash;
use sovd_handlers::get_last_part_after_dash;
use sovd_handlers::get_memory_usage;
use sovd_handlers::get_system_cpu_usage;
use sovd_handlers::get_system_disk_io;
use sovd_handlers::get_system_memory_usage;
use sovd_handlers::handle_app_resource;
use sovd_handlers::handle_system_resource;
use sovd_handlers::is_host_available;
use sovd_handlers::update_href_with_base_uri;
use openapi::{
    apis::bulk_data::*,
    apis::capabilities::*,
    apis::communication_logs::*,
    apis::configurations::*,
    apis::data_retrieval::*,
    apis::discovery::*,
    apis::fault_handling::*,
    apis::locking::*,
    apis::logging::*,
    apis::operations_control::*,
    apis::target_modes::*,
    apis::updates::*,
};
use crate::ServerImpl;

//use vehicle_auth_server;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
// Global variable for the server configuration
static SERVER_CONFIG: OnceCell<ServerConfig> = OnceCell::new();

// Function to initialize the global server configuration
pub fn init_server_config(config: ServerConfig) {
    SERVER_CONFIG
        .set(config)
        .expect("Failed to set server config");
}

// Function to access the global server configuration
pub fn get_server_config() -> Option<&'static ServerConfig> {
    SERVER_CONFIG.get()
}

pub async fn create(server_config: &ServerConfig, addr: &str) {
    let addr:SocketAddr = addr.parse().expect("Failed to parse bind address");

    //Set SERVER_CONFIG to apply config settings
    init_server_config(server_config.clone());

    let id = server_config.host_name.clone();
    let name = format!("sovd-{}",server_config.sovd_mode.clone());
    let app = Arc::new(ServerImpl { id, name });

    //start mdns
    register_sovd_mdns(&server_config.host_name.clone(), server_config.get_port().parse().unwrap_or_default()).await;

    let app = openapi::server::new(app);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

#[allow(dead_code)]
pub async fn spawn_test_server(server_config: &ServerConfig) -> (SocketAddr, JoinHandle<()>) {
    // Init Axum server instance (the generated server builder wraps our implementation)
    let id = server_config.host_name.clone();
    let name = format!("sovd-{}",server_config.sovd_mode.clone());
    let app = Arc::new(ServerImpl { id, name });
    let app = openapi::server::new(app);

    // Bind to port 0 to let OS assign a free port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get local address");

    let server_future = axum::serve(listener, app);
    
    let handle = tokio::spawn(async move {
        if let Err(e) = server_future.await {
            info!("Server error {}", e);
        }
    });

    (addr, handle)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub async fn register_sovd_mdns(id: &str, port: u16) {
        let mdns = ServiceDaemon::new().expect("Failed to create mDNS daemon");

        let service_type = "_sovd._tcp.local.";
        let instance_name = id;
        let host_name = format!("{}.local.", id);

        let mut properties = HashMap::new();
        properties.insert("path".to_string(), "/v1".to_string());
        properties.insert("version".to_string(), "1.0".to_string());

        let service_info = ServiceInfo::new(
            service_type,
            instance_name,
            &host_name,
            "", // IP auto-filled
            port,
            properties,
        )
        .unwrap()
        .enable_addr_auto();

        mdns.register(service_info)
            .expect("Failed to register mDNS service");

        info!(
            "SOVD server mDNS registered: {} on port {}",
            instance_name, port
        );
    }

#[cfg(test)]
mod tests {

    use super::*;
    use {
        EntityCollectionEntityIdBulkDataGetPathParams as ColParam,
        EntityCollectionEntityIdBulkDataGetResponse as BulkResp,
        EntityCollectionEntityIdDataDataIdGetResponse as DataIdResp,
        ComponentsComponentIdRelatedAppsGetResponse as AppsResp,
        EntityCollectionEntityIdDataGetResponse as DataResp,
        EntityCollectionGetResponse as EntityResp,
        AnyPathDocsGetDefaultResponse as ErrBody,
        serde_json::Value as Value,
    };
    use std::sync::LazyLock;
    use axum_extra::extract::CookieJar;
    use http::Method;

    // Function used as mock SERVER_CONFIG for some of the tests.
    fn ensure_server_config(sovd_mode: String, host_name: String) {
        #[allow(unused)]
        let cfg =     ServerConfig::create_server_settings(
        "../config/sovd_server_apps.conf",
        "http".to_string(),
        "127.0.0.1".to_string(),
        "8080".to_string(),
        sovd_mode,
        host_name,
        ).expect("Failed to create server config");

        if SERVER_CONFIG.get().is_none() {
            let _ = SERVER_CONFIG.set(cfg);
        }
        
    }
    
    //Mock for tests
    fn make_server() -> ServerImpl {
        ServerImpl { id: "007".to_string(), name: "SOVD Test".to_string() }
    }

    
    // Mock Host header
    static MOCK_HOST: LazyLock<Host> = LazyLock::new(|| Host("example.com".to_string()));
    static MOCK_COOKIES: LazyLock<CookieJar> = LazyLock::new(|| CookieJar::new());




    /**
     * Test: `bulk_data_get_schema_none_when_not_requested`
     *
     * Purpose:
     * Verifies the behavior of the `entity_collection_entity_id_bulk_data_get` endpoint
     * when no schema is requested (`None`).
     *
     * Expected Result:
     * - `body.items` should be empty.
     * - `body.schema` should be `None`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn bulk_data_get_schema_none_when_not_requested() {
        
        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdBulkDataGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: String::from("id-1"),
        };
        let query_params = EntityCollectionEntityIdBulkDataGetQueryParams { include_schema: None };

        //Run test
        let rsp = server
            .entity_collection_entity_id_bulk_data_get(&Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params)
            .await
            .expect("Fail for entity_collection_entity_id_bulk_data_get");

        //Assert
        match rsp {
            BulkResp::Status200_TheBulkDataCategoriesSupportedByTheEntity(body) => {
                assert!(body.items.is_empty());
                assert_eq!(body.schema, None);
            }
            _ => panic!("unexpected variant"),
        }
    }

    
    
    /**
     * Test: `bulk_data_get_schema_some_false_when_true_requested`
     *
     * Purpose:
     * Verifies the behavior of the `entity_collection_entity_id_bulk_data_get` endpoint
     * when schema is explicitly requested (`Some(true)`).
     *
     * Expected Result:
     * - `body.items` should be empty.
     * - `body.schema` should be `Some(false)` (schema not available).
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn bulk_data_get_schema_some_false_when_true_requested() {
        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdBulkDataGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: String::from("id-2"),
        };
        let query_params = EntityCollectionEntityIdBulkDataGetQueryParams { include_schema: Some(true) };

        //Run test
        let rsp = server
            .entity_collection_entity_id_bulk_data_get(&Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params)
            .await
            .expect("Fail for entity_collection_entity_id_bulk_data_get");

        //Assert
        match rsp {
            BulkResp::Status200_TheBulkDataCategoriesSupportedByTheEntity(body) => {
                assert!(body.items.is_empty());
                assert_eq!(body.schema, Some(false));
            }
            _ => panic!("unexpected variant"),
        }
    }

    
    
    /**
     * Test: `data_get_apps_builds_four_items_with_expected_ids_and_name`
     *
     * Purpose:
     * Validates the `entity_collection_entity_id_data_get` endpoint for the `Apps` collection.
     *
     * Expected Result:
     * - Four items should be returned.
     * - Each item should have an expected ID (`cpu`, `disk`, `memory`, `all`).
     * - Each item's name should match the format: "current <id> usage for apps <cleaned_id>".
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn data_get_apps_builds_four_items_with_expected_ids_and_name() {

        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: String::from("chassis-hpc"),
        };
        let query_params = EntityCollectionEntityIdDataGetQueryParams { 
             groups: None,
             category: vec!["test".to_string()],
            include_schema: None
        };
        let entity_id = "chassis-hpc";

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_get(&Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params)
            .await
            .expect("Fail for entity_collection_entity_id_data_get");

        //Assert
        match rsp {
            DataResp::Status200_TheRequestWasSuccessful(body) => {
                let ids = vec!["cpu", "disk", "memory", "all"];
                for id in ids {
                    for item in &body.items {
                        if id == item.id {
                            assert_eq!(item.id, id);
                            assert_eq!(item.name.to_lowercase(), format!("current {} usage for apps {}", id, entity_id.split('-').next().unwrap()));
                            break;
                        }
                    }
                }
                
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    
    
    /**
     * Test: `data_get_components_builds_four_items`
     *
     * Purpose:
     * Verifies the `entity_collection_entity_id_data_get` endpoint for the `Components` collection.
     *
     * Expected Result:
     * - Exactly four items should be returned.
     * - Each item ID should end with one of the expected suffixes: `-cpu`, `-disk`, `-memory`, `-all`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn data_get_components_builds_four_items() {

        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataGetPathParams {
            entity_collection: String::from("Components"),
            entity_id: String::from("comp-xyz-7"),
        };
        let query_params = EntityCollectionEntityIdDataGetQueryParams { 
             groups: None,
             category: vec!["test".to_string()],
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_get");

        //Assert
        match rsp {
            DataResp::Status200_TheRequestWasSuccessful(body) => {
                assert_eq!(body.items.len(), 4);
                
                let ids: Vec<_> = body.items.iter().map(|it| it.id.as_str()).collect();
                assert!(ids.iter().any(|id| id.ends_with("-cpu")));
                assert!(ids.iter().any(|id| id.ends_with("-disk")));
                assert!(ids.iter().any(|id| id.ends_with("-memory")));
                assert!(ids.iter().any(|id| id.ends_with("-all")));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    

    /**
     * Test: `data_get_default_branch_returns_not_yet_implemented`
     *
     * Purpose:
     * Ensures that the `entity_collection_entity_id_data_get` endpoint returns an error
     * when called for the `Functions` collection, which is not yet implemented.
     *
     * Expected Result:
     * - Response should be an error variant.
     * - `error_code` should be `"NotYetImplemented"`.
     * - Error message should contain `"Not yet implemented"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn data_get_default_branch_returns_not_yet_implemented() {

        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataGetPathParams {
            entity_collection: String::from("Functions"),
            entity_id: String::from("test"),
        };
        let query_params = EntityCollectionEntityIdDataGetQueryParams { 
             groups: None,
             category: vec!["test".to_string()],
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_get");
        
        //Assert
        match rsp {
            DataResp::Status0_AnUnexpectedRequestOccurred(err) => {
                assert_eq!(err.error_code, "NotYetImplemented");
                assert!(err.message.contains("Not yet implemented"));
            }
            other => panic!("expected error variant, got: {:?}", other),
        }
    }

    
    
    /**
     * Test: `data_groups_get_forwards_success_from_group_by_writability`
     *
     * Purpose:
     * Verifies that the `entity_collection_entity_id_data_groups_get` endpoint
     * correctly forwards the result from the `group_by_writability` processor.
     *
     * Expected Result:
     * - API response should match the result of `group_by_writability(test_data)`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn data_groups_get_forwards_success_from_group_by_writability() {

        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataGroupsGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: String::from("entity-123"),
        };
        let test = vec![Value::Bool(true)];
        let expected = group_by_writability(&test).expect("should succeed for test data");

        //Run test
        let got = server
            .entity_collection_entity_id_data_groups_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_groups_get");

        //Assert
        assert_eq!(format!("{:?}", got), format!("{:?}", expected),
            "API result should equal processor result");
    }

    

    /**
     * Test: `entity_collection_entity_id_data_data_id_get_not_initialized`
     *
     * Purpose:
     * Checks the behavior of the `entity_collection_entity_id_data_data_id_get` endpoint
     * when requesting a specific data ID that has not been initialized.
     *
     * Expected Result:
     * - Response should be an error variant.
     * - `error_code` should be `"UnknownResource"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_entity_id_data_data_id_get_not_initialized() {

        //Pre-requesities before run
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataDataIdGetPathParams {
            entity_collection: String::from("Components"),
            entity_id: String::from("telematics"),
            data_id: String::from("veh-01"),
        };
        let query_params = EntityCollectionEntityIdDataDataIdGetQueryParams { 
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        //Assert
        match rsp {
            DataIdResp::Status0_AnUnexpectedRequestOccurred(body) => {
                let error_code = "UnknownResource".to_string();
                assert_eq!(error_code, body.error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
    }


    
    /**
     * Test: `entity_collection_entity_id_data_data_id_get_apps_fail_to_find_process`
     *
     * Purpose:
     * Tests the `entity_collection_entity_id_data_data_id_get` endpoint for the `Apps` collection
     * when the process cannot be found for the given entity.
     *
     * Expected Result:
     * - The response should be an error variant.
     * - `error_code` should be `"ProcessNotFound"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_entity_id_data_data_id_get_apps_fail_to_find_process() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("noprocess"));
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataDataIdGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: String::from("noprocess"),
            data_id: String::from("veh-01-cpu"),
        };
        let query_params = EntityCollectionEntityIdDataDataIdGetQueryParams { 
            include_schema: None
        };

        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::Status0_AnUnexpectedRequestOccurred(body) => {
                let error_code = "ProcessNotFound".to_string();
                assert_eq!(error_code, body.error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
    }
    

    
    /**
     * Test: `entity_collection_entity_id_data_data_id_get_apps_by_process_with_unknown_resource`
     *
     * Purpose:
     * Verifies the behavior of the `entity_collection_entity_id_data_data_id_get` endpoint
     * when the process exists but the resource is unknown.
     *
     * Expected Result:
     * - The response should be an error variant.
     * - `error_code` should be `"UnknownResource"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_entity_id_data_data_id_get_apps_by_process_with_unknown_resource() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataDataIdGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: format!("sovd_server-{}", std::process::id()),
            data_id: String::from("sovd_server"),
        };
        let query_params = EntityCollectionEntityIdDataDataIdGetQueryParams { 
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                &Method::GET,
                &MOCK_HOST,
                &MOCK_COOKIES,
                &path_params,
                &query_params
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");
        
        //Assert
        match rsp {
            DataIdResp::Status0_AnUnexpectedRequestOccurred(body) => {
                let error_code = "UnknownResource".to_string();
                assert_eq!(error_code, body.error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }

    
    /**
     * Test: `entity_collection_entity_id_data_data_id_get_apps_by_process_with_cpu_usage`
     *
     * Purpose:
     * Verifies that the `entity_collection_entity_id_data_data_id_get` endpoint
     * correctly returns CPU usage data for a known process in the `Apps` collection.
     *
     * Expected Result:
     * - The response should contain a data object with:
     *   - `"cpu_usage"` field present.
     *   - `"description"` matching "CPU usage for sovd_server".
     *   - `"name"` equal to "CPU".
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */


    #[tokio::test]
    async fn entity_collection_entity_id_data_data_id_get_apps_by_process_with_cpu_usage() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataDataIdGetPathParams {
            entity_collection: String::from("Apps"),
            entity_id: format!("sovd_server-{}", std::process::id()),
            data_id: String::from("sovd_server-cpu"),
        };
        let query_params = EntityCollectionEntityIdDataDataIdGetQueryParams { 
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                &Method::GET,
                &MOCK_HOST,
                &MOCK_COOKIES,
                &path_params,
                &query_params     
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        //Assert
        match rsp {
            DataIdResp::Status200_TheRequestWasSuccessful(body) => {
                
                let data = openapi::types::Object::new(json!({
                    "cpu_usage": "cpu_usage",
                    "description": "CPU usage for sovd_server",
                    "name": "CPU"
                }));

                assert_eq!(body.data, data);
            
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }


    
    /**
     * Test: `entity_collection_entity_id_data_data_id_get_apps_process_not_found`
     *
     * Purpose:
     * Tests the behavior of the `entity_collection_entity_id_data_data_id_get` endpoint
     * when the process is not found in the `Functions` collection.
     *
     * Expected Result:
     * - The response should be an error variant.
     * - `error_code` should be `"EntityCollectionNotFound"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_entity_id_data_data_id_get_apps_process_not_found() {
        
        //Pre-requesities before run
        ensure_server_config(String::from("no_process"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionEntityIdDataDataIdGetPathParams {
            entity_collection: String::from("Functions"),
            entity_id: format!("sovd_server-{}", std::process::id()),
            data_id: String::from("sovd_server"),
        };
        let query_params = EntityCollectionEntityIdDataDataIdGetQueryParams { 
            include_schema: None
        };

        //Run test
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                &Method::GET,
                &MOCK_HOST,
                &MOCK_COOKIES,
                &path_params,
                &query_params  
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");
        
        //Assert
        match rsp {
            DataIdResp::Status0_AnUnexpectedRequestOccurred(body) => {
                let error_code = "EntityCollectionNotFound".to_string();
                assert_eq!(error_code, body.error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }

    

    /**
     * Test: `components_component_id_related_apps_get_sovd_mode_standalone`
     *
     * Purpose:
     * Verifies that the `components_component_id_related_apps_get` endpoint
     * returns related apps for a given component in standalone mode.
     *
     * Expected Result:
     * - The response should contain a non-empty list of related apps in `body.items`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn components_component_id_related_apps_get_sovd_mode_standalone() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = ComponentsComponentIdRelatedAppsGetPathParams {
            component_id: String::from("chassis-hpc"),
        };

        //Run test
        let rsp = server
            .components_component_id_related_apps_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params
            )
            .await
            .expect("Fail for components_component_id_related_apps_get");
        
        //Assert
        match rsp {
            AppsResp::Status200_ResponseBody(body) => {
                assert!(!body.items.is_empty());
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }


    
    /**
     * Test: `entity_collection_get_chassis_hpc_with_schema`
     *
     * Purpose:
     * Verifies the behavior of the `entity_collection_get` endpoint for the `Components` collection
     * when schema is explicitly requested (`Some(true)`).
     *
     * Expected Result:
     * - The response should contain an item with the name `"Chassis-HPC"` in `body.items`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_get_chassis_hpc_with_schema() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionGetPathParams {
            entity_collection: String::from("Components"),
        };
        let query_params = EntityCollectionGetQueryParams {
            include_schema: Some(true),
        };
        

        //Run test
        let rsp = server
            .entity_collection_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_get");

        //Assert
        match rsp {
            EntityResp::Status200_ResponseBody(body) => {
                let expect = body.items.iter()
                .any(|item| item.name == "Chassis-HPC");
                assert!(expect);
                assert_eq!(body.schema, Some(false)); //Currently in the actual implementation there is just Some(false)
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }


    
    /**
     * Test: `entity_collection_get_chassis_hpc`
     *
     * Purpose:
     * Verifies the behavior of the `entity_collection_get` endpoint for the `Components` collection
     * when schema is not requested (`Some(false)`).
     *
     * Expected Result:
     * - The response should still contain an item with the name `"Chassis-HPC"` in `body.items`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_get_chassis_hpc() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionGetPathParams {
            entity_collection: String::from("Components"),
        };
        let query_params = EntityCollectionGetQueryParams {
            include_schema: None,
        };

        //Run test
        let rsp = server
            .entity_collection_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_get");

        //Assert
        match rsp {
            EntityResp::Status200_ResponseBody(body) => {
                let expect = body.items.iter()
                .any(|item| item.name == "Chassis-HPC");
                assert!(expect);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }


    
    /**
     * Test: `entity_collection_get_no_defined_collection`
     *
     * Purpose:
     * Tests the behavior of the `entity_collection_get` endpoint when called for the `Apps` collection,
     * which is not defined in the current configuration.
     *
     * Expected Result:
     * - The response should be an error variant.
     * - `error_code` should be `"UnexpectedRequest"`.
     *
     * This test uses the real endpoint function to simulate an actual API call.
     */

    #[tokio::test]
    async fn entity_collection_get_no_defined_collection() {

        //Pre-requesities before run
        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        let server = make_server();
        let path_params = EntityCollectionGetPathParams {
            entity_collection: String::from("Apps"),
        };
        let query_params = EntityCollectionGetQueryParams {
            include_schema: None,
        };

        //Run test
        let rsp = server
            .entity_collection_get(
                &Method::GET, &MOCK_HOST, &MOCK_COOKIES, &path_params, &query_params
            )
            .await
            .expect("Fail for entity_collection_get");

        //Assert
        match rsp {
            EntityResp::Status0_AnUnexpectedRequestOccurred(body) => {
                let error_code = "UnexpectedRequest".to_string();
                assert_eq!(body.error_code, error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }
    

}