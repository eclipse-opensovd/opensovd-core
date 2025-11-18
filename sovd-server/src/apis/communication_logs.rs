use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::communication_logs::{
        EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse,
        EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse,
        EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse,
        EntityCollectionEntityIdCommunicationLogsGetResponse,
        EntityCollectionEntityIdCommunicationLogsPostResponse,
    },
    models,
    types::ByteArray,
};
use log::info;
use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::communication_logs::CommunicationLogs<()> for ServerImpl {
    /// EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDelete - DELETE /v1/{entity_collection}/{entity_id}/communication-logs/{communication_log_id}
    async fn entity_collection_entity_id_communication_logs_communication_log_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeletePathParams,
        query_params: &models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteQueryParams,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdDeleteResponse, ()> {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_delete({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGet - GET /v1/{entity_collection}/{entity_id}/communication-logs/{communication_log_id}
    async fn entity_collection_entity_id_communication_logs_communication_log_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetPathParams,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdGetResponse, ()> {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPut - PUT /v1/{entity_collection}/{entity_id}/communication-logs/{communication_log_id}
    async fn entity_collection_entity_id_communication_logs_communication_log_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutPathParams,
        body: &models::EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutRequest,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsCommunicationLogIdPutResponse, ()> {
        info!("entity_collection_entity_id_communication_logs_communication_log_id_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, body);
        Err(())
    }

    /// EntityCollectionEntityIdCommunicationLogsGet - GET /v1/{entity_collection}/{entity_id}/communication-logs
    async fn entity_collection_entity_id_communication_logs_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdCommunicationLogsGetPathParams,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsGetResponse, ()> {
        info!("entity_collection_entity_id_communication_logs_get({} {:?} {:?} {:?}", method, host, cookies, path_params);
        Err(())
    }

    /// EntityCollectionEntityIdCommunicationLogsPost - POST /v1/{entity_collection}/{entity_id}/communication-logs
    async fn entity_collection_entity_id_communication_logs_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdCommunicationLogsPostPathParams,
        query_params: &models::EntityCollectionEntityIdCommunicationLogsPostQueryParams,
        body: &ByteArray,
    ) -> Result<EntityCollectionEntityIdCommunicationLogsPostResponse, ()> {
        info!("entity_collection_entity_id_communication_logs_post({} {:?} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params, body);
        Err(())
    }
}