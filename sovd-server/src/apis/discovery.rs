use async_trait::async_trait;
use axum::{body::Bytes, http::Method};
use axum_extra::extract::{CookieJar, Host};
use hyper::{HeaderMap, header::HeaderValue};
use log::info;
use openapi::{
    apis::discovery::{
        AreasAreaIdRelatedComponentsGetResponse, AreasAreaIdSubareasGetResponse,
        ComponentsComponentIdRelatedAppsGetResponse, ComponentsComponentIdSubcomponentsGetResponse,
        EntityCollectionEntityIdGetResponse, EntityCollectionGetResponse,
    },
    models::{self, AnyPathDocsGetDefaultResponse, EntityCollectionEntityIdGet200Response},
};
use serde_json::Value;
use sovd_handlers::{extract_response_data_from_json_to_response, find_processes, find_single_process, gateway_request, is_host_available};
use crate::sovd_server::get_server_config;
use openapi::models::*;
use std::sync::{Arc, Mutex};
use mdns_sd::ServiceDaemon;
use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::discovery::Discovery<()> for ServerImpl {
    /// AreasAreaIdRelatedComponentsGet - GET /v1/areas/{area_id}/related-components
    async fn areas_area_id_related_components_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::AreasAreaIdRelatedComponentsGetPathParams,
    ) -> Result<AreasAreaIdRelatedComponentsGetResponse, ()> {
        info!("areas_area_id_related_components_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// AreasAreaIdSubareasGet - GET /v1/areas/{area_id}/subareas
    async fn areas_area_id_subareas_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::AreasAreaIdSubareasGetPathParams,
        query_params: &models::AreasAreaIdSubareasGetQueryParams,
    ) -> Result<AreasAreaIdSubareasGetResponse, ()> {
        info!("areas_area_id_subareas_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// ComponentsComponentIdRelatedAppsGet - GET /v1/components/{component_id}/related-apps
    async fn components_component_id_related_apps_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::ComponentsComponentIdRelatedAppsGetPathParams,
    ) -> Result<ComponentsComponentIdRelatedAppsGetResponse, ()> {
        info!("components_component_id_related_apps_get({} {:?} {:?} {:?})", method, host, cookies, path_params);

         if let Some(server_config) = get_server_config() {
            // Check if the SOVD mode is "gateway"
            if server_config.get_sovd_mode() == "gateway" {
                let mdns = Arc::new(Mutex::new(ServiceDaemon::new().expect("Failed to create mDNS daemon")));
                let instance_name = server_config.get_instance_name_for_standalone();

                if let Some(instance_name) = instance_name {
                    if let Some((ip_address, port)) =
                        server_config.get_ip_and_port(&mdns, &instance_name)
                    {
                        // drop(mdns);
                        // Check if the host is available
                        if is_host_available(&ip_address, port).await {
                            if path_params.component_id == "telematics" {
                                let mut response_items = Vec::new();
                                let empty_vec = Vec::new();

                                // Only for the current component
                                if server_config.host_name == path_params.component_id {
                                    let sovd_apps_list = server_config
                                        .get_apps_by_component_id(path_params.component_id.as_str())
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
                                    ComponentsComponentIdRelatedAppsGetResponse::Status200_ResponseBody(
                                        AreasAreaIdRelatedComponentsGet200Response::new(
                                            response_items,
                                        ),
                                    );

                                Ok(response_body)
                            } else {
                                let uri_get_related_apps = format!(
                                    "http://{}:{}/v1/components/{}/related-apps",
                                    ip_address, port, path_params.component_id
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
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let body_str = match String::from_utf8(body_bytes.to_vec())
                                        {
                                            Ok(str) => str,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code:
                                                        "GatewayStatus200_ResponseBodyConversionError"
                                                            .to_string(),
                                                    message: format!(
                                                        "Failed to convert response body to string: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                            }
                                        };

                                        let json_value: Value = match serde_json::from_str(
                                            &body_str,
                                        ) {
                                            Ok(value) => value,
                                            Err(err) => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayStatus200_ResponseBodyParsingError"
                                                        .to_string(),
                                                    message: format!(
                                                        "Failed to parse response body: {}",
                                                        err
                                                    ),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                                "GatewayStatus200_ResponseBodyParsingError"
                                                                    .to_string(),
                                                            message: format!(
                                                                "Failed to parse 'items' array: {}",
                                                                err
                                                            ),
                                                            vendor_code: None,
                                                            translation_id: None,
                                                            parameters: None,
                                                        };
                                                        return Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                                    }
                                                }
                                            }
                                            None => {
                                                let error = AnyPathDocsGetDefaultResponse {
                                                    error_code: "GatewayStatus200_ResponseBodyParsingError"
                                                        .to_string(),
                                                    message:
                                                        "Response body does not contain 'items' arra"
                                                    .to_string(),
                                                    vendor_code: None,
                                                    translation_id: None,
                                                    parameters: None,
                                                };
                                                return Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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

                                        let response_body = ComponentsComponentIdRelatedAppsGetResponse::Status200_ResponseBody(
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
                                        Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                                    }
                                }
                            }
                        } else if path_params.component_id == "chassis-hpc" {
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "GatewayRequestGatewayDown".to_string(),
                                message: "Failed to connect".to_string(),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error))
                        } else {
                            // Implementation for other cases (if host is not available and not chassis-hpc)
                            let mut response_items = Vec::new();
                            let empty_vec = Vec::new();

                            // Only for the current component
                            if server_config.host_name == path_params.component_id {
                                let sovd_apps_list = server_config
                                    .get_apps_by_component_id(path_params.component_id.as_str())
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
                                ComponentsComponentIdRelatedAppsGetResponse::Status200_ResponseBody(
                                    AreasAreaIdRelatedComponentsGet200Response::new(response_items),
                                );

                            Ok(response_body)
                        }
                    } else if path_params.component_id == "telematics" {
                        let mut response_items = Vec::new();
                        let empty_vec = Vec::new();

                        // Only for the current component
                        if server_config.host_name == path_params.component_id {
                            let sovd_apps_list = server_config
                                .get_apps_by_component_id(path_params.component_id.as_str())
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
                            ComponentsComponentIdRelatedAppsGetResponse::Status200_ResponseBody(
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
                        Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error))
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
                        ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(
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
                    path_params.component_id, server_config.host_name
                );
                // Only for the current component
                if server_config.host_name == path_params.component_id {
                    let sovd_apps_list = server_config
                        .get_apps_by_component_id(path_params.component_id.as_str())
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
                let response_body = ComponentsComponentIdRelatedAppsGetResponse::Status200_ResponseBody(
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
            Ok(ComponentsComponentIdRelatedAppsGetResponse::Status0_AnUnexpectedRequestOccurred(error))
        }
    }

    /// ComponentsComponentIdSubcomponentsGet - GET /v1/components/{component_id}/subcomponents
    async fn components_component_id_subcomponents_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::ComponentsComponentIdSubcomponentsGetPathParams,
        query_params: &models::ComponentsComponentIdSubcomponentsGetQueryParams,
    ) -> Result<ComponentsComponentIdSubcomponentsGetResponse, ()> {
        info!("components_component_id_subcomponents_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// EntityCollectionEntityIdGet - GET /v1/{entity_collection}/{entity_id}
    async fn entity_collection_entity_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdGetPathParams,
    ) -> Result<EntityCollectionEntityIdGetResponse, ()> {
        info!("entity_collection_entity_id_get({} {:?} {:?} {:?})", method, host, cookies, path_params);

        if let Some(server_config) = get_server_config() {
            info!("Server configuration initialized!");
            if path_params.entity_collection.to_lowercase().as_str() ==
                "apps"
            {
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

                let app_id = path_params.entity_id.clone();
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
                        path_params.entity_id.clone(),
                        app.name.clone(),
                    );
                    let app_data = format!(
                        "{}/{}/{}/data",
                        server_config.base_uri,
                        path_params.entity_collection.clone(),
                        app.id.clone()
                    );

                    response.data = Some(app_data);

                    return Ok(EntityCollectionEntityIdGetResponse::Status200_TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                } else {
                    //Check if gateway mode is active, because perhaps the app is on another device
                    if let Some(server_config) = get_server_config() {
                        if server_config.get_sovd_mode() == "gateway" {
                            let mdns = Arc::new(Mutex::new(ServiceDaemon::new().expect("Failed to create mDNS daemon")));
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
                                            ip_address, port, path_params.entity_id
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
                                                            return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                        return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                                    }
                                                };

                                                let mut json_value: Value =
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
                                                            return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                                return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                        return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                                    return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
                                }
                            } else {
                                let error = AnyPathDocsGetDefaultResponse {
                                    error_code: "InstanceNameNotFound".to_string(),
                                    message: "No standalone instance name found.".to_string(),
                                    vendor_code: None,
                                    translation_id: None,
                                    parameters: None,
                                };
                                return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
                            EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error),
                        );
                    }

                    let error = AnyPathDocsGetDefaultResponse {
                        error_code: "EntityNotFound".to_string(),
                        message: format!("Entity '{}' not found.", path_params.entity_id),
                        vendor_code: None,
                        translation_id: None,
                        parameters: None,
                    };
                    return Ok(
                        EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error),
                    );
                }
            } else if path_params.entity_collection.to_lowercase().as_str()
                == "components"
            {
                // Declaration of id and name as Option
                let mut id: Option<String> = None;
                let mut name: Option<String> = None;

                let scheme = EntityCollectionGetQueryParams { include_schema: None};
                let params = EntityCollectionGetPathParams { entity_collection: path_params.entity_collection.clone() };
                // Call entity_collection_get and process the response
                match self
                    .entity_collection_get(&method, &host, &cookies, &params, &scheme)
                    .await
                {
                    Ok(EntityCollectionGetResponse::Status200_ResponseBody(response_body)) => {
                        // Extract the required data from the response
                        for entity_ref in &response_body.items {
                            if entity_ref.id == path_params.entity_id {
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
                                path_params.entity_collection.clone(),
                                id.clone()
                            );
                            response.data = Some(app_data);

                            return Ok(EntityCollectionEntityIdGetResponse::Status200_TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                        } else {
                            // If no matching entity was found
                            let error = AnyPathDocsGetDefaultResponse {
                                error_code: "EntityNotFound".to_string(),
                                message: format!("Entity '{}' not found.", path_params.entity_id),
                                vendor_code: None,
                                translation_id: None,
                                parameters: None,
                            };
                            return Ok(
                                EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(
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
                            EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error),
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
                return Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error));
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
        Ok(EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(error))
    }

    /// EntityCollectionGet - GET /v1/{entity_collection}
    async fn entity_collection_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionGetPathParams,
        query_params: &models::EntityCollectionGetQueryParams,
    ) -> Result<EntityCollectionGetResponse, ()> {
        info!("entity_collection_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);

        // Directly extract from the server_config structure
        if let Some(server_config) = get_server_config() {
            if path_params.entity_collection.to_lowercase().as_str() == "components"
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
                    let mdns = Arc::new(Mutex::new(ServiceDaemon::new().expect("Failed to create mDNS daemon")));
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
                        return Ok(EntityCollectionGetResponse::Status0_AnUnexpectedRequestOccurred(
                            error,
                        ));
                    }
                }

                // Create InlineResponse200 with the EntityReferences and optionally the schema
                let mut response_body =
                    models::EntityCollectionGet200Response::new(entity_references);
                if let Some(include_schema) = query_params.include_schema
                    && include_schema
                {
                    // Set the schema if required
                    response_body.schema = Some(false);
                }

                // Create EntityCollectionGetResponse with Status200_ResponseBody
                return Ok(EntityCollectionGetResponse::Status200_ResponseBody(response_body));
            }
        } else {
            info!("Server configuration not initialized!");
        }

        // If the value of entity_collection is not "components",
        // return EntityCollectionGetResponse::Status0_AnUnexpectedRequestOccurred
        let error = AnyPathDocsGetDefaultResponse {
            error_code: "UnexpectedRequest".to_string(),
            message: "An unexpected request occurred.".to_string(),
            vendor_code: None,
            translation_id: None,
            parameters: None,
        };

        Ok(EntityCollectionGetResponse::Status0_AnUnexpectedRequestOccurred(
            error,
        ))
    }
}