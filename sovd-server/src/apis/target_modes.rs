use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
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
        info!("entity_collection_entity_id_modes_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
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
        info!("entity_collection_entity_id_modes_mode_id_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
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
        info!("entity_collection_entity_id_modes_mode_id_put({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, body);
        Err(())
    }
}