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

extern crate libc;
extern crate procfs;

use std::process::Command;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt; // Import for gethostname function
use std::path::Path;
use serde_json::Value;
use std::process::Stdio;
use async_trait::async_trait;
use futures::{future, Stream, StreamExt, TryFutureExt, TryStreamExt};
use hyper::{Body, Request, Response, header};
use hyper::server::conn::Http;
use hyper::http;
use hyper::service::Service;
use hyper::body::Bytes;
use log::{info, warn, error};
use serde_json::error::Category;
use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use swagger::{Has, XSpanIdString};
use swagger::auth::MakeAllowAllAuthenticator;
use swagger::EmptyContext;
use tokio::net::TcpListener;
use openssl::ssl::{Ssl, SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod};
use regex::Regex;
use chrono::Utc;
use chrono::DateTime;
use procfs::process::all_processes;
use serde_json::Error as SerdeError;
use std::str::from_utf8;
use std::env;
use std::io::ErrorKind;
use std::collections::BTreeMap; // Import for BTreeMap
use serde_json::{Number, json}; // Import for Number and JSON
use tokio::task;
use tokio_openssl::SslStream;
use hyper::header::HeaderMap;
use std::fs::File;
use std::io::Write;
use hyper::header::{HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_HEADERS};
use std::convert::Infallible;
use std::str::FromStr;
use serde_json::Value as JsonValue;
use serde_json::to_value;
use serde_json::Map;

use openapi_client::models;
use openapi_client::models::*;

use crate::server_config::ServerConfig;
use crate::server_config::ServiceDaemonWrapper;

// Import the required modules
use sovd_handlers::{find_and_create_read_value};
use sovd_handlers::IDENT_DATA_RESPONSE;
use sovd_handlers::filter_by_writable;
use sovd_handlers::create_entity_collection_response;
use sovd_handlers::group_by_writability;
use sovd_handlers::prepare_data_response;
use sovd_handlers::find_processes;
use sovd_handlers::find_single_process;
use sovd_handlers::get_before_last_dash;

use sovd_handlers::find_entity_by_name;
use sovd_handlers::extract_name_and_replace_dashes;
use sovd_handlers::get_last_part_after_dash;
use sovd_handlers::get_disk_io;
use sovd_handlers::get_memory_usage;
use sovd_handlers::get_cpu_usage;
use sovd_handlers::handle_system_resource;
use sovd_handlers::handle_app_resource;
use sovd_handlers::get_first_part_after_dash;
use sovd_handlers::get_system_disk_io;
use sovd_handlers::get_system_memory_usage;
use sovd_handlers::get_system_cpu_usage;
use sovd_handlers::gateway_request;
use sovd_handlers::is_host_available;
use sovd_handlers::update_href_with_base_uri;
use sovd_handlers::extract_response_data_from_json_to_response;


//use vehicle_auth_server;
use std::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
// Global variable for the server configuration
static SERVER_CONFIG: OnceCell<ServerConfig> = OnceCell::new();

// Function to initialize the global server configuration
pub fn init_server_config(config: ServerConfig) {
    SERVER_CONFIG.set(config).expect("Failed to set server config");
}

// Function to access the global server configuration
pub fn get_server_config() -> Option<&'static ServerConfig> {
    SERVER_CONFIG.get()
}

/// Builds an SSL implementation for Simple HTTPS from some hard-coded file names
/// 

#[derive(Debug, Deserialize, Serialize)]
struct ErrorResponse {
    error_code: String,
    message: String,
}

use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};

fn create_m_dns(server_config: &ServerConfig, mdns: &ServiceDaemonWrapper) {

    let hostname = server_config.get_hostname();
    let ip_address = server_config.get_ip_address();

    // Create a service info.
    let service_type = "_sovd_server._udp.local.";//"_mdns-sd-my-test._udp.local.";
    let instance_name = format!("{}_instance", hostname);//"my_instance";
    let ip = ip_address;
    let host_name = format!("{}{}",hostname, service_type);
    let port = server_config.get_port().parse::<u16>().unwrap();
    let properties = [("identification", hostname), ("accessurl", server_config.get_base_uri()), ("sovd_mode", server_config.get_sovd_mode())];

    let my_service = ServiceInfo::new(
        service_type,
        &instance_name.as_str(),
        &host_name.as_str(),
        ip,
        port,
        &properties[..],
    ).unwrap();

    // Register with the daemon, which publishes the service.
    mdns.register(my_service).expect("Failed to register our service");
}

pub async fn get_m_dns_messages(_server_config: Arc<ServerConfig>, mdns: Arc<ServiceDaemonWrapper>) {
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

                    let value = info.get_property("sovd_mode").expect("Failed to get property").val_str();
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
    create_m_dns(&server_config, &mdns_wrapper);

    let arc_server_config = Arc::new(server_config.clone());
    get_m_dns_messages(arc_server_config, Arc::clone(&mdns_wrapper)).await;

    #[allow(unused_mut)]
    let mut service =
        openapi_client::server::context::MakeAddContext::<_, EmptyContext>::new(
            service
        );


    //Start HTTP server
    info!("Starting HTTP server on {}", addr);
    let server = hyper::Server::bind(&addr).serve(service);
    if let Err(e) = server.await {
        error!("Server error: {}", e);
    }
    

    //Wait for Ctrl + C to shutdown server
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    shutdown_signal.store(true, Ordering::Relaxed);

    info!("Shutting down server...");
    // Additional cleanup tasks can be performed here

    info!("Shutting down mDNS server...");
    mdns_wrapper.shutdown();
    
}

#[derive(Copy, Clone)]
pub struct Server<C> {
    marker: PhantomData<C>,
}

impl<C> Server<C> {
    pub fn new() -> Self {
        Server{marker: PhantomData}
    }
}


use swagger::auth::Authorization;


use openapi_client::{
    Api,
    EntityCollectionEntityIdBulkDataGetResponse,
    EntityCollectionEntityIdBulkDataCategoryDeleteResponse,
    EntityCollectionEntityIdBulkDataCategoryGetResponse,
    EntityCollectionEntityIdBulkDataCategoryPostResponse,
    EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse,
    EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse,
    AnyPathDocsGetResponse,
    EntityCollectionEntityIdCommunicationLogsGetResponse,
    EntityCollectionEntityIdCommunicationLogsPostResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse,
    EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse,
    EntityCollectionEntityIdConfigurationsGetResponse,
    EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse,
    EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse,
    EntityCollectionEntityIdDataCategoriesGetResponse,
    EntityCollectionEntityIdDataGetResponse,
    EntityCollectionEntityIdDataGroupsGetResponse,
    EntityCollectionEntityIdDataListsGetResponse,
    EntityCollectionEntityIdDataListsPostResponse,
    EntityCollectionEntityIdDataDataIdGetResponse,
    EntityCollectionEntityIdDataDataIdPutResponse,
    EntityCollectionEntityIdDataListsDataListIdDeleteResponse,
    EntityCollectionEntityIdDataListsDataListIdGetResponse,
    AreasAreaIdRelatedComponentsGetResponse,
    AreasAreaIdSubareasGetResponse,
    ComponentsComponentIdRelatedAppsGetResponse,
    ComponentsComponentIdSubcomponentsGetResponse,
    EntityCollectionGetResponse,
    EntityCollectionEntityIdGetResponse,
    DeleteAllFaultsResponse,
    GetFaultsResponse,
    DeleteFaultByIdResponse,
    GetFaultByIdResponse,
    EntityCollectionEntityIdLocksGetResponse,
    EntityCollectionEntityIdLocksPostResponse,
    EntityCollectionEntityIdLocksLockIdDeleteResponse,
    EntityCollectionEntityIdLocksLockIdGetResponse,
    EntityCollectionEntityIdLocksLockIdPutResponse,
    EntityCollectionEntityIdLogsConfigDeleteResponse,
    EntityCollectionEntityIdLogsConfigGetResponse,
    EntityCollectionEntityIdLogsConfigPutResponse,
    EntityCollectionEntityIdLogsEntriesGetResponse,
    EntityCollectionEntityIdOperationsGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse,
    EntityCollectionEntityIdOperationsOperationIdGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse,
    EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse,
    EntityCollectionEntityIdModesGetResponse,
    EntityCollectionEntityIdModesModeIdGetResponse,
    EntityCollectionEntityIdModesModeIdPutResponse,
    UpdatesGetResponse,
    UpdatesPostResponse,
    UpdatesUpdatePackageIdAutomatedPutResponse,
    UpdatesUpdatePackageIdDeleteResponse,
    UpdatesUpdatePackageIdExecutePutResponse,
    UpdatesUpdatePackageIdGetResponse,
    UpdatesUpdatePackageIdPreparePutResponse,
    UpdatesUpdatePackageIdStatusGetResponse,
};

use openapi_client::server::MakeService;

use std::error::Error;
use swagger::ApiError;


#[async_trait]
impl<C> Api<C> for Server<C> where C: Has<XSpanIdString> + Send + Sync
{
    async fn entity_collection_entity_id_bulk_data_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdBulkDataGetResponse, ApiError>
    {
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
        let response = EntityCollectionEntityIdBulkDataGetResponse::TheBulkDataCategoriesSupportedByTheEntity(inline_response);
    
        // Return the response
        Ok(response)
    }

    async fn entity_collection_entity_id_bulk_data_category_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        context: &C) -> Result<EntityCollectionEntityIdBulkDataCategoryDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_bulk_data_category_delete({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, category, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_bulk_data_category_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdBulkDataCategoryGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_bulk_data_category_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, category, include_schema, context.get().0.clone());
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
        context: &C) -> Result<EntityCollectionEntityIdBulkDataCategoryPostResponse, ApiError>
    {
        info!("entity_collection_entity_id_bulk_data_category_post({:?}, \"{}\", {:?}, \"{}\", {}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, category, content_type, content_length, content_disposition, body, context.get().0.clone());
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "ServerConfigurationNotInitialized".to_string(),
            message: "Server configuration not initialized.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None
        };
        return Ok(EntityCollectionEntityIdBulkDataCategoryPostResponse::AnUnexpectedRequestOccurred(error));
    }

    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        bulk_data_id: String,
        context: &C) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_bulk_data_category_bulk_data_id_delete({:?}, \"{}\", {:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, category, bulk_data_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        category: models::EntityCollectionEntityIdBulkDataGet200ResponseItemsInner,
        bulk_data_id: String,
        accept: Option<String>,
        context: &C) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_bulk_data_category_bulk_data_id_get({:?}, \"{}\", {:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, category, bulk_data_id, accept, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn any_path_docs_get(
        &self,
        any_path: String,
        context: &C) -> Result<AnyPathDocsGetResponse, ApiError>
    {
        info!("any_path_docs_get(\"{}\") - X-Span-ID: {:?}", any_path, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    
    async fn entity_collection_entity_id_communication_logs_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdCommunicationLogsGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_communication_logs_get({:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        body: swagger::ByteArray,
        stream: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdCommunicationLogsPostResponse, ApiError>
    {
        info!("entity_collection_entity_id_communication_logs_post({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, body, stream, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        delete_log: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_delete({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, communication_log_id, delete_log, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        context: &C) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, communication_log_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_communication_logs_communication_log_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        communication_log_id: String,
        entity_collection_entity_id_communication_logs_communication_log_id_put_request: models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutRequest,
        context: &C) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, communication_log_id, entity_collection_entity_id_communication_logs_communication_log_id_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdConfigurationsGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_configurations_get({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_configuration_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        configuration_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_configurations_configuration_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, configuration_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_configurations_configuration_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        configuration_id: String,
        entity_collection_entity_id_configurations_configuration_id_put_request: models::EntityCollectionEntityIdConfigurationsConfigurationIdPutRequest,
        context: &C) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_configurations_configuration_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, configuration_id, entity_collection_entity_id_configurations_configuration_id_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_categories_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdDataCategoriesGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_categories_get(\"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        let response = EntityCollectionEntityIdDataCategoriesGet200Response {
                    items: vec!["sysInfo".to_string()]
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
        context: &C) -> Result<EntityCollectionEntityIdDataGetResponse, ApiError>
    {
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
            EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps =>  {
            for resource_name in &resource_names {
                    let id = format!("{}-{}", entity_id, resource_name.to_lowercase());
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name,
                        entity_collection,
                        entity_id_cleaned
                    );
                    let value_metadata = EntityCollectionEntityIdDataGet200ResponseItemsInner::new(id, name, "sysInfo".to_string());
                    items.push(value_metadata);
                }
            },
            
            EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components =>  {
            for resource_name in &resource_names {
                    let id = format!("{}-{}", entity_id, resource_name.to_lowercase());
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name,
                        entity_collection,
                        entity_id_cleaned
                    );
                    let value_metadata = EntityCollectionEntityIdDataGet200ResponseItemsInner::new(id, name, "sysInfo".to_string());
                    items.push(value_metadata);
                }
            },

            _ => {
                info!("Default case");
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "NotYetImplemented".to_string(),
                    message: format!("Not yet implemented."),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None
                };
                return Ok(EntityCollectionEntityIdDataGetResponse::AnUnexpectedRequestOccurred(error));
            }
        }
    
        let response = EntityCollectionEntityIdDataGet200Response::new(items);
        Ok(EntityCollectionEntityIdDataGetResponse::TheRequestWasSuccessful(response))
    }

    async fn entity_collection_entity_id_data_groups_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdDataGroupsGetResponse, ApiError>
    {
         info!("entity_collection_entity_id_data_groups_get(\"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        
        // Lock mutex and retrieve data
        let response_mutex = IDENT_DATA_RESPONSE.lock().unwrap();
        let response_vec = response_mutex.clone();  // Here we copy the mutex content into a new Vec<ValueGroup>

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
        context: &C) -> Result<EntityCollectionEntityIdDataListsGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_lists_get({:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_data_lists_post_request: models::EntityCollectionEntityIdDataListsPostRequest,
        context: &C) -> Result<EntityCollectionEntityIdDataListsPostResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_lists_post({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, entity_collection_entity_id_data_lists_post_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_data_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdDataDataIdGetResponse, ApiError>
    {
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
                            let response = handle_system_resource(resource.as_str(), component_name, data_id.as_str());
                            Ok(response)
                        }

                        "chassis-hpc" => match server_config.get_sovd_mode() {
                            "gateway" => {
                                let mdns = ServiceDaemonWrapper::new(ServiceDaemon::new().expect("Failed to create daemon"));
                                let instance_name = server_config.get_instance_name_for_standalone();
                            
                                if let Some(instance_name) = instance_name {
                                    if let Some((ip_address, port)) = server_config.get_ip_and_port(&mdns, &instance_name) {
                                        let uri_get_components = format!("http://{}:{}/v1/components", ip_address, port);
                            
                                        let uri = format!(
                                            "{}/{}/data/{}",
                                            uri_get_components,
                                            component_name,
                                            data_id
                                        );
                                        // drop(mdns);
                                        let mut headers = HeaderMap::new();
                                        headers.insert("Accept", HeaderValue::from_static("application/json"));

                                        match gateway_request(uri, hyper::Method::GET, headers, None).await {
                                            Ok(response) => {
                                                let response_body = response.into_body();
                                                let body_bytes = match hyper::body::to_bytes(response_body).await {
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
                            
                                                let body_str = match String::from_utf8(body_bytes.to_vec()) {
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
                            
                                                let json_value: JsonValue = match serde_json::from_str(&body_str) {
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
                            
                                                if let serde_json::Value::Object(map) = json_value {
                                                    if let Some(data_value) = map.get("data") {
                                                        let mut data_map: Map<String, Value> = Map::new();
                                                        data_map.insert("data".to_string(), data_value.clone());

                                                        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                                                            id: map["id"].as_str().unwrap_or_default().to_string(),
                                                            data: to_value(data_map).expect("Failed to filter writables"),
                                                            errors: None,
                                                            schema: None,
                                                        };
                                                        return Ok(EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value));
                                                    }
                                                }
                            
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "ResourceNotAvailable".to_string(),
                                                    message: format!("Resource not available."),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                            }
                                            Err(_) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayRequestFailed".to_string(),
                                                    message: format!("Failed to fetch data from gateway."),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                            }
                                        }
                                        
                                    } else {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "InstanceNotFound".to_string(),
                                            message: format!("Instance not found."),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                } else {
                                    let error = AnyPathDocsGetDefaultResponse {
                                        error_code: "StandaloneInstanceNotFound".to_string(),
                                        message: format!("Standalone instance not found."),
                                        vendor_code: None,
                                        translation_id: None,
                                        parameters: None
                                    };
                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                }
                            
                            }
                            "standalone" => {
                                let resource = get_last_part_after_dash(&data_id);
                                let response = handle_system_resource(resource.as_str(), component_name, data_id.as_str());
                                Ok(response)
                            }
                            _ => {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "GateWayModeNotFound".to_string(),
                                    message: format!("This gateway mode is not allowed."),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None
                                };
                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                            }
                        },
                        _ => {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "ComponentNotFound".to_string(),
                                message: format!("The component was not found."),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None
                            };
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                        }
                    }
                },
                EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps => {
                    info!("Apps case: collection-ID {} entity-ID {} data-ID {}", entity_collection, entity_id, data_id);
                    let resource_to_check = get_before_last_dash(&entity_id);
                    let pid = get_last_part_after_dash(&entity_id);
    
                    if let Some(_app) = find_single_process(&resource_to_check, &pid, &server_config.base_uri) {
                        let resource = get_last_part_after_dash(&data_id);
                        let pid_to_monitor = get_last_part_after_dash(&entity_id);
                        let app_name = get_first_part_after_dash(&entity_id);
    
                        let response = handle_app_resource(resource.as_str(), pid_to_monitor.as_str(), app_name.as_str(), data_id.as_str());
                        Ok(response)
                    } else if server_config.get_sovd_mode() == "gateway" {
                        let mdns = ServiceDaemonWrapper::new(ServiceDaemon::new().expect("Failed to create daemon"));
                        let instance_name = server_config.get_instance_name_for_standalone();
                        
                        if let Some(instance_name) = instance_name {
                            if let Some((ip_address, port)) = server_config.get_ip_and_port(&mdns, &instance_name) {
                                let uri = format!(
                                    "http://{}:{}/v1/apps/{}/data/{}",
                                    ip_address, port, entity_id, data_id
                                );
                                // drop(mdns);
                                let mut headers = HeaderMap::new();
                                headers.insert("Accept", HeaderValue::from_static("application/json"));

                                match gateway_request(uri, hyper::Method::GET, headers, None).await {
                                    Ok(response) => {
                                        let response_body = response.into_body();
                                        let od_body_bytes = match hyper::body::to_bytes(response_body).await {
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
                        
                                        let od_body_str = match String::from_utf8(od_body_bytes.to_vec()) {
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
                        
                                        let json_value: JsonValue = match serde_json::from_str(&od_body_str) {
                                            Ok(value) => value,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError".to_string(),
                                                    message: format!("Failed to parse response body:: {}", err),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };
                        
                                        if let serde_json::Value::Object(map) = json_value {
                                            if let Some(data_value) = map.get("data") {
                                                let mut data: Map<String, Value> = Map::new();
                                                data.insert("data".to_string(), data_value.clone());
                                                let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                                                    id: map["id"].as_str().unwrap_or_default().to_string(),
                                                    data: to_value(data).expect("Failed to filter writables"),
                                                    errors: None,
                                                    schema: None,
                                                };
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::TheRequestWasSuccessful(read_value));
                                            }
                                        }
                    
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "ResourceNotAvailable".to_string(),
                                            message: format!("Resource not available."),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                    Err(_) => {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayRequestFailed".to_string(),
                                            message: format!("Failed to fetch data from gateway."),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                }
                            } else {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "IPAndPortResolutionFailed".to_string(),
                                    message: format!("Failed to resolve IP and port for the given instance."),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None
                                };
                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                            }
                        } else {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "InstanceNameNotFound".to_string(),
                                message: format!("No standalone instance name found."),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None
                            };
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                        }
                        
                    } else {
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "ProcessNotFound".to_string(),
                            message: format!("The process was not found."),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None
                        };
                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                    }
                },
                _ => {
                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "EntityCollectionNotFound".to_string(),
                        message: format!("The entity collection was not found."),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None
                    };
                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::AnUnexpectedRequestOccurred(error))
                }
            }
        } else {
            info!("Server configuration not initialized!");
            let error = AnyPathDocsGetDefaultResponse {
                error_code: "ServerConfigurationNotInitialized".to_string(),
                message: format!("Server configuration not initialized."),
                vendor_code: None,
                translation_id: None,
                parameters: None
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
        context: &C) -> Result<EntityCollectionEntityIdDataDataIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_data_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, data_id, entity_collection_entity_id_data_data_id_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_data_list_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_list_id: String,
        context: &C) -> Result<EntityCollectionEntityIdDataListsDataListIdDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_lists_data_list_id_delete({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, data_list_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_data_lists_data_list_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        data_list_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdDataListsDataListIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_data_lists_data_list_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, data_list_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn areas_area_id_related_components_get(
        &self,
        area_id: String,
        context: &C) -> Result<AreasAreaIdRelatedComponentsGetResponse, ApiError>
    {
        info!("areas_area_id_related_components_get(\"{}\") - X-Span-ID: {:?}", area_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn areas_area_id_subareas_get(
        &self,
        area_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<AreasAreaIdSubareasGetResponse, ApiError>
    {
        info!("areas_area_id_subareas_get(\"{}\", {:?}) - X-Span-ID: {:?}", area_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn components_component_id_related_apps_get(
        &self,
        component_id: String,
        context: &C) -> Result<ComponentsComponentIdRelatedAppsGetResponse, ApiError>
    {
        info!(
            "components_component_id_related_apps_get(\"{}\") - X-Span-ID: {:?}",
            component_id,
            context.get().0.clone()
        );
    
        if let Some(server_config) = SERVER_CONFIG.get() {
            // Check if the SOVD mode is "gateway"
            if server_config.get_sovd_mode() == "gateway" {
                let mdns = ServiceDaemonWrapper::new(ServiceDaemon::new().expect("Failed to create daemon"));
                let instance_name = server_config.get_instance_name_for_standalone();
            
                if let Some(instance_name) = instance_name {
                    if let Some((ip_address, port)) = server_config.get_ip_and_port(&mdns, &instance_name) {
                        // drop(mdns);
                        // Check if the host is available
                        if is_host_available(&ip_address, port).await {
                            if component_id == "telematics" {
                                let mut response_items = Vec::new();
                                let empty_vec = Vec::new();
            
                                // Only for the current component
                                if server_config.host_name == component_id {
                                    let sovd_apps_list = server_config.get_apps_by_component_id(component_id.as_str()).unwrap_or(&empty_vec);
            
                                    // Extract search terms from the sovd_apps_list
                                    let search_terms: Vec<&str> = sovd_apps_list.iter().map(AsRef::as_ref).collect();
            
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
                            } else {
                                let uri_get_related_apps = format!(
                                    "http://{}:{}/v1/components/{}/related-apps",
                                    ip_address, port, component_id
                                );
            
                                // drop(mdns);
                                let mut headers = HeaderMap::new();
                                headers.insert("Accept", HeaderValue::from_static("application/json"));

                                match gateway_request(uri_get_related_apps, hyper::Method::GET, headers, None).await {
                                    // Process successful response
                                    Ok(response) => {
                                        let response_body = response.into_body();
                                        let body_bytes: Bytes = match hyper::body::to_bytes(response_body).await {
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
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };
            
                                        let body_str = match String::from_utf8(body_bytes.to_vec()) {
                                            Ok(str) => str,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyConversionError".to_string(),
                                                    message: format!("Failed to convert response body to string: {}", err),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };
            
                                        let json_value: JsonValue = match serde_json::from_str(&body_str) {
                                            Ok(value) => value,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError".to_string(),
                                                    message: format!("Failed to parse response body: {}", err),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        };
            
                                        let response_items: Vec<EntityCollectionGet200ResponseItemsInner> = match json_value.get("items") {
                                            Some(items) => match serde_json::from_value(items.clone()) {
                                                Ok(items) => items,
                                                Err(err) => {
                                                    let error = AnyPathDocsGetDefaultResponse {
                                                        error_code: "GatewayResponseBodyParsingError".to_string(),
                                                        message: format!("Failed to parse 'items' array: {}", err),
                                                        vendor_code: None,
                                                        translation_id: None,
                                                        parameters: None
                                                    };
                                                    return Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error));
                                                }
                                            },
                                            None => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayResponseBodyParsingError".to_string(),
                                                    message: format!("Response body does not contain 'items' arra"),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
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
                                            let href = format!("{}/apps/{}", server_config.base_uri, id);
                                            extracted_items.push(EntityCollectionGet200ResponseItemsInner::new(id, name, href));
                                        }
            
                                        let response_body = ComponentsComponentIdRelatedAppsGetResponse::ResponseBody(
                                            AreasAreaIdRelatedComponentsGet200Response::new(extracted_items),
                                        );
            
                                        Ok(response_body)
                                    }
                                    Err(_) => {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayRequestFailed".to_string(),
                                            message: format!("Failed to fetch data from gateway."),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None
                                        };
                                        Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                                    }
                                }
                            }
                        } else if component_id == "chassis-hpc" {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "GatewayRequestGatewayDown".to_string(),
                                message: format!("Failed to connect"),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None
                            };
                            Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                        } else {
                            // Implementation for other cases (if host is not available and not chassis-hpc)
                            let mut response_items = Vec::new();
                            let empty_vec = Vec::new();
            
                            // Only for the current component
                            if server_config.host_name == component_id {
                                let sovd_apps_list = server_config.get_apps_by_component_id(component_id.as_str()).unwrap_or(&empty_vec);
            
                                // Extract search terms from the sovd_apps_list
                                let search_terms: Vec<&str> = sovd_apps_list.iter().map(AsRef::as_ref).collect();
            
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

                        if component_id == "telematics" {
                            let mut response_items = Vec::new();
                            let empty_vec = Vec::new();
        
                            // Only for the current component
                            if server_config.host_name == component_id {
                                let sovd_apps_list = server_config.get_apps_by_component_id(component_id.as_str()).unwrap_or(&empty_vec);
        
                                // Extract search terms from the sovd_apps_list
                                let search_terms: Vec<&str> = sovd_apps_list.iter().map(AsRef::as_ref).collect();
        
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
                        } else {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "InstanceResolutionFailed".to_string(),
                                message: format!("Failed to resolve IP and port for the given instance."),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None
                            };
                            Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                        }
                    }
                } else {
                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "InstanceNameNotFound".to_string(),
                        message: format!("No standalone instance name found."),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None
                    };
                    Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
                }
            } else {
                // Implementation for other cases (if SOVD mode is not "gateway")
                // Load the app data
                let mut response_items = Vec::new();
                let empty_vec = Vec::new();
            info!("component_id {} host {}", component_id, server_config.host_name);
                // Only for the current component
                if server_config.host_name == component_id {
                    let sovd_apps_list = server_config.get_apps_by_component_id(component_id.as_str()).unwrap_or(&empty_vec);
            
                    // Extract search terms from the sovd_apps_list
                    let search_terms: Vec<&str> = sovd_apps_list.iter().map(AsRef::as_ref).collect();
            
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
                message: format!("Server configuration not initialized."),
                vendor_code: None,
                translation_id: None,
                parameters: None
            };
            Ok(ComponentsComponentIdRelatedAppsGetResponse::AnUnexpectedRequestOccurred(error))
        }
    }

    async fn components_component_id_subcomponents_get(
        &self,
        component_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<ComponentsComponentIdSubcomponentsGetResponse, ApiError>
    {
        info!("components_component_id_subcomponents_get(\"{}\", {:?}) - X-Span-ID: {:?}", component_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionGetResponse, ApiError>
    {
        info!(
            "entity_collection_get(\"{}\", {:?}) - X-Span-ID: {:?}",
            entity_collection,
            include_schema,
            context.get().0.clone()
        );
    
        // Directly extract from the server_config structure
        if let Some(server_config) = SERVER_CONFIG.get() {
            if entity_collection == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components {
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
                    let mdns = ServiceDaemonWrapper::new(ServiceDaemon::new().expect("Failed to create daemon"));
                    let instance_name = server_config.get_instance_name_for_standalone();
    
                    if let Some(instance_name) = instance_name {

                        if let Some((_ip_address, _port)) = server_config.get_ip_and_port(&mdns, &instance_name) {

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
                            parameters: None
                        };
                        return Ok(EntityCollectionGetResponse::AnUnexpectedRequestOccurred(error));
                    }
                }
    
                // Create InlineResponse200 with the EntityReferences and optionally the schema
                let mut response_body = models::EntityCollectionGet200Response::new(entity_references);
                if let Some(include_schema) = include_schema {
                    if include_schema {
                        // Set the schema if required
                        response_body.schema = Some(false);
                    }
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
            parameters: None
        };
        
        Ok(EntityCollectionGetResponse::AnUnexpectedRequestOccurred(error))
    }

    async fn entity_collection_entity_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdGetResponse, ApiError>
    {
         info!(
            "entity_collection_entity_id_get(\"{}\", \"{}\") - X-Span-ID: {:?}",
            entity_collection,
            entity_id,
            context.get().0.clone()
        );
    
        if let Some(server_config) = get_server_config() {
            info!("Server configuration initialized!");
    
            if entity_collection == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Apps {
                let resource = get_before_last_dash(&String::from(entity_id.clone()));
                let pid = get_last_part_after_dash(&String::from(entity_id.clone()));
                let app_id = String::from(entity_id.clone());
                let mut _comp_id = "telematics";
                
                match server_config.get_component_by_app(&app_id) {
                    Some(component_id) =>{
                        info!("Component ID: {}", component_id);
                        _comp_id = component_id;
                    } 
                    None => {
                        info!("No component found for app_id: {}", &app_id);
                    } 
                }

                if let Some(app) = find_single_process(&resource, &pid, &server_config.base_uri) {

                    let mut response = EntityCollectionEntityIdGet200Response::new(entity_id.clone(), app.name.clone());
                    let app_data = format!("{}/{}/{}/data", server_config.base_uri, entity_collection.clone(), app.id.clone());


                    response.data = Some(app_data);
                    
                    return Ok(EntityCollectionEntityIdGetResponse::TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
            
            
                } else {

                    //Check if gateway mode is active, because perhaps the app is on another device
                    if let Some(server_config) = SERVER_CONFIG.get() {
	
                        if server_config.get_sovd_mode() == "gateway" {
                            let mdns = ServiceDaemonWrapper::new(ServiceDaemon::new().expect("Failed to create daemon"));
                            let instance_name = server_config.get_instance_name_for_standalone();
                            
                            if let Some(instance_name) = instance_name {
                                if let Some((ip_address, port)) = server_config.get_ip_and_port(&mdns, &instance_name) {
                                    // drop(mdns);
                                    if is_host_available(&ip_address, port).await {
                                        // Host is available
                                        let uri = format!(
                                            "http://{}:{}/v1/apps/{}",
                                            ip_address, port, entity_id
                                        );
                                        
                                         // drop(mdns);
                                         let mut headers = HeaderMap::new();
                                         headers.insert("Accept", HeaderValue::from_static("application/json"));
 
                                         match gateway_request(uri, hyper::Method::GET, headers, None).await{
                                            // Process successful response
                                            Ok(response) => {
                                                let response_body = response.into_body();
                                                let od_body_bytes: Bytes = match hyper::body::to_bytes(response_body).await {
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
                        
                                                let od_body_str = match String::from_utf8(od_body_bytes.to_vec()) {
                                                    Ok(str) => str,
                                                    Err(err) => {
                                                        let error = AnyPathDocsGetDefaultResponse {
                                                            error_code: "GatewayResponseBodyConversionError".to_string(),
                                                            message: format!("Failed to convert response body to string: {}", err),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None
                                                        };
                                                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                                    }
                                                };
                        
                                                let mut json_value: JsonValue = match serde_json::from_str(&od_body_str) {
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
                                                let extracted_data = extract_response_data_from_json_to_response(&mut json_value, &server_config.get_base_uri());
                        
                                                return Ok(extracted_data);
                                            }
                                            Err(_) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayRequestFailed".to_string(),
                                                    message: format!("Failed to fetch data from gateway."),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None
                                                };
                                                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                            }
                                        }
                                    } else {
                                        // Gateway down
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "GatewayDown".to_string(),
                                            message: format!("Failed to connect to gateway."),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None
                                        };
                                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                    }
                                } else {
                                    let error = AnyPathDocsGetDefaultResponse {
                                        error_code: "IPAndPortResolutionFailed".to_string(),
                                        message: format!("Failed to resolve IP and port for the given instance."),
                                        vendor_code: None,
                                        translation_id: None,
                                        parameters: None
                                    };
                                    return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                }
                            } else {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "InstanceNameNotFound".to_string(),
                                    message: format!("No standalone instance name found."),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None
                                };
                                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                            }
                        }
                         else {
                            // Implementation for other cases (if SOVD mode is not "gateway")
                        }
                        
                    } else {
                        // Error handling for uninitialized server configuration
                        info!("Server configuration not initialized!");
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "ServerConfigurationNotInitialized".to_string(),
                            message: format!("Server configuration not initialized."),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None
                        };
                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                                
                    }

                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "EntityNotFound".to_string(),
                        message: format!("Entity '{}' not found.", entity_id),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None
                    };
                    return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                }
            } else if entity_collection == EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter::Components {
    
                // Declaration of id and name as Option
                let mut id: Option<String> = None;
                let mut name: Option<String> = None;
    
                // Call entity_collection_get and process the response
                match self.entity_collection_get(entity_collection.clone(), None, context).await {
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
    

                            let mut response = EntityCollectionEntityIdGet200Response::new(id.clone(), name.clone());
                            let app_data = format!("{}/{}/{}/data", server_config.base_uri, entity_collection.clone(), id.clone());
                            response.data = Some(app_data);
    
                            return Ok(EntityCollectionEntityIdGetResponse::TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                        } else {
                            // If no matching entity was found
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "EntityNotFound".to_string(),
                                message:  format!("Entity '{}' not found.", entity_id),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None
                            };
                            return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                        }
                    }
                    Err(err) => {
                        // Error while querying entity_collection_get
                        return Err(err.into());
                    }
                    _ => {
                        // Unexpected response from entity_collection_get
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "UnexpectedResponse".to_string(),
                            message: format!("Unexpected response from entity_collection_get."),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None
                        };
                        return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
                    }
                }
            } else {
                let error = AnyPathDocsGetDefaultResponse {
                    error_code: "UnexpectedRequest".to_string(),
                    message: format!("An unexpected request occurred."),
                    vendor_code: None,
                    translation_id: None,
                    parameters: None
                };
                return Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error));
            }
        } else {
            info!("Server configuration not initialized!");
        }
    
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnexpectedRequest".to_string(),
            message: format!("An unexpected request occurred."),
            vendor_code: None,
            translation_id: None,
            parameters: None
        };
        Ok(EntityCollectionEntityIdGetResponse::AnUnexpectedRequestOccurred(error))
    }

    async fn delete_all_faults(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        scope: Option<String>,
        context: &C) -> Result<DeleteAllFaultsResponse, ApiError>
    {
        info!("delete_all_faults({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, scope, context.get().0.clone());
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
        context: &C) -> Result<GetFaultsResponse, ApiError>
    {
        info!("get_faults({:?}, \"{}\", {:?}, {:?}, {:?}, {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, include_schema, status_left_square_bracket_key_right_square_bracket, severity, scope, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn delete_fault_by_id(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        fault_code: String,
        context: &C) -> Result<DeleteFaultByIdResponse, ApiError>
    {
        info!("delete_fault_by_id({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, fault_code, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn get_fault_by_id(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        fault_code: String,
        include_schema: Option<bool>,
        context: &C) -> Result<GetFaultByIdResponse, ApiError>
    {
        info!("get_fault_by_id({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, fault_code, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdLocksGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_locks_get({:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_locks_post_request: models::EntityCollectionEntityIdLocksPostRequest,
        context: &C) -> Result<EntityCollectionEntityIdLocksPostResponse, ApiError>
    {
        info!("entity_collection_entity_id_locks_post({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, entity_collection_entity_id_locks_post_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        context: &C) -> Result<EntityCollectionEntityIdLocksLockIdDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_locks_lock_id_delete({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, lock_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        context: &C) -> Result<EntityCollectionEntityIdLocksLockIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_locks_lock_id_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, lock_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_locks_lock_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        lock_id: String,
        entity_collection_entity_id_locks_post_request: models::EntityCollectionEntityIdLocksPostRequest,
        context: &C) -> Result<EntityCollectionEntityIdLocksLockIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_locks_lock_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, lock_id, entity_collection_entity_id_locks_post_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdLogsConfigDeleteResponse, ApiError>
    {
        info!("entity_collection_entity_id_logs_config_delete({:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        context: &C) -> Result<EntityCollectionEntityIdLogsConfigGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_logs_config_get({:?}, \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_config_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        entity_collection_entity_id_logs_config_put_request: models::EntityCollectionEntityIdLogsConfigPutRequest,
        context: &C) -> Result<EntityCollectionEntityIdLogsConfigPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_logs_config_put({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, entity_collection_entity_id_logs_config_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_logs_entries_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        severity: Option<models::EntityCollectionEntityIdLogsEntriesGetSeverityParameter>,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdLogsEntriesGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_logs_entries_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, severity, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_proximity_proof: Option<bool>,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdOperationsGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_get({:?}, \"{}\", {:?}, {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, include_proximity_proof, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_operation_id_executions_get({:?}, \"{}\", \"{}\") - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_post(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        entity_collection_entity_id_operations_operation_id_executions_post_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsPostRequest,
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_operation_id_executions_post({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, entity_collection_entity_id_operations_operation_id_executions_post_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_operation_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_delete(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        execution_id: String,
        entity_collection_entity_id_operations_operation_id_executions_execution_id_delete_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteRequest,
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse, ApiError>
    {
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
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_operation_id_executions_execution_id_get({:?}, \"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, execution_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        operation_id: String,
        execution_id: String,
        entity_collection_entity_id_operations_operation_id_executions_execution_id_put_request: models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutRequest,
        context: &C) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_operations_operation_id_executions_execution_id_put({:?}, \"{}\", \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, operation_id, execution_id, entity_collection_entity_id_operations_operation_id_executions_execution_id_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdModesGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_modes_get({:?}, \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_mode_id_get(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        mode_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<EntityCollectionEntityIdModesModeIdGetResponse, ApiError>
    {
        info!("entity_collection_entity_id_modes_mode_id_get({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, mode_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn entity_collection_entity_id_modes_mode_id_put(
        &self,
        entity_collection: models::EntityCollectionEntityIdDataCategoriesGetEntityCollectionParameter,
        entity_id: String,
        mode_id: String,
        entity_collection_entity_id_modes_mode_id_put_request: models::EntityCollectionEntityIdModesModeIdPutRequest,
        context: &C) -> Result<EntityCollectionEntityIdModesModeIdPutResponse, ApiError>
    {
        info!("entity_collection_entity_id_modes_mode_id_put({:?}, \"{}\", \"{}\", {:?}) - X-Span-ID: {:?}", entity_collection, entity_id, mode_id, entity_collection_entity_id_modes_mode_id_put_request, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_get(
        &self,
        target_version: Option<String>,
        origin: Option<models::UpdatesGetOriginParameter>,
        context: &C) -> Result<UpdatesGetResponse, ApiError>
    {
        info!("updates_get({:?}, {:?}) - X-Span-ID: {:?}", target_version, origin, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_post(
        &self,
        content_type: Option<String>,
        body: Option<serde_json::Value>,
        context: &C) -> Result<UpdatesPostResponse, ApiError>
    {
        info!("updates_post({:?}, {:?}) - X-Span-ID: {:?}", content_type, body, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_automated_put(
        &self,
        update_package_id: String,
        context: &C) -> Result<UpdatesUpdatePackageIdAutomatedPutResponse, ApiError>
    {
        info!("updates_update_package_id_automated_put(\"{}\") - X-Span-ID: {:?}", update_package_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_delete(
        &self,
        update_package_id: String,
        context: &C) -> Result<UpdatesUpdatePackageIdDeleteResponse, ApiError>
    {
        info!("updates_update_package_id_delete(\"{}\") - X-Span-ID: {:?}", update_package_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_execute_put(
        &self,
        update_package_id: String,
        context: &C) -> Result<UpdatesUpdatePackageIdExecutePutResponse, ApiError>
    {
        info!("updates_update_package_id_execute_put(\"{}\") - X-Span-ID: {:?}", update_package_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_get(
        &self,
        update_package_id: String,
        include_schema: Option<bool>,
        context: &C) -> Result<UpdatesUpdatePackageIdGetResponse, ApiError>
    {
        info!("updates_update_package_id_get(\"{}\", {:?}) - X-Span-ID: {:?}", update_package_id, include_schema, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_prepare_put(
        &self,
        update_package_id: String,
        context: &C) -> Result<UpdatesUpdatePackageIdPreparePutResponse, ApiError>
    {
        info!("updates_update_package_id_prepare_put(\"{}\") - X-Span-ID: {:?}", update_package_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

    async fn updates_update_package_id_status_get(
        &self,
        update_package_id: String,
        context: &C) -> Result<UpdatesUpdatePackageIdStatusGetResponse, ApiError>
    {
        info!("updates_update_package_id_status_get(\"{}\") - X-Span-ID: {:?}", update_package_id, context.get().0.clone());
        Err(ApiError("Api-Error: Operation is NOT implemented".into()))
    }

}
