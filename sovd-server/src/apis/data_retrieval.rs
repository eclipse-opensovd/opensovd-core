use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
use openapi::{
    apis::data_retrieval::{
        EntityCollectionEntityIdDataCategoriesGetResponse,
        EntityCollectionEntityIdDataDataIdGetResponse,
        EntityCollectionEntityIdDataDataIdPutResponse, EntityCollectionEntityIdDataGetResponse,
        EntityCollectionEntityIdDataGroupsGetResponse,
        EntityCollectionEntityIdDataListsDataListIdDeleteResponse,
        EntityCollectionEntityIdDataListsDataListIdGetResponse,
        EntityCollectionEntityIdDataListsGetResponse,
        EntityCollectionEntityIdDataListsPostResponse,
    },
    models, types::Object,
};
use openapi::models::*;
use sovd_handlers::{IDENT_DATA_RESPONSE, get_first_part_after_dash, handle_app_resource};
use sovd_handlers::group_by_writability;
use crate::sovd_server::get_server_config;
use sovd_handlers::get_last_part_after_dash;
use sovd_handlers::handle_system_resource;
use hyper::HeaderMap;
use hyper::header::HeaderValue;
use std::sync::{Arc, Mutex};
use mdns_sd::ServiceDaemon;
use sovd_handlers::gateway_request;
use sovd_handlers::find_single_process;
use serde_json::Value;
use serde_json::Map;
use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::data_retrieval::DataRetrieval<()> for ServerImpl {
    /// EntityCollectionEntityIdDataCategoriesGet - GET /v1/{entity_collection}/{entity_id}/data-categories
    async fn entity_collection_entity_id_data_categories_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataCategoriesGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataCategoriesGetResponse, ()> {
        info!("entity_collection_entity_id_data_categories_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        let response = EntityCollectionEntityIdDataCategoriesGet200Response {
            items: vec!["sysInfo".to_string()],
        };
        Ok(EntityCollectionEntityIdDataCategoriesGetResponse::Status200_TheRequestWasSuccessful(response))
    }

    /// EntityCollectionEntityIdDataDataIdGet - GET /v1/{entity_collection}/{entity_id}/data/{data_id}
    async fn entity_collection_entity_id_data_data_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataDataIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataDataIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataDataIdGetResponse, ()> {
        info!("entity_collection_entity_id_data_data_id_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);

        if let Some(server_config) = get_server_config() {
            match path_params.entity_collection.to_lowercase().as_str() {
                "components" => {
                    let component_name = &path_params.entity_id;

                    match component_name.as_str() {
                        "telematics" => {
                            let resource = get_last_part_after_dash(&path_params.data_id);
                            let response = handle_system_resource(
                                resource.as_str(),
                                component_name,
                                path_params.data_id.as_str(),
                            );
                            Ok(response)
                        }

                        "chassis-hpc" => {
                            match server_config.get_sovd_mode() {
                                "gateway" => {
                                    let mdns = Arc::new(Mutex::new(ServiceDaemon::new().expect("Failed to create mDNS daemon")));
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
                                                uri_get_components, component_name, path_params.data_id
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
                                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                                        }
                                                    };

                                                    let json_value: serde_json::Value =
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
                                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                                            }
                                                        };

                                                    if let serde_json::Value::Object(map) =
                                                        json_value
                                                        && let Some(data_value) = map.get("data")
                                                    {
                                                        let mut data_map: Map<String, serde_json::Value> =
                                                            Map::new();
                                                        data_map.insert(
                                                            "data".to_string(),
                                                            data_value.clone(),
                                                        );

                                                       
                                                        let read_value = EntityCollectionEntityIdDataDataIdGet200Response {
                                                            id: map["id"].as_str().unwrap_or_default().to_string(),
                                                            data: Object::new(serde_json::Value::Object(data_map)),
                                                            r_errors: None,
                                                            schema: None,
                                                        };
                                                        return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status200_TheRequestWasSuccessful(read_value));
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
                                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                                        }
                                    } else {
                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "StandaloneInstanceNotFound".to_string(),
                                            message: "Standalone instance not found.".to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                                    }
                                }
                                "standalone" => {
                                    let resource = get_last_part_after_dash(&path_params.data_id);
                                    let response = handle_system_resource(
                                        resource.as_str(),
                                        component_name,
                                        path_params.data_id.as_str(),
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
                                    Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                        }
                    }
                }
                "apps" => {
                    info!(
                        "Apps case: collection-ID {} entity-ID {} data-ID {}",
                        path_params.entity_collection, path_params.entity_id, path_params.data_id
                    );
                    // let resource_to_check = get_before_last_dash(&entity_id);
                    // let pid = get_last_part_after_dash(&entity_id);

                    let tokens = path_params.entity_id.split('-');

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
                        let resource = get_last_part_after_dash(&path_params.data_id);
                        let tokens = app.id.split('-');
                        let pid_to_monitor = tokens.clone().next_back().unwrap();
                        // let pid_to_monitor = get_last_part_after_dash(&entity_id);
                        let app_name = get_first_part_after_dash(&path_params.entity_id);
                        let response = handle_app_resource(
                            resource.as_str(),
                            pid_to_monitor,
                            app_name.as_str(),
                            path_params.data_id.as_str(),
                        );

                        Ok(response)
                    } else if server_config.get_sovd_mode() == "gateway" {
                        let mdns = Arc::new(Mutex::new(ServiceDaemon::new().expect("Failed to create mDNS daemon")));
                        let instance_name = server_config.get_instance_name_for_standalone();

                        if let Some(instance_name) = instance_name {
                            if let Some((ip_address, port)) =
                                server_config.get_ip_and_port(&mdns, &instance_name)
                            {
                                let uri = format!(
                                    "http://{}:{}/v1/apps/{}/data/{}",
                                    ip_address, port, path_params.entity_id, path_params.data_id
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
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let json_value: Value = match serde_json::from_str(
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
                                                return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                    data: Object(serde_json::Value::Object(data)),
                                                    r_errors: None,
                                                    schema: None,
                                                };
                                            return Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status200_TheRequestWasSuccessful(read_value));
                                        }

                                        let error = AnyPathDocsGetDefaultResponse {
                                            error_code: "ResourceNotAvailable".to_string(),
                                            message: "Resource not available.".to_string(),
                                            vendor_code: None,
                                            translation_id: None,
                                            parameters: None,
                                        };
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                                Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                            }
                        } else {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "InstanceNameNotFound".to_string(),
                                message: "No standalone instance name found.".to_string(),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                        }
                    } else {
                        let error = AnyPathDocsGetDefaultResponse {
                            error_code: "ProcessNotFound".to_string(),
                            message: "The process was not found.".to_string(),
                            vendor_code: None,
                            translation_id: None,
                            parameters: None,
                        };
                        Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                        EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(
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
            Ok(EntityCollectionEntityIdDataDataIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
        }
    }

    /// EntityCollectionEntityIdDataDataIdPut - PUT /v1/{entity_collection}/{entity_id}/data/{data_id}
    async fn entity_collection_entity_id_data_data_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataDataIdPutPathParams,
        body: &models::EntityCollectionEntityIdDataDataIdPutRequest,
    ) -> Result<EntityCollectionEntityIdDataDataIdPutResponse, ()> {
        info!("entity_collection_entity_id_data_data_id_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, body);
        Err(())
    }

    /// EntityCollectionEntityIdDataGet - GET /v1/{entity_collection}/{entity_id}/data
    async fn entity_collection_entity_id_data_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataGetResponse, ()> {
        info!(
            "entity_collection_entity_id_data_get({} {:?} {:?} {:?} {:?})",
            method,
            host,
            cookies,
            path_params,
            query_params,
        );
        let resource_names = ["CPU", "Disk", "Memory", "All"];
        let last_dash_index = path_params.entity_id.rfind('-').unwrap_or(0);
        let entity_id_cleaned = path_params.entity_id[..last_dash_index].to_string();
        let mut items = Vec::new();

        match path_params.entity_collection.to_lowercase().as_str() {
            "apps" => {
                for resource_name in &resource_names {
                    let id = resource_name.to_lowercase().to_string();
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name, path_params.entity_collection, entity_id_cleaned
                    );
                    let value_metadata = EntityCollectionEntityIdDataGet200ResponseItemsInner::new(
                        id,
                        name,
                        "sysInfo".to_string(),
                    );
                    items.push(value_metadata);
                }
            }

            "components" => {
                for resource_name in &resource_names {
                    let id = format!("{}-{}", path_params.entity_id, resource_name.to_lowercase());
                    let name = format!(
                        "Current {} usage for {} {}",
                        resource_name, path_params.entity_collection, entity_id_cleaned
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
                    EntityCollectionEntityIdDataGetResponse::Status0_AnUnexpectedRequestOccurred(error),
                );
            }
        }

        let response = EntityCollectionEntityIdDataGet200Response::new(items);
        Ok(EntityCollectionEntityIdDataGetResponse::Status200_TheRequestWasSuccessful(response))
    }

    /// EntityCollectionEntityIdDataGroupsGet - GET /v1/{entity_collection}/{entity_id}/data-groups
    async fn entity_collection_entity_id_data_groups_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataGroupsGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataGroupsGetResponse, ()> {
        info!(
            "entity_collection_entity_id_data_groups_get({} {:?} {:?} {:?})",
            method,
            host,
            cookies,
            path_params
        );

        // Lock mutex and retrieve data
        let response_mutex = IDENT_DATA_RESPONSE.lock().unwrap();
        let response_vec = response_mutex.clone(); // Here we copy the mutex content into a new Vec<ValueGroup>

        // Call process_json_data synchronously
        match group_by_writability(&response_vec) {
            Ok(result) => Ok(result),
            Err(_error) => Err(()),
        }
    }

    /// EntityCollectionEntityIdDataListsDataListIdDelete - DELETE /v1/{entity_collection}/{entity_id}/data-lists/{data_list_id}
    async fn entity_collection_entity_id_data_lists_data_list_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsDataListIdDeletePathParams,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdDeleteResponse, ()> {
        info!("entity_collection_entity_id_data_lists_data_list_id_delete({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdDataListsDataListIdGet - GET /v1/{entity_collection}/{entity_id}/data-lists/{data_list_id}
    async fn entity_collection_entity_id_data_lists_data_list_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsDataListIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataListsDataListIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdGetResponse, ()> {
        info!("entity_collection_entity_id_data_lists_data_list_id_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// EntityCollectionEntityIdDataListsGet - GET /v1/{entity_collection}/{entity_id}/data-lists
    async fn entity_collection_entity_id_data_lists_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataListsGetResponse, ()> {
        info!("entity_collection_entity_id_data_lists_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdDataListsPost - POST /v1/{entity_collection}/{entity_id}/data-lists
    async fn entity_collection_entity_id_data_lists_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsPostPathParams,
        body: &models::EntityCollectionEntityIdDataListsPostRequest,
    ) -> Result<EntityCollectionEntityIdDataListsPostResponse, ()> {
        info!("entity_collection_entity_id_data_lists_post({} {:?} {:?} {:?} {:?})",method, host, cookies, path_params, body);
        Err(())
    }
}