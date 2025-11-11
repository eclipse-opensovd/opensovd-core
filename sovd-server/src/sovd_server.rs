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
use hyper::http;
use hyper::server::conn::Http;
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

use openapi_client::models;
use openapi_client::models::*;

use crate::server_config::ServerConfig;
use crate::server_config::ServiceDaemonWrapper;

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

//use vehicle_auth_server;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Builds an SSL implementation for Simple HTTPS from some hard-coded file names
///
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

fn create_m_dns(server_config: &ServerConfig, mdns: &ServiceDaemonWrapper) {
    let hostname = server_config.get_hostname();
    let ip_address = server_config.get_ip_address();

    // Create a service info.
    let service_type = "_sovd_server._udp.local."; //"_mdns-sd-my-test._udp.local.";
    let instance_name = format!("{}_instance", hostname); //"my_instance";
    let ip = ip_address;
    let host_name = format!("{}{}", hostname, service_type);
    let port = server_config.get_port().parse::<u16>().unwrap();
    let properties = [
        ("identification", hostname),
        ("accessurl", server_config.get_base_uri()),
        ("sovd_mode", server_config.get_sovd_mode()),
    ];

    let my_service = ServiceInfo::new(
        service_type,
        instance_name.as_str(),
        host_name.as_str(),
        ip,
        port,
        &properties[..],
    )
    .unwrap();

    // Register with the daemon, which publishes the service.
    mdns.register(my_service)
        .expect("Failed to register our service");
}

pub async fn get_m_dns_messages(
    _server_config: Arc<ServerConfig>,
    mdns: Arc<ServiceDaemonWrapper>,
) {
    let service_type = "_sovd_server._udp.local.";
    let receiver = mdns.browse(service_type).expect("Failed to browse");

    // Spawning a Tokio task to handle events asynchronously
    task::spawn(async move {
        while let Ok(event) = receiver.recv_async().await {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    // Example: Handling ServiceResolved event
                    if let Some(text_property) = info.get_property("sovd_mode") {
                        let val_text = text_property.val_str();
                        info!("val_text in spawn: {}", val_text);
                    }

                    let value = info
                        .get_property("sovd_mode")
                        .expect("Failed to get property")
                        .val_str();
                    info!(
                        "Resolved a new service: {} IP: {}:{} with mode {}",
                        info.get_fullname(),
                        info.get_addresses_v4().iter().next().unwrap(),
                        info.get_port(),
                        value
                    );
                }
                _other_event => {
                    // Example: Handling other events (optional)
                }
            }
        }
    });
}

pub async fn create(server_config: &ServerConfig, addr: &str) {
    let addr = addr.parse().expect("Failed to parse bind address");

    // Atomic boolean for controlling server shutdown
    let shutdown_signal = Arc::new(AtomicBool::new(false));

    let server = Server::new();
    let service = MakeService::new(server);

    let service = MakeAllowAllAuthenticator::new(service, "cosmo");

    init_server_config(server_config.clone());

    let ip_address = server_config.get_ip_address();

    info!("Starting mDNS server: {}", ip_address);

    let service_daemon = ServiceDaemon::new().unwrap();
    let mdns_wrapper = Arc::new(ServiceDaemonWrapper::new(service_daemon));

    info!("Starting mDNS server: {}", server_config.get_ip_address());
    create_m_dns(server_config, &mdns_wrapper);

    let arc_server_config = Arc::new(server_config.clone());
    get_m_dns_messages(arc_server_config, Arc::clone(&mdns_wrapper)).await;

    #[allow(unused_mut)]
    let mut service =
        openapi_client::server::context::MakeAddContext::<_, EmptyContext>::new(service);

    //Start HTTP server
    info!("Starting HTTP server on {}", addr);
    let server = hyper::Server::bind(&addr).serve(service);
    if let Err(e) = server.await {
        error!("Server error: {}", e);
    }

    //Wait for Ctrl + C to shutdown server
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    shutdown_signal.store(true, Ordering::Relaxed);

    info!("Shutting down server...");
    // Additional cleanup tasks can be performed here

    info!("Shutting down mDNS server...");
    mdns_wrapper.shutdown();
}

#[allow(dead_code)]
pub async fn spawn_test_server(server_config: &ServerConfig) -> (SocketAddr, JoinHandle<()>) {
    // Bind to port 0 to let OS assign a free port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get local address");
    let incoming = AddrIncoming::from_listener(listener).expect("Failed to create AddrIncoming");

    let server = Server::new();
    let service = MakeService::new(server);
    let service = MakeAllowAllAuthenticator::new(service, "cosmo");

    if SERVER_CONFIG.get().is_none() {
        init_server_config(server_config.clone());
    }

    let service = openapi_client::server::context::MakeAddContext::<_, EmptyContext>::new(service);
    let server_future = hyper::Server::builder(incoming).serve(service);

    let handle = tokio::spawn(async move {
        if let Err(e) = server_future.await {
            error!("Server error {}", e);
        }
    });

    (addr, handle)
}

#[derive(Copy, Clone)]
pub struct Server<C> {
    marker: PhantomData<C>,
}

impl<C> Server<C> {
    pub fn new() -> Self {
        Server {
            marker: PhantomData,
        }
    }
}

impl<C> Default for Server<C> {
    fn default() -> Self {
        Self::new()
    }
}

use swagger::auth::Authorization;

use openapi_client::{
    AnyPathDocsGetResponse, Api, AreasAreaIdRelatedComponentsGetResponse,
    AreasAreaIdSubareasGetResponse, ComponentsComponentIdRelatedAppsGetResponse,
    ComponentsComponentIdSubcomponentsGetResponse, DeleteAllFaultsResponse,
    DeleteFaultByIdResponse, EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse,
    EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse,
    EntityCollectionEntityIdBulkDataCategoryDeleteResponse,
    EntityCollectionEntityIdBulkDataCategoryGetResponse,
    EntityCollectionEntityIdBulkDataCategoryPostResponse,
    EntityCollectionEntityIdBulkDataGetResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse,
    EntityCollectionEntityIdCommunicationLogsGetResponse,
    EntityCollectionEntityIdCommunicationLogsPostResponse,
    EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse,
    EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse,
    EntityCollectionEntityIdConfigurationsGetResponse,
    EntityCollectionEntityIdDataCategoriesGetResponse,
    EntityCollectionEntityIdDataDataIdGetResponse, EntityCollectionEntityIdDataDataIdPutResponse,
    EntityCollectionEntityIdDataGetResponse, EntityCollectionEntityIdDataGroupsGetResponse,
    EntityCollectionEntityIdDataListsDataListIdDeleteResponse,
    EntityCollectionEntityIdDataListsDataListIdGetResponse,
    EntityCollectionEntityIdDataListsGetResponse, EntityCollectionEntityIdDataListsPostResponse,
    EntityCollectionEntityIdGetResponse, EntityCollectionEntityIdLocksGetResponse,
    EntityCollectionEntityIdLocksLockIdDeleteResponse,
    EntityCollectionEntityIdLocksLockIdGetResponse, EntityCollectionEntityIdLocksLockIdPutResponse,
    EntityCollectionEntityIdLocksPostResponse, EntityCollectionEntityIdLogsConfigDeleteResponse,
    EntityCollectionEntityIdLogsConfigGetResponse, EntityCollectionEntityIdLogsConfigPutResponse,
    EntityCollectionEntityIdLogsEntriesGetResponse, EntityCollectionEntityIdModesGetResponse,
    EntityCollectionEntityIdModesModeIdGetResponse, EntityCollectionEntityIdModesModeIdPutResponse,
    EntityCollectionEntityIdOperationsGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse,
    EntityCollectionEntityIdOperationsOperationIdGetResponse, EntityCollectionGetResponse,
    GetFaultByIdResponse, GetFaultsResponse, UpdatesGetResponse, UpdatesPostResponse,
    UpdatesUpdatePackageIdAutomatedPutResponse, UpdatesUpdatePackageIdDeleteResponse,
    UpdatesUpdatePackageIdExecutePutResponse, UpdatesUpdatePackageIdGetResponse,
    UpdatesUpdatePackageIdPreparePutResponse, UpdatesUpdatePackageIdStatusGetResponse,
};

use openapi_client::server::MakeService;

use std::error::Error;
use swagger::ApiError;

#[async_trait]
impl<C> Api<C> for Server<C>
where
    C: Has<XSpanIdString> + Send + Sync,
{
    async fn entity_collection_entity_id_bulk_data_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_get(\"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            include_schema,
            context.get().0.clone()
        );
        info!("HERE entity_collection_entity_id_bulk_data_get");

        // Create a vector of categories based on the entity_id and entity_collection
        let categories: Vec<EntityCollectionEntityIdBulkDataGet200ResponseItemsInner> = vec![];

        // Create an instance of EntityCollectionEntityIdBulkDataGet200Response with retrieved categories
        let inline_response = EntityCollectionEntityIdBulkDataGet200Response::new(categories);

        // Check if include_schema is true and set schema accordingly
        let schema = if let Some(true) = include_schema {
            Some(false)
        } else {
            None
        };

        // Attach schema to EntityCollectionEntityIdBulkDataGet200Response
        let inline_response = EntityCollectionEntityIdBulkDataGet200Response {
            items: inline_response.items,
            schema,
        };

        // Create the response body variant
        let response =
            EntityCollectionEntityIdBulkDataGetResponse::TheBulkDataCategoriesSupportedByTheEntity(
                inline_response,
            );

        // Return the response
        Ok(response)
    }

    async fn entity_collection_entity_id_bulk_data_category_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryDeleteResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_category_delete({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            category,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_bulk_data_category_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_category_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            category,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_bulk_data_category_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        content_type: String,
        content_length: i32,
        content_disposition: String,
        body: swagger::ByteArray,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryPostResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_category_post({:?}, \"{}\", {:?}, \"{}\", {}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            category,
            content_type,
            content_length,
            content_disposition,
            body,
            context.get().0.clone()
        );
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "ServerConfigurationNotInitialized".to_string(),
            message: "Server configuration not initialized.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };
        return Ok(
            EntityCollectionEntityIdBulkDataCategoryPostResponse::AnUnexpectedRequestOccurred(
                error,
            ),
        );
    }

    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        bulk_data_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_category_bulk_data_id_delete({:?}, \"{}\", {:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            category,
            bulk_data_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        bulk_data_id: String,
        accept: Option<String>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_bulk_data_category_bulk_data_id_get({:?}, \"{}\", {:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            category,
            bulk_data_id,
            accept,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn any_path_docs_get(
        &self,
        any_path: String,
        context: &C,
    ) -> Result<AnyPathDocsGetResponse, ApiError> {
        info!(
            "any_path_docs_get(\"{}\") - X-Span-ID: {:?}",
            any_path,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_communication_logs_get({:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        body: swagger::ByteArray,
        stream: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsPostResponse, ApiError> {
        info!(
            "entity_collection_entity_id_communication_logs_post({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            body,
            stream,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        delete_log: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse, ApiError>
    {
        info!(
            "entity_collection_entity_id_communication_logs_communication_log_id_delete({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            communication_log_id,
            delete_log,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse, ApiError>
    {
        info!(
            "entity_collection_entity_id_communication_logs_communication_log_id_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            communication_log_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        entity_collection_entity_id_communication_logs_communication_log_id_put_request: models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse, ApiError>
    {
        info!(
            "entity_collection_entity_id_communication_logs_communication_log_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            communication_log_id,
            entity_collection_entity_id_communication_logs_communication_log_id_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdConfigurationsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_configurations_get({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_configuration_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        configuration_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_configurations_configuration_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            configuration_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_configuration_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        configuration_id: String,
        entity_collection_entity_id_configurations_configuration_id_put_request: models::EntityCollectionEntityIdConfigurationsConfigurationIdPutRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse, ApiError> {
        info!(
            "entity_collection_entity_id_configurations_configuration_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            configuration_id,
            entity_collection_entity_id_configurations_configuration_id_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_categories_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataCategoriesGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_categories_get(\"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        let response = EntityCollectionEntityIdDataCategoriesGet200Response {
            items: vec!["sysInfo".to_string()],
        };

        Ok(EntityCollectionEntityIdDataCategoriesGetResponse::TheRequestWasSuccessful(response))
    }

    async fn entity_collection_entity_id_data_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        groups: Option<String>,
        category: Option<&Vec<String>>,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_get(\"{}\", \"{}\", {:?}, {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            groups,
            category,
            include_schema,
            context.get().0.clone()
        );

        let resource_names = ["CPU", "Disk", "Memory", "All"];
        let last_dash_index = entity_id.rfind('-').unwrap_or(0);
        let entity_id_cleaned = entity_id[..last_dash_index].to_string();
        let mut items = Vec::new();

        match entity_collection {
            EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps => {
                for resource_name in &resource_names {
                    let id = resource_name.to_lowercase().to_string();
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name, entity_collection, entity_id_cleaned
                    );
                    let value_metadata = EntityCollectionEntityIdDataGet200ResponseItemsInner::new(
                        id,
                        name,
                        "sysInfo".to_string(),
                    );
                    items.push(value_metadata);
                }
            }

            EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components => {
                for resource_name in &resource_names {
                    let id = format!("{}-{}", entity_id, resource_name.to_lowercase());
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name, entity_collection, entity_id_cleaned
                    );
                    let value_metadata = EntityCollectionEntityIdDataGet200ResponseItemsInner::new(
                        id,
                        name,
                        "sysInfo".to_string(),
                    );
                    items.push(value_metadata);
                }
            }

            _ => {
                info!("Default case");
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "NotYetImplemented".to_string(),
                    message: "Not yet implemented.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                return Ok(
                    EntityCollectionEntityIdDataGetResponse::AnUnexpectedRequestOccurred(error),
                );
            }
        }

        let response = EntityCollectionEntityIdDataGet200Response::new(items);
        Ok(EntityCollectionEntityIdDataGetResponse::TheRequestWasSuccessful(response))
    }

    async fn entity_collection_entity_id_data_groups_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataGroupsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_groups_get(\"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );

        // Lock mutex and retrieve data
        let response_mutex = IDENT_DATA_RESPONSE.lock().unwrap();
        let response_vec = response_mutex.clone(); // Here we copy the mutex content into a new Vec<ValueGroup>

        // Call process_json_data synchronously
        match group_by_writability(&response_vec) {
            Ok(result) => Ok(result),
            Err(_error) => Err(ApiError("Generic failure".into())),
        }
    }

    async fn entity_collection_entity_id_data_lists_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataListsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_lists_get({:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_data_lists_post_request: models::EntityCollectionEntityIdDataListsPostRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataListsPostResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_lists_post({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            entity_collection_entity_id_data_lists_post_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_data_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataDataIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_data_id_get(\"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            data_id,
            include_schema,
            context.get().0.clone()
        );

        if let Some(server_config) = SERVER_CONFIG.get() {
            match entity_collection {
                EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components => {
                    let component_name = &entity_id;

                    match component_name.as_str() {
                        "telematics" => {
                            let resource = get_last_part_after_dash(&data_id);
                            let response = handle_system_resource(
                                resource.as_str(),
                                component_name,
                                data_id.as_str(),
                            );
                            Ok(response)
                        }

                        "chassis-hpc" => {
                            match server_config.get_sovd_mode() {
                                "gateway" => {
                                    let mdns = ServiceDaemonWrapper::new(
                                        ServiceDaemon::new().expect("Failed to create daemon"),
                                    );
                                    let instance_name =
                                        server_config.get_instance_name_for_standalone();

                                    if let Some(instance_name) = instance_name {
                                        if let Some((ip_address, port)) =
                                            server_config.get_ip_and_port(&mdns, &instance_name)
                                        {
                                            let uri_get_components = format!(
                                                "http://{}:{}/v1/components",
                                                ip_address, port
                                            );

                                            let uri = format!(
                                                "{}/{}/data/{}",
                                                uri_get_components, component_name, data_id
                                            );
                                            // drop(mdns);
                                            let mut headers = HeaderMap::new();
                                            headers.insert(
                                                "Accept",
                                                HeaderValue::from_static("application/json"),
                                            );

                                            match gateway_request(
                                                uri,
                                                hyper::Method::GET,
                                                headers,
                                                None,
                                            )
                                            .await
                                            {
                                                Ok(response) => {
                                                    let response_body = response.into_body();
                                                    let body_bytes = match hyper::body::to_bytes(
                                                        response_body,
                                                    )
                                                    .await
                                                    {
                                                        Ok(bytes) => bytes,
                                                        Err(err) => {
                                                            let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayRequestBodyConversionError".to_string(),
                                                            message: format!("Failed to convert response body: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                        }
                                                    };

                                                    let body_str = match String::from_utf8(
                                                        body_bytes.to_vec(),
                                                    ) {
                                                        Ok(str) => str,
                                                        Err(err) => {
                                                            let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayResponseBodyConversionError".to_string(),
                                                            message: format!("Failed to convert response body to string: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                        }
                                                    };

                                                    let json_value: JsonValue =
                                                        match serde_json::from_str(&body_str) {
                                                            Ok(value) => value,
                                                            Err(err) => {
                                                                let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayResponseBodyParsingError".to_string(),
                                                            message: format!("Failed to parse response body: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                            }
                                                        };

                                                    if let serde_json::Value::Object(map) =
                                                        json_value
                                                        && let Some(data_value) = map.get("data")
                                                    {
                                                        let mut data_map: Map<String, Value> =
                                                            Map::new();
                                                        data_map.insert(
                                                            "data".to_string(),
                                                            data_value.clone(),
                                                        );

                                                        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                                                            id: map["id"].as_str().unwrap_or_default().to_string(),
                                                            data: to_value(data_map).expect("Failed to filter writables"),
                                                            errors: None,
                                                            schema: None,
                                                        };
                                                        return Ok(EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value));
                                                    }

                                                    let error = AnyPathDocsGetDefaultResponse {
                                                        error_code: "ResourceNotAvailable"
                                                            .to_string(),
                                                        message: "Resource not available."
                                                            .to_string(),
                                                        vendor_code: None,
                                                        translation_id: None,
                                                        parameters: None,
                                                    };
                                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                                }
                                                Err(_) => {
                                                    let error = AnyPathDocsGetDefaultResponse {
                                                        error_code: "GatewayRequestFailed"
                                                            .to_string(),
                                                        message:
                                                            "Failed to fetch data from gateway."
                                                                .to_string(),
                                                        vendor_code: None,
                                                        translation_id: None,
                                                        parameters: None,
                                                    };
                                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                                }
                                            }
                                        } else {
                                            let error = AnyPathDocsGetDefaultResponse {
                                                error_code: "InstanceNotFound".to_string(),
                                                message: "Instance not found.".to_string(),
                                                vendor_code: None,
                                                translation_id: None,
                                                parameters: None,
                                            };
                                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                        }
                                    } else {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "StandaloneInstanceNotFound".to_string(),
                                            message: "Standalone instance not found.".to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                }
                                "standalone" => {
                                    let resource = get_last_part_after_dash(&data_id);
                                    let response = handle_system_resource(
                                        resource.as_str(),
                                        component_name,
                                        data_id.as_str(),
                                    );
                                    Ok(response)
                                }
                                _ => {
                                    let error = AnyPathDocsGetDefaultResponse {
                                        error_code: "GateWayModeNotFound".to_string(),
                                        message: "This gateway mode is not allowed.".to_string(),
                                        vendor_code: None,
                                        translation_id: None,
                                        parameters: None,
                                    };
                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                }
                            }
                        }
                        _ => {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "ComponentNotFound".to_string(),
                                message: "The component was not found.".to_string(),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                        }
                    }
                }
                EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps => {
                    info!(
                        "Apps case: collection-ID {} entity-ID {} data-ID {}",
                        entity_collection, entity_id, data_id
                    );
                    // let resource_to_check = get_before_last_dash(&entity_id);
                    // let pid = get_last_part_after_dash(&entity_id);

                    let tokens = entity_id.split('-');

                    // Check, if last token is a number (is the PID in that case)
                    let last_token = tokens.clone().next_back().unwrap();
                    let pid = match last_token.parse::<u32>() {
                        Ok(pid) => pid.to_string(),
                        Err(_) => "".to_string(),
                    };

                    let mut resource = String::new();
                    for token in tokens {
                        if token.ne(last_token) {
                            resource.push_str(token);
                            resource.push('-');
                        } else if pid.is_empty() {
                            resource.push_str(token);
                        } else {
                            resource.remove(resource.len() - 1);
                        }
                    }

                    if let Some(app) = find_single_process(&resource, &pid, &server_config.base_uri)
                    {
                        let resource = get_last_part_after_dash(&data_id);
                        let tokens = app.id.split('-');
                        let pid_to_monitor = tokens.clone().next_back().unwrap();
                        // let pid_to_monitor = get_last_part_after_dash(&entity_id);
                        let app_name = get_first_part_after_dash(&entity_id);
                        let response = handle_app_resource(
                            resource.as_str(),
                            pid_to_monitor,
                            app_name.as_str(),
                            data_id.as_str(),
                        );

                        Ok(response)
                    } else if server_config.get_sovd_mode() == "gateway" {
                        let mdns = ServiceDaemonWrapper::new(
                            ServiceDaemon::new().expect("Failed to create daemon"),
                        );
                        let instance_name = server_config.get_instance_name_for_standalone();

                        if let Some(instance_name) = instance_name {
                            if let Some((ip_address, port)) =
                                server_config.get_ip_and_port(&mdns, &instance_name)
                            {
                                let uri = format!(
                                    "http://{}:{}/v1/apps/{}/data/{}",
                                    ip_address, port, entity_id, data_id
                                );
                                // drop(mdns);
                                let mut headers = HeaderMap::new();
                                headers
                                    .insert("Accept", HeaderValue::from_static("application/json"));

                                match gateway_request(uri, hyper::Method::GET, headers, None).await
                                {
                                    Ok(response) => {
                                        let response_body = response.into_body();
                                        let od_body_bytes = match hyper::body::to_bytes(
                                            response_body,
                                        )
                                        .await
                                        {
                                            Ok(bytes) => bytes,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayRequestBodyConversionError"
                                                        .to_string(),
                                                    message: format!(
                                                        "Failed to convert response body: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let od_body_str = match String::from_utf8(
                                            od_body_bytes.to_vec(),
                                        ) {
                                            Ok(str) => str,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code:
                                                        "GatewayResponseBodyConversionError"
                                                            .to_string(),
                                                    message: format!(
                                                        "Failed to convert response body to string: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let json_value: JsonValue = match serde_json::from_str(
                                            &od_body_str,
                                        ) {
                                            Ok(value) => value,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError"
                                                        .to_string(),
                                                    message: format!(
                                                        "Failed to parse response body:: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        if let serde_json::Value::Object(map) = json_value
                                            && let Some(data_value) = map.get("data")
                                        {
                                            let mut data: Map<String, Value> = Map::new();
                                            data.insert("data".to_string(), data_value.clone());
                                            let read_value =
                                                EntityCollectionEntityIdDataDataIdGet200Response {
                                                    id: map["id"]
                                                        .as_str()
                                                        .unwrap_or_default()
                                                        .to_string(),
                                                    data: to_value(data)
                                                        .expect("Failed to filter writables"),
                                                    errors: None,
                                                    schema: None,
                                                };
                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value));
                                        }

                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "ResourceNotAvailable".to_string(),
                                            message: "Resource not available.".to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                    Err(_) => {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayRequestFailed".to_string(),
                                            message: "Failed to fetch data from gateway."
                                                .to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                }
                            } else {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "IPAndPortResolutionFailed".to_string(),
                                    message:
                                        "Failed to resolve IP and port for the given instance."
                                            .to_string(),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None,
                                };
                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                            }
                        } else {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "InstanceNameNotFound".to_string(),
                                message: "No standalone instance name found.".to_string(),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                        }
                    } else {
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "ProcessNotFound".to_string(),
                            message: "The process was not found.".to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                    }
                }
                _ => {
                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "EntityCollectionNotFound".to_string(),
                        message: "The entity collection was not found.".to_string(),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None,
                    };
                    Ok(
                        EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(
                            error,
                        ),
                    )
                }
            }
        } else {
            info!("Server configuration not initialized!");
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "ServerConfigurationNotInitialized".to_string(),
                message: "Server configuration not initialized.".to_string(),
                vendor_code: None,
                translation_id: None,
                parameters: None,
            };
            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
        }
    }

    async fn entity_collection_entity_id_data_data_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_id: String,
        entity_collection_entity_id_data_data_id_put_request: models::EntityCollectionEntityIdDataDataIdPutRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataDataIdPutResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_data_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            data_id,
            entity_collection_entity_id_data_data_id_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_data_list_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_list_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdDeleteResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_lists_data_list_id_delete({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            data_list_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_data_list_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_list_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_data_lists_data_list_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            data_list_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn areas_area_id_related_components_get(
        &self,
        area_id: String,
        context: &C,
    ) -> Result<AreasAreaIdRelatedComponentsGetResponse, ApiError> {
        info!(
            "areas_area_id_related_components_get(\"{}\") - X-Span-ID: {:?}",
            area_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn areas_area_id_subareas_get(
        &self,
        area_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<AreasAreaIdSubareasGetResponse, ApiError> {
        info!(
            "areas_area_id_subareas_get(\"{}\", {:?}) - X-Span-ID: {:?}",
            area_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn components_component_id_related_apps_get(
        &self,
        component_id: String,
        context: &C,
    ) -> Result<ComponentsComponentIdRelatedAppsGetResponse, ApiError> {
        info!(
            "components_component_id_related_apps_get(\"{}\") - X-Span-ID: {:?}",
            component_id,
            context.get().0.clone()
        );

        if let Some(server_config) = SERVER_CONFIG.get() {
            // Check if the SOVD mode is "gateway"
            if server_config.get_sovd_mode() == "gateway" {
                let mdns = ServiceDaemonWrapper::new(
                    ServiceDaemon::new().expect("Failed to create daemon"),
                );
                let instance_name = server_config.get_instance_name_for_standalone();

                if let Some(instance_name) = instance_name {
                    if let Some((ip_address, port)) =
                        server_config.get_ip_and_port(&mdns, &instance_name)
                    {
                        // drop(mdns);
                        // Check if the host is available
                        if is_host_available(&ip_address, port).await {
                            if component_id == "telematics" {
                                let mut response_items = Vec::new();
                                let empty_vec = Vec::new();

                                // Only for the current component
                                if server_config.host_name == component_id {
                                    let sovd_apps_list = server_config
                                        .get_apps_by_component_id(component_id.as_str())
                                        .unwrap_or(&empty_vec);

                                    // Extract search terms from the sovd_apps_list
                                    let search_terms: Vec<&str> =
                                        sovd_apps_list.iter().map(AsRef::as_ref).collect();

                                    // Use the new function to search for processes
                                    let found_entities =
                                        find_processes(search_terms, &server_config.base_uri);

                                    // Add the found entities to the response list
                                    response_items.extend(found_entities);

                                    // Debug output
                                    for entity in &response_items {
                                        info!("Found app: {:?}", entity);
                                    }

                                    if response_items.is_empty() {
                                        info!("No apps found.");
                                    }
                                }

                                // Create the response
                                let response_body =
                                    ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                                        AreasAreaIdRelatedComponentsGet200Response::new(
                                            response_items,
                                        ),
                                    );

                                Ok(response_body)
                            } else {
                                let uri_get_related_apps = format!(
                                    "http://{}:{}/v1/components/{}/related-apps",
                                    ip_address, port, component_id
                                );

                                // drop(mdns);
                                let mut headers = HeaderMap::new();
                                headers
                                    .insert("Accept", HeaderValue::from_static("application/json"));

                                match gateway_request(
                                    uri_get_related_apps,
                                    hyper::Method::GET,
                                    headers,
                                    None,
                                )
                                .await
                                {
                                    // Process successful response
                                    Ok(response) => {
                                        let response_body = response.into_body();
                                        let body_bytes: Bytes = match hyper::body::to_bytes(
                                            response_body,
                                        )
                                        .await
                                        {
                                            Ok(bytes) => bytes,
                                            Err(err) => {
                                                // Error handling for failed gateway request
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayRequestBodyConversionError"
                                                        .to_string(),
                                                    message: format!(
                                                        "Failed to convert response body: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let body_str = match String::from_utf8(body_bytes.to_vec())
                                        {
                                            Ok(str) => str,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code:
                                                        "GatewayResponseBodyConversionError"
                                                            .to_string(),
                                                    message: format!(
                                                        "Failed to convert response body to string: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let json_value: JsonValue = match serde_json::from_str(
                                            &body_str,
                                        ) {
                                            Ok(value) => value,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError"
                                                        .to_string(),
                                                    message: format!(
                                                        "Failed to parse response body: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let response_items: Vec<
                                            EntityCollectionGet200ResponseItemsInner,
                                        > = match json_value.get("items") {
                                            Some(items) => {
                                                match serde_json::from_value(items.clone()) {
                                                    Ok(items) => items,
                                                    Err(err) => {
                                                        let error = AnyPathDocsGetDefaultResponse {
                                                            error_code:
                                                                "GatewayResponseBodyParsingError"
                                                                    .to_string(),
                                                            message: format!(
                                                                "Failed to parse 'items' array: {}",
                                                                err
                                                            ),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None,
                                                        };
                                                        return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                                    }
                                                }
                                            }
                                            None => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError"
                                                        .to_string(),
                                                    message:
                                                        "Response body does not contain 'items' arra"
                                                    .to_string(),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        // Extracting id, name, and constructing href
                                        let mut extracted_items = Vec::new();
                                        for item in response_items.iter() {
                                            let id: String = item.id.clone();
                                            let name = item.name.clone();
                                            // Assuming base_uri is already defined in your context
                                            let href =
                                                format!("{}/apps/{}", server_config.base_uri, id);
                                            extracted_items.push(
                                                EntityCollectionGet200ResponseItemsInner::new(
                                                    id, name, href,
                                                ),
                                            );
                                        }

                                        let response_body = ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                                            AreasAreaIdRelatedComponentsGet200Response::new(extracted_items),
                                        );

                                        Ok(response_body)
                                    }
                                    Err(_) => {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayRequestFailed".to_string(),
                                            message: "Failed to fetch data from gateway."
                                                .to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                }
                            }
                        } else if component_id == "chassis-hpc" {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "GatewayRequestGatewayDown".to_string(),
                                message: "Failed to connect".to_string(),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                        } else {
                            // Implementation for other cases (if host is not available and not chassis-hpc)
                            let mut response_items = Vec::new();
                            let empty_vec = Vec::new();

                            // Only for the current component
                            if server_config.host_name == component_id {
                                let sovd_apps_list = server_config
                                    .get_apps_by_component_id(component_id.as_str())
                                    .unwrap_or(&empty_vec);

                                // Extract search terms from the sovd_apps_list
                                let search_terms: Vec<&str> =
                                    sovd_apps_list.iter().map(AsRef::as_ref).collect();

                                // Use the new function to search for processes
                                let found_entities =
                                    find_processes(search_terms, &server_config.base_uri);

                                // Add the found entities to the response list
                                response_items.extend(found_entities);

                                // Debug output
                                for entity in &response_items {
                                    info!("Found app: {:?}", entity);
                                }

                                if response_items.is_empty() {
                                    info!("No apps found.");
                                }
                            }

                            // Create the response
                            let response_body =
                                ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                                    AreasAreaIdRelatedComponentsGet200Response::new(response_items),
                                );

                            Ok(response_body)
                        }
                    } else if component_id == "telematics" {
                        let mut response_items = Vec::new();
                        let empty_vec = Vec::new();

                        // Only for the current component
                        if server_config.host_name == component_id {
                            let sovd_apps_list = server_config
                                .get_apps_by_component_id(component_id.as_str())
                                .unwrap_or(&empty_vec);

                            // Extract search terms from the sovd_apps_list
                            let search_terms: Vec<&str> =
                                sovd_apps_list.iter().map(AsRef::as_ref).collect();

                            // Use the new function to search for processes
                            let found_entities =
                                find_processes(search_terms, &server_config.base_uri);

                            // Add the found entities to the response list
                            response_items.extend(found_entities);

                            // Debug output
                            for entity in &response_items {
                                info!("Found app: {:?}", entity);
                            }

                            if response_items.is_empty() {
                                info!("No apps found.");
                            }
                        }

                        // Create the response
                        let response_body =
                            ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                                AreasAreaIdRelatedComponentsGet200Response::new(response_items),
                            );

                        Ok(response_body)
                    } else {
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "InstanceResolutionFailed".to_string(),
                            message: "Failed to resolve IP and port for the given instance."
                                .to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                    }
                } else {
                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "InstanceNameNotFound".to_string(),
                        message: "No standalone instance name found.".to_string(),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None,
                    };
                    Ok(
                        ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(
                            error,
                        ),
                    )
                }
            } else {
                // Implementation for other cases (if SOVD mode is not "gateway")
                // Load the app data
                let mut response_items = Vec::new();
                let empty_vec = Vec::new();
                info!(
                    "component_id {} host {}",
                    component_id, server_config.host_name
                );
                // Only for the current component
                if server_config.host_name == component_id {
                    let sovd_apps_list = server_config
                        .get_apps_by_component_id(component_id.as_str())
                        .unwrap_or(&empty_vec);

                    // Extract search terms from the sovd_apps_list
                    let search_terms: Vec<&str> =
                        sovd_apps_list.iter().map(AsRef::as_ref).collect();

                    // Use the new function to search for processes
                    let found_entities = find_processes(search_terms, &server_config.base_uri);

                    // Add the found entities to the response list
                    response_items.extend(found_entities);

                    // Debug output
                    for entity in &response_items {
                        info!("Found app: {:?}", entity);
                    }

                    if response_items.is_empty() {
                        info!("No apps found.");
                    }
                }

                // Create the response
                let response_body = ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                    AreasAreaIdRelatedComponentsGet200Response::new(response_items),
                );

                Ok(response_body)
            }
        } else {
            // Error handling for uninitialized server configuration
            info!("Server configuration not initialized!");
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "ServerConfigurationNotInitialized".to_string(),
                message: "Server configuration not initialized.".to_string(),
                vendor_code: None,
                translation_id: None,
                parameters: None,
            };
            Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
        }
    }

    async fn components_component_id_subcomponents_get(
        &self,
        component_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<ComponentsComponentIdSubcomponentsGetResponse, ApiError> {
        info!(
            "components_component_id_subcomponents_get(\"{}\", {:?}) - X-Span-ID: {:?}",
            component_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionGetResponse, ApiError> {
        info!(
            "entity_collection_get(\"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            include_schema,
            context.get().0.clone()
        );

        // Directly extract from the server_config structure
        if let Some(server_config) = SERVER_CONFIG.get() {
            if entity_collection
                == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components
            {
                // Create EntityReference objects for chassis-hpc and telematics
                let chassis_ref = EntityCollectionGet200ResponseItemsInner::new(
                    "chassis-hpc".to_string(),
                    "Chassis-HPC".to_string(),
                    format!("{}/components/chassis-hpc", server_config.base_uri),
                );
                let telematics_ref = EntityCollectionGet200ResponseItemsInner::new(
                    "telematics".to_string(),
                    "Telematics-HPC".to_string(),
                    format!("{}/components/telematics", server_config.base_uri),
                );

                // Create the vector for the EntityReferences
                let mut entity_references = Vec::new();

                // Add the EntityReferences based on host availability
                if server_config.host_name == "chassis-hpc" {
                    entity_references.push(chassis_ref);
                } else {
                    let mdns = ServiceDaemonWrapper::new(
                        ServiceDaemon::new().expect("Failed to create daemon"),
                    );
                    let instance_name = server_config.get_instance_name_for_standalone();

                    if let Some(instance_name) = instance_name {
                        if let Some((_ip_address, _port)) =
                            server_config.get_ip_and_port(&mdns, &instance_name)
                        {
                            entity_references.push(chassis_ref);
                            entity_references.push(telematics_ref);
                        } else {
                            entity_references.push(telematics_ref);
                        }
                    } else {
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "InstanceNameNotFound".to_string(),
                            message: "No standalone instance name found.".to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        return Ok(EntityCollectionGetResponse::AnUnexpectedRequestOccurred(
                            error,
                        ));
                    }
                }

                // Create InlineResponse200 with the EntityReferences and optionally the schema
                let mut response_body =
                    models::EntityCollectionGet200Response::new(entity_references);
                if let Some(include_schema) = include_schema
                    && include_schema
                {
                    // Set the schema if required
                    response_body.schema = Some(false);
                }

                // Create EntityCollectionGetResponse with ResponseBody
                return Ok(EntityCollectionGetResponse::ResponseBody(response_body));
            }
        } else {
            info!("Server configuration not initialized!");
        }

        // If the value of entity_collection is not "components",
        // return EntityCollectionGetResponse::AnUnexpectedRequestOccurred
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnexpectedRequest".to_string(),
            message: "An unexpected request occurred.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };

        Ok(EntityCollectionGetResponse::AnUnexpectedRequestOccurred(
            error,
        ))
    }

    async fn entity_collection_entity_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_get(\"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );

        if let Some(server_config) = get_server_config() {
            info!("Server configuration initialized!");
            if entity_collection
                == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps
            {
                let tokens = entity_id.split('-');

                // Check, if last token is a number (is the PID in that case)
                let last_token = tokens.clone().next_back().unwrap();
                let pid = match last_token.parse::<u32>() {
                    Ok(pid) => pid.to_string(),
                    Err(_) => "".to_string(),
                };

                let mut resource = String::new();
                for token in tokens {
                    if token.ne(last_token) {
                        resource.push_str(token);
                        resource.push('-');
                    } else if pid.is_empty() {
                        resource.push_str(token);
                    } else {
                        resource.remove(resource.len() - 1);
                    }
                }

                let app_id = entity_id.clone();
                let mut _comp_id = "telematics";

                match server_config.get_component_by_app(&app_id) {
                    Some(component_id) => {
                        info!("Component ID: {}", component_id);
                        _comp_id = component_id;
                    }
                    None => {
                        info!("No component found for app_id: {}", &app_id);
                    }
                }

                if let Some(app) = find_single_process(&resource, &pid, &server_config.base_uri) {
                    let mut response = EntityCollectionEntityIdGet200Response::new(
                        entity_id.clone(),
                        app.name.clone(),
                    );
                    let app_data = format!(
                        "{}/{}/{}/data",
                        server_config.base_uri,
                        entity_collection.clone(),
                        app.id.clone()
                    );

                    response.data = Some(app_data);

                    return Ok(EntityCollectionEntityIdGetResponse::TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                } else {
                    //Check if gateway mode is active, because perhaps the app is on another device
                    if let Some(server_config) = SERVER_CONFIG.get() {
                        if server_config.get_sovd_mode() == "gateway" {
                            let mdns = ServiceDaemonWrapper::new(
                                ServiceDaemon::new().expect("Failed to create daemon"),
                            );
                            let instance_name = server_config.get_instance_name_for_standalone();

                            if let Some(instance_name) = instance_name {
                                if let Some((ip_address, port)) =
                                    server_config.get_ip_and_port(&mdns, &instance_name)
                                {
                                    // drop(mdns);
                                    if is_host_available(&ip_address, port).await {
                                        // Host is available
                                        let uri = format!(
                                            "http://{}:{}/v1/apps/{}",
                                            ip_address, port, entity_id
                                        );

                                        // drop(mdns);
                                        let mut headers = HeaderMap::new();
                                        headers.insert(
                                            "Accept",
                                            HeaderValue::from_static("application/json"),
                                        );

                                        match gateway_request(
                                            uri,
                                            hyper::Method::GET,
                                            headers,
                                            None,
                                        )
                                        .await
                                        {
                                            // Process successful response
                                            Ok(response) => {
                                                let response_body = response.into_body();
                                                let od_body_bytes: Bytes =
                                                    match hyper::body::to_bytes(response_body).await
                                                    {
                                                        Ok(bytes) => bytes,
                                                        Err(err) => {
                                                            // Error handling for failed gateway request
                                                            let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayRequestBodyConversionError".to_string(),
                                                            message: format!("Failed to convert response body: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                            return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                        }
                                                    };

                                                let od_body_str = match String::from_utf8(
                                                    od_body_bytes.to_vec(),
                                                ) {
                                                    Ok(str) => str,
                                                    Err(err) => {
                                                        let error = AnyPathDocsGetDefaultResponse {
                                                            error_code:
                                                                "GatewayResponseBodyConversionError"
                                                                    .to_string(),
                                                            message: format!(
                                                                "Failed to convert response body to string: {}",
                                                                err
                                                            ),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None,
                                                        };
                                                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                    }
                                                };

                                                let mut json_value: JsonValue =
                                                    match serde_json::from_str(&od_body_str) {
                                                        Ok(value) => value,
                                                        Err(err) => {
                                                            let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayResponseBodyParsingError".to_string(),
                                                            message: format!("Failed to parse response body: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                            return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                        }
                                                    };
                                                let extracted_data =
                                                    extract_response_data_from_json_to_response(
                                                        &mut json_value,
                                                        server_config.get_base_uri(),
                                                    );

                                                return Ok(extracted_data);
                                            }
                                            Err(_) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayRequestFailed".to_string(),
                                                    message: "Failed to fetch data from gateway."
                                                        .to_string(),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        }
                                    } else {
                                        // Gateway down
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayDown".to_string(),
                                            message: "Failed to connect to gateway.".to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                    }
                                } else {
                                    let error = AnyPathDocsGetDefaultResponse {
                                        error_code: "IPAndPortResolutionFailed".to_string(),
                                        message:
                                            "Failed to resolve IP and port for the given instance."
                                                .to_string(),
                                        vendor_code: None,
                                        translation_id: None,
                                        parameters: None,
                                    };
                                    return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                }
                            } else {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "InstanceNameNotFound".to_string(),
                                    message: "No standalone instance name found.".to_string(),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None,
                                };
                                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                            }
                        } else {
                            // Implementation for other cases (if SOVD mode is not "gateway")
                        }
                    } else {
                        // Error handling for uninitialized server configuration
                        info!("Server configuration not initialized!");
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "ServerConfigurationNotInitialized".to_string(),
                            message: "Server configuration not initialized.".to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        return Ok(
                            EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error),
                        );
                    }

                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "EntityNotFound".to_string(),
                        message: format!("Entity '{}' not found.", entity_id),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None,
                    };
                    return Ok(
                        EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error),
                    );
                }
            } else if entity_collection
                == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components
            {
                // Declaration of id and name as Option
                let mut id: Option<String> = None;
                let mut name: Option<String> = None;

                // Call entity_collection_get and process the response
                match self
                    .entity_collection_get(entity_collection, None, context)
                    .await
                {
                    Ok(EntityCollectionGetResponse::ResponseBody(response_body)) => {
                        // Extract the required data from the response
                        for entity_ref in &response_body.items {
                            if entity_ref.id == entity_id {
                                // If the matching entity is found, set id and name
                                id = Some(entity_ref.id.clone());
                                name = Some(entity_ref.name.clone());
                                break;
                            }
                        }

                        // Check if a matching entity was found
                        if let (Some(id), Some(name)) = (id, name) {
                            let mut response = EntityCollectionEntityIdGet200Response::new(
                                id.clone(),
                                name.clone(),
                            );
                            let app_data = format!(
                                "{}/{}/{}/data",
                                server_config.base_uri,
                                entity_collection.clone(),
                                id.clone()
                            );
                            response.data = Some(app_data);

                            return Ok(EntityCollectionEntityIdGetResponse::TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                        } else {
                            // If no matching entity was found
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "EntityNotFound".to_string(),
                                message: format!("Entity '{}' not found.", entity_id),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            return Ok(
                                EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(
                                    error,
                                ),
                            );
                        }
                    }
                    Err(err) => {
                        // Error while querying entity_collection_get
                        return Err(err);
                    }
                    _ => {
                        // Unexpected response from entity_collection_get
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "UnexpectedResponse".to_string(),
                            message: "Unexpected response from entity_collection_get.".to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        return Ok(
                            EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error),
                        );
                    }
                }
            } else {
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "UnexpectedRequest".to_string(),
                    message: "An unexpected request occurred.".to_string(),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None,
                };
                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
            }
        } else {
            info!("Server configuration not initialized!");
        }

        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnexpectedRequest".to_string(),
            message: "An unexpected request occurred.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };
        Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error))
    }

    async fn delete_all_faults(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        scope: Option<String>,
        context: &C,
    ) -> Result<DeleteAllFaultsResponse, ApiError> {
        info!(
            "delete_all_faults({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            scope,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn get_faults(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        status_left_square_bracket_key_right_square_bracket: Option<String>,
        severity: Option<i32>,
        scope: Option<String>,
        context: &C,
    ) -> Result<GetFaultsResponse, ApiError> {
        info!(
            "get_faults({:?}, \"{}\", {:?}, {:?}, {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            include_schema,
            status_left_square_bracket_key_right_square_bracket,
            severity,
            scope,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn delete_fault_by_id(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        fault_code: String,
        context: &C,
    ) -> Result<DeleteFaultByIdResponse, ApiError> {
        info!(
            "delete_fault_by_id({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            fault_code,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn get_fault_by_id(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        fault_code: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<GetFaultByIdResponse, ApiError> {
        info!(
            "get_fault_by_id({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            fault_code,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLocksGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_locks_get({:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_locks_post_request: models::EntityCollectionEntityIdLocksPostRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLocksPostResponse, ApiError> {
        info!(
            "entity_collection_entity_id_locks_post({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            entity_collection_entity_id_locks_post_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLocksLockIdDeleteResponse, ApiError> {
        info!(
            "entity_collection_entity_id_locks_lock_id_delete({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            lock_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLocksLockIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_locks_lock_id_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            lock_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        entity_collection_entity_id_locks_post_request: models::EntityCollectionEntityIdLocksPostRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLocksLockIdPutResponse, ApiError> {
        info!(
            "entity_collection_entity_id_locks_lock_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            lock_id,
            entity_collection_entity_id_locks_post_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLogsConfigDeleteResponse, ApiError> {
        info!(
            "entity_collection_entity_id_logs_config_delete({:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLogsConfigGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_logs_config_get({:?}, \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_logs_config_put_request: models::EntityCollectionEntityIdLogsConfigPutRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLogsConfigPutResponse, ApiError> {
        info!(
            "entity_collection_entity_id_logs_config_put({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            entity_collection_entity_id_logs_config_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_entries_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        severity: Option<models::EntityCollectionEntityIdLogsEntriesGetSeverityParameter>,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdLogsEntriesGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_logs_entries_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            severity,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_proximity_proof: Option<bool>,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdOperationsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_operations_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            include_proximity_proof,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        context: &C,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_operations_operation_id_executions_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            operation_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        entity_collection_entity_id_operations_operation_id_executions_post_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsPostRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse, ApiError> {
        info!(
            "entity_collection_entity_id_operations_operation_id_executions_post({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            operation_id,
            entity_collection_entity_id_operations_operation_id_executions_post_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_operations_operation_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            operation_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        execution_id: String,
        entity_collection_entity_id_operations_operation_id_executions_execution_id_delete_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteRequest,
        context: &C,
    ) -> Result<
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse,
        ApiError,
    > {
        info!("entity_collection_entity_id_operations_operation_id_executions_execution_id_delete({:?}, \"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, execution_id, entity_collection_entity_id_operations_operation_id_executions_execution_id_delete_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        execution_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse,
        ApiError,
    > {
        info!(
            "entity_collection_entity_id_operations_operation_id_executions_execution_id_get({:?}, \"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            operation_id,
            execution_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        execution_id: String,
        entity_collection_entity_id_operations_operation_id_executions_execution_id_put_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutRequest,
        context: &C,
    ) -> Result<
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse,
        ApiError,
    > {
        info!(
            "entity_collection_entity_id_operations_operation_id_executions_execution_id_put({:?}, \"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            operation_id,
            execution_id,
            entity_collection_entity_id_operations_operation_id_executions_execution_id_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdModesGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_modes_get({:?}, \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_mode_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        mode_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<EntityCollectionEntityIdModesModeIdGetResponse, ApiError> {
        info!(
            "entity_collection_entity_id_modes_mode_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            mode_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_mode_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        mode_id: String,
        entity_collection_entity_id_modes_mode_id_put_request: models::EntityCollectionEntityIdModesModeIdPutRequest,
        context: &C,
    ) -> Result<EntityCollectionEntityIdModesModeIdPutResponse, ApiError> {
        info!(
            "entity_collection_entity_id_modes_mode_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            mode_id,
            entity_collection_entity_id_modes_mode_id_put_request,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_get(
        &self,
        target_version: Option<String>,
        origin: Option<models::UpdatesGetOriginParameter>,
        context: &C,
    ) -> Result<UpdatesGetResponse, ApiError> {
        info!(
            "updates_get({:?}, {:?}) - X-Span-ID: {:?}",
            target_version,
            origin,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_post(
        &self,
        content_type: Option<String>,
        body: Option<serde_json::Value>,
        context: &C,
    ) -> Result<UpdatesPostResponse, ApiError> {
        info!(
            "updates_post({:?}, {:?}) - X-Span-ID: {:?}",
            content_type,
            body,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_automated_put(
        &self,
        update_package_id: String,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdAutomatedPutResponse, ApiError> {
        info!(
            "updates_update_package_id_automated_put(\"{}\") - X-Span-ID: {:?}",
            update_package_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_delete(
        &self,
        update_package_id: String,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdDeleteResponse, ApiError> {
        info!(
            "updates_update_package_id_delete(\"{}\") - X-Span-ID: {:?}",
            update_package_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_execute_put(
        &self,
        update_package_id: String,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdExecutePutResponse, ApiError> {
        info!(
            "updates_update_package_id_execute_put(\"{}\") - X-Span-ID: {:?}",
            update_package_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_get(
        &self,
        update_package_id: String,
        include_schema: Option<bool>,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdGetResponse, ApiError> {
        info!(
            "updates_update_package_id_get(\"{}\", {:?}) - X-Span-ID: {:?}",
            update_package_id,
            include_schema,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_prepare_put(
        &self,
        update_package_id: String,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdPreparePutResponse, ApiError> {
        info!(
            "updates_update_package_id_prepare_put(\"{}\") - X-Span-ID: {:?}",
            update_package_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_status_get(
        &self,
        update_package_id: String,
        context: &C,
    ) -> Result<UpdatesUpdatePackageIdStatusGetResponse, ApiError> {
        info!(
            "updates_update_package_id_status_get(\"{}\") - X-Span-ID: {:?}",
            update_package_id,
            context.get().0.clone()
        );
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use {
        EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter as ColParam,
        EntityCollectionEntityIdBulkDataGetResponse as BulkResp,
        EntityCollectionEntityIdDataDataIdGetResponse as DataIdResp,
        ComponentsComponentIdRelatedAppsGetResponse as AppsResp,
        EntityCollectionEntityIdDataGetResponse as DataResp,
        EntityCollectionGetResponse as EntityResp,
        AnyPathDocsGetDefaultResponse as ErrBody,
        serde_json::Value as Value,
    };

    //Mock struct for tests
    #[derive(Clone, Debug)]
    struct TestContext(XSpanIdString);

    //Mock TestContext for tests
    impl Has<XSpanIdString> for TestContext {
        fn get(&self) -> &XSpanIdString { &self.0 }
        fn get_mut(&mut self) -> &mut XSpanIdString { &mut self.0 }
        fn set(&mut self, v: XSpanIdString) { self.0 = v; }
    }

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
    fn make_server<C>() -> Server<C> { Server { marker: PhantomData } }




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
 
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-1".into()));

        let rsp = server
            .entity_collection_entity_id_bulk_data_get(ColParam::Apps, "id-1".into(), None, &ctx)
            .await
            .expect("Fail for entity_collection_entity_id_bulk_data_get");

        match rsp {
            BulkResp::TheBulkDataCategoriesSupportedByTheEntity(body) => {
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
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-2".into()));

        let rsp = server
            .entity_collection_entity_id_bulk_data_get(ColParam::Apps, "id-2".into(), Some(true), &ctx)
            .await
            .expect("Fail for entity_collection_entity_id_bulk_data_get");

        match rsp {
            BulkResp::TheBulkDataCategoriesSupportedByTheEntity(body) => {
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

        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-a".into()));

        let entity_id = "chassis-hpc";
        let rsp = server
            .entity_collection_entity_id_data_get(
                ColParam::Apps,
                entity_id.to_string(),
                None,
                None,
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_get");

        match rsp {
            DataResp::TheRequestWasSuccessful(body) => {
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

        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-b".into()));

        let entity_id = "comp-xyz-7";
        let rsp = server
            .entity_collection_entity_id_data_get(
                ColParam::Components,
                entity_id.to_string(),
                None, 
                None,  
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_get");

        match rsp {
            DataResp::TheRequestWasSuccessful(body) => {
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

        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let rsp = server
            .entity_collection_entity_id_data_get(
                ColParam::Functions,
                "abc".into(),
                None,
                None,
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_get");

        match rsp {
            DataResp::AnUnexpectedRequestOccurred(err) => {
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

        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-ok".into()));

        let test = vec![Value::Bool(true)];

        let expected = group_by_writability(&test).expect("should succeed for test data");

       
        let got = server
            .entity_collection_entity_id_data_groups_get(
                ColParam::Apps,
                "entity-123".to_string(),
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_groups_get");

        
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

        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let data_id = "veh-01".to_string();
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                ColParam::Components,
                "telematics".into(),
                data_id,
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::AnUnexpectedRequestOccurred(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("noprocess"));
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let data_id = "veh-01-cpu".to_string();
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                ColParam::Apps,
                "noprocess".into(),
                data_id.clone(),
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::AnUnexpectedRequestOccurred(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

    
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                ColParam::Apps,
                format!("sovd_server-{}", std::process::id()),
                "sovd_server".to_string(),
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::AnUnexpectedRequestOccurred(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

    
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                ColParam::Apps,
                format!("sovd_server-{}", std::process::id()),
                "sovd_server-cpu".to_string(),
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::TheRequestWasSuccessful(body) => {
                
                let data = json!({
                    "cpu_usage": body.data.get("cpu_usage").and_then(|val| val.as_str()),
                    "description": "CPU usage for sovd_server",
                    "name": "CPU"
                });

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

        ensure_server_config(String::from("no_process"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

    
        let rsp = server
            .entity_collection_entity_id_data_data_id_get(
                ColParam::Functions,
                format!("sovd_server-{}", std::process::id()),
                "sovd_server".to_string(),
                None,
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_entity_id_data_data_id_get");

        match rsp {
            DataIdResp::AnUnexpectedRequestOccurred(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let component_id = "chassis-hpc".to_string();
        let rsp = server
            .components_component_id_related_apps_get(
                component_id,
                &ctx,
            )
            .await
            .expect("Fail for components_component_id_related_apps_get");

        match rsp {
            AppsResp::ResponseBody(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let rsp = server
            .entity_collection_get(
                ColParam::Components,
                Some(true),
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_get");

        match rsp {
            EntityResp::ResponseBody(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let rsp = server
            .entity_collection_get(
                ColParam::Components,
                Some(false),
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_get");

        match rsp {
            EntityResp::ResponseBody(body) => {
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

        ensure_server_config(String::from("standalone"), String::from("chassis-hpc"));
        
        let server = make_server::<TestContext>();
        let ctx = TestContext(XSpanIdString("span-c".into()));

        let rsp = server
            .entity_collection_get(
                ColParam::Apps,
                Some(false),
                &ctx,
            )
            .await
            .expect("Fail for entity_collection_get");

        match rsp {
            EntityResp::AnUnexpectedRequestOccurred(body) => {
                let error_code = "UnexpectedRequest".to_string();
                assert_eq!(body.error_code, error_code);
            }
            other => panic!("unexpected variant: {:?}", other)
        }
            
    }
    

}