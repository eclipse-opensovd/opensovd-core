use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::configurations::{
        EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse,
        EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse,
        EntityCollectionEntityIdConfigurationsGetResponse,
    },
    models,
};

use crate::ServerImpl;
use log::info;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::configurations::Configurations<()> for ServerImpl {
    /// EntityCollectionEntityIdConfigurationsConfigurationIdGet - GET /v1/{entity_collection}/{entity_id}/configurations/{configuration_id}
    async fn entity_collection_entity_id_configurations_configuration_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdConfigurationsConfigurationIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdConfigurationsConfigurationIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdGetResponse, ()> {
        info!("entity_collection_entity_id_configurations_configuration_id_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// EntityCollectionEntityIdConfigurationsConfigurationIdPut - PUT /v1/{entity_collection}/{entity_id}/configurations/{configuration_id}
    async fn entity_collection_entity_id_configurations_configuration_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdConfigurationsConfigurationIdPutPathParams,
        body: &models::EntityCollectionEntityIdConfigurationsConfigurationIdPutRequest,
    ) -> Result<EntityCollectionEntityIdConfigurationsConfigurationIdPutResponse, ()> {
        info!("entity_collection_entity_id_configurations_configuration_id_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, body);
        Err(())
    }

    /// EntityCollectionEntityIdConfigurationsGet - GET /v1/{entity_collection}/{entity_id}/configurations
    async fn entity_collection_entity_id_configurations_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdConfigurationsGetPathParams,
        query_params: &models::EntityCollectionEntityIdConfigurationsGetQueryParams,
    ) -> Result<EntityCollectionEntityIdConfigurationsGetResponse, ()> {
        info!("entity_collection_entity_id_configurations_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }
}