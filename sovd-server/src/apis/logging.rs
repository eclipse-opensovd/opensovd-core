use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
use openapi::{
    apis::logging::{
        EntityCollectionEntityIdLogsConfigDeleteResponse,
        EntityCollectionEntityIdLogsConfigGetResponse,
        EntityCollectionEntityIdLogsConfigPutResponse,
        EntityCollectionEntityIdLogsEntriesGetResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::logging::Logging<()> for ServerImpl {
    /// EntityCollectionEntityIdLogsConfigDelete - DELETE /v1/{entity_collection}/{entity_id}/logs/config
    async fn entity_collection_entity_id_logs_config_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLogsConfigDeletePathParams,
    ) -> Result<EntityCollectionEntityIdLogsConfigDeleteResponse, ()> {
        info!("entity_collection_entity_id_logs_config_delete({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdLogsConfigGet - GET /v1/{entity_collection}/{entity_id}/logs/config
    async fn entity_collection_entity_id_logs_config_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLogsConfigGetPathParams,
    ) -> Result<EntityCollectionEntityIdLogsConfigGetResponse, ()> {
        info!("entity_collection_entity_id_logs_config_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdLogsConfigPut - PUT /v1/{entity_collection}/{entity_id}/logs/config
    async fn entity_collection_entity_id_logs_config_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLogsConfigPutPathParams,
        body: &models::EntityCollectionEntityIdLogsConfigPutRequest,
    ) -> Result<EntityCollectionEntityIdLogsConfigPutResponse, ()> {
        info!("entity_collection_entity_id_logs_config_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, body);
        Err(())
    }

    /// EntityCollectionEntityIdLogsEntriesGet - GET /v1/{entity_collection}/{entity_id}/logs/entries
    async fn entity_collection_entity_id_logs_entries_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLogsEntriesGetPathParams,
        query_params: &models::EntityCollectionEntityIdLogsEntriesGetQueryParams,
    ) -> Result<EntityCollectionEntityIdLogsEntriesGetResponse, ()> {
        info!("entity_collection_entity_id_logs_config_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }
}