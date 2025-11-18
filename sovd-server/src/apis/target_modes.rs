use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::target_modes::{
        EntityCollectionEntityIdModesGetResponse, EntityCollectionEntityIdModesModeIdGetResponse,
        EntityCollectionEntityIdModesModeIdPutResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::target_modes::TargetModes<()> for ServerImpl {
    /// EntityCollectionEntityIdModesGet - GET /v1/{entity_collection}/{entity_id}/modes
    async fn entity_collection_entity_id_modes_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdModesGetPathParams,
        query_params: &models::EntityCollectionEntityIdModesGetQueryParams,
    ) -> Result<EntityCollectionEntityIdModesGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdModesModeIdGet - GET /v1/{entity_collection}/{entity_id}/modes/{mode_id}
    async fn entity_collection_entity_id_modes_mode_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdModesModeIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdModesModeIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdModesModeIdGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdModesModeIdPut - PUT /v1/{entity_collection}/{entity_id}/modes/{mode_id}
    async fn entity_collection_entity_id_modes_mode_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdModesModeIdPutPathParams,
        body: &models::EntityCollectionEntityIdModesModeIdPutRequest,
    ) -> Result<EntityCollectionEntityIdModesModeIdPutResponse, ()> {
        todo!();
    }
}