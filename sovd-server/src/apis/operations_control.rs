use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::operations_control::{
        EntityCollectionEntityIdOperationsGetResponse,
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse,
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse,
        EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse,
        EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse,
        EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse,
        EntityCollectionEntityIdOperationsOperationIdGetResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::operations_control::OperationsControl<()> for ServerImpl {
    /// EntityCollectionEntityIdOperationsGet - GET /v1/{entity_collection}/{entity_id}/operations
    async fn entity_collection_entity_id_operations_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsGetPathParams,
        query_params: &models::EntityCollectionEntityIdOperationsGetQueryParams,
    ) -> Result<EntityCollectionEntityIdOperationsGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDelete - DELETE /v1/{entity_collection}/{entity_id}/operations/{operation_id}/executions/{execution_id}
    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeletePathParams,
        body: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteRequest,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdDeleteResponse, ()>
    {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGet - GET /v1/{entity_collection}/{entity_id}/operations/{operation_id}/executions/{execution_id}
    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdGetResponse, ()>
    {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPut - PUT /v1/{entity_collection}/{entity_id}/operations/{operation_id}/executions/{execution_id}
    async fn entity_collection_entity_id_operations_operation_id_executions_execution_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutPathParams,
        body: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutRequest,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsExecutionIdPutResponse, ()>
    {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdExecutionsGet - GET /v1/{entity_collection}/{entity_id}/operations/{operation_id}/executions
    async fn entity_collection_entity_id_operations_operation_id_executions_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsGetPathParams,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdExecutionsPost - POST /v1/{entity_collection}/{entity_id}/operations/{operation_id}/executions
    async fn entity_collection_entity_id_operations_operation_id_executions_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsPostPathParams,
        body: &models::EntityCollectionEntityIdOperationsOperationIdExecutionsPostRequest,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdExecutionsPostResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdOperationsOperationIdGet - GET /v1/{entity_collection}/{entity_id}/operations/{operation_id}
    async fn entity_collection_entity_id_operations_operation_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdOperationsOperationIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdOperationsOperationIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdOperationsOperationIdGetResponse, ()> {
        todo!();
    }
}