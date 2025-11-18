use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::discovery::{
        AreasAreaIdRelatedComponentsGetResponse, AreasAreaIdSubareasGetResponse,
        ComponentsComponentIdRelatedAppsGetResponse, ComponentsComponentIdSubcomponentsGetResponse,
        EntityCollectionEntityIdGetResponse, EntityCollectionGetResponse,
    },
    models::{self, AnyPathDocsGetDefaultResponse, EntityCollectionEntityIdGet200Response},
};
use sovd_handlers::find_single_process;

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
        todo!();
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
        todo!();
    }

    /// ComponentsComponentIdRelatedAppsGet - GET /v1/components/{component_id}/related-apps
    async fn components_component_id_related_apps_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::ComponentsComponentIdRelatedAppsGetPathParams,
    ) -> Result<ComponentsComponentIdRelatedAppsGetResponse, ()> {
        todo!();
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
        todo!();
    }

    /// EntityCollectionEntityIdGet - GET /v1/{entity_collection}/{entity_id}
    async fn entity_collection_entity_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdGetPathParams,
    ) -> Result<EntityCollectionEntityIdGetResponse, ()> {
        // Check, if requested entity_id is supported. TODO: include registered
        // sub SOVD Servers. Currently, we support only the component which hosts the SOVD server
        match path_params.entity_collection.as_str() {
            "areas" => todo!(),
            "components" => {
                if path_params.entity_id != self.id {
                    return Ok(
                        EntityCollectionEntityIdGetResponse::Status0_AnUnexpectedRequestOccurred(
                            AnyPathDocsGetDefaultResponse::new(
                                "not-responding".to_owned(),
                                format!("Component {} did not respond.", path_params.entity_id),
                            ),
                        ),
                    );
                }

                let mut response = models::EntityCollectionEntityIdGet200Response::new(
                    self.id.to_owned(),
                    self.name.to_owned(),
                );

                let _ = response.data.insert(format!(
                    "http://{}{}/components/{}/data",
                    host.0,
                    openapi::BASE_PATH,
                    self.id
                ));

                Ok(EntityCollectionEntityIdGetResponse::Status200_TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(
                    response
                ))
            },
            "apps" => {
                let tokens = path_params.entity_id.split('-');

                // Check, if last token is a number (is the PID in that case)
                let last_token = tokens.clone().last().unwrap();
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

                if let Some(app) = find_single_process(&resource, &pid, &format!("http://{}{}",
                    host.0,
                    openapi::BASE_PATH)) {
                    let mut response = EntityCollectionEntityIdGet200Response::new(
                        path_params.entity_id.clone(),
                        app.name.clone(),
                    );
                    let app_data = format!(
                        "http://{}{}/{}/{}/data",
                        host.0,
                        openapi::BASE_PATH,
                        path_params.entity_collection.to_owned(),
                        app.id.clone()
                    );

                    response.data = Some(app_data);

                    return Ok(EntityCollectionEntityIdGetResponse::Status200_TheResponseBodyContainsAPropertyForEachSupportedResourceAndRelatedCollection(response));
                } else {
                    todo!()
                }
            }
            "functions" => todo!(),
            _ => todo!()
        }
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
        // TODO: Besides self, sub SOVD Servers registering via mDNS must be
        // supported here.
        match path_params.entity_collection.as_str() {
            "areas" => todo!(),
            "components" => {
                let mut items = Vec::<models::EntityCollectionGet200ResponseItemsInner>::new();
                items.push(models::EntityCollectionGet200ResponseItemsInner::new(
                    self.id.to_owned(),
                    self.name.to_owned(),
                    String::from(format!(
                        "http://{}{}/components/{}",
                        host.0,
                        openapi::BASE_PATH,
                        self.id
                    )),
                ));

                Ok(EntityCollectionGetResponse::Status200_ResponseBody(
                    models::EntityCollectionGet200Response::new(items),
                ))
            },
            "apps" => {
                Ok(EntityCollectionGetResponse::Status0_AnUnexpectedRequestOccurred(
                    models::AnyPathDocsGetDefaultResponse::new("-1".to_owned(), "Not implemented yet".to_owned())
                ))
            },
            "functions" => todo!(),
            _ => todo!()
        }
    }
}