use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::locking::{
        EntityCollectionEntityIdLocksGetResponse,
        EntityCollectionEntityIdLocksLockIdDeleteResponse,
        EntityCollectionEntityIdLocksLockIdGetResponse,
        EntityCollectionEntityIdLocksLockIdPutResponse, EntityCollectionEntityIdLocksPostResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::locking::Locking<()> for ServerImpl {
    /// EntityCollectionEntityIdLocksGet - GET /v1/{entity_collection}/{entity_id}/locks
    async fn entity_collection_entity_id_locks_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLocksGetPathParams,
    ) -> Result<EntityCollectionEntityIdLocksGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdLocksLockIdDelete - DELETE /v1/{entity_collection}/{entity_id}/locks/{lock_id}
    async fn entity_collection_entity_id_locks_lock_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLocksLockIdDeletePathParams,
    ) -> Result<EntityCollectionEntityIdLocksLockIdDeleteResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdLocksLockIdGet - GET /v1/{entity_collection}/{entity_id}/locks/{lock_id}
    async fn entity_collection_entity_id_locks_lock_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLocksLockIdGetPathParams,
    ) -> Result<EntityCollectionEntityIdLocksLockIdGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdLocksLockIdPut - PUT /v1/{entity_collection}/{entity_id}/locks/{lock_id}
    async fn entity_collection_entity_id_locks_lock_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLocksLockIdPutPathParams,
        body: &models::EntityCollectionEntityIdLocksPostRequest,
    ) -> Result<EntityCollectionEntityIdLocksLockIdPutResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdLocksPost - POST /v1/{entity_collection}/{entity_id}/locks
    async fn entity_collection_entity_id_locks_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdLocksPostPathParams,
        body: &models::EntityCollectionEntityIdLocksPostRequest,
    ) -> Result<EntityCollectionEntityIdLocksPostResponse, ()> {
        todo!();
    }
}