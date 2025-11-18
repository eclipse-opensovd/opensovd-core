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
        todo!();
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
        todo!();
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
        todo!();
    }
}