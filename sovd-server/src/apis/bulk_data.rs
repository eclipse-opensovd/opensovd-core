use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
use openapi::{
    apis::bulk_data::{
        EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse,
        EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse,
        EntityCollectionEntityIdBulkDataCategoryDeleteResponse,
        EntityCollectionEntityIdBulkDataCategoryGetResponse,
        EntityCollectionEntityIdBulkDataCategoryPostResponse,
        EntityCollectionEntityIdBulkDataGetResponse,
    },
    models,
    types::ByteArray,
};
use openapi::models::*;

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::bulk_data::BulkData<()> for ServerImpl {
    /// EntityCollectionEntityIdBulkDataCategoryBulkDataIdDelete - DELETE /v1/{entity_collection}/{entity_id}/bulk-data/{category}/{bulk_data_id}
    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeletePathParams,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdDeleteResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdBulkDataCategoryBulkDataIdGet - GET /v1/{entity_collection}/{entity_id}/bulk-data/{category}/{bulk_data_id}
    async fn entity_collection_entity_id_bulk_data_category_bulk_data_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetHeaderParams,
        path_params: &models::EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetPathParams,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryBulkDataIdGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdBulkDataCategoryDelete - DELETE /v1/{entity_collection}/{entity_id}/bulk-data/{category}
    async fn entity_collection_entity_id_bulk_data_category_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdBulkDataCategoryDeletePathParams,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryDeleteResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdBulkDataCategoryGet - GET /v1/{entity_collection}/{entity_id}/bulk-data/{category}
    async fn entity_collection_entity_id_bulk_data_category_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdBulkDataCategoryGetPathParams,
        query_params: &models::EntityCollectionEntityIdBulkDataCategoryGetQueryParams,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdBulkDataCategoryPost - POST /v1/{entity_collection}/{entity_id}/bulk-data/{category}
    async fn entity_collection_entity_id_bulk_data_category_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::EntityCollectionEntityIdBulkDataCategoryPostHeaderParams,
        path_params: &models::EntityCollectionEntityIdBulkDataCategoryPostPathParams,
        body: &ByteArray,
    ) -> Result<EntityCollectionEntityIdBulkDataCategoryPostResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdBulkDataGet - GET /v1/{entity_collection}/{entity_id}/bulk-data
    async fn entity_collection_entity_id_bulk_data_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdBulkDataGetPathParams,
        query_params: &models::EntityCollectionEntityIdBulkDataGetQueryParams,
    ) -> Result<EntityCollectionEntityIdBulkDataGetResponse, ()> {
        info!("entity_collection_entity_id_bulk_data_get({}, {:?}, {:?}, {:?}, {:?})", method, host, cookies, path_params, query_params);

        // Create a vector of categories based on the entity_id and entity_collection
        let categories: Vec<String> = vec![];

        // Create an instance of EntityCollectionEntityIdBulkDataGet200Response with retrieved categories
        let inline_response = EntityCollectionEntityIdBulkDataGet200Response::new(categories);

        // Check if include_schema is true and set schema accordingly
        let schema = if let Some(true) = query_params.include_schema {
            Some(false)
        } else {
            None
        };

        // Attach schema to EntityCollectionEntityIdBulkDataGet200Response
        let inline_response = EntityCollectionEntityIdBulkDataGet200Response {
            items: inline_response.items,
            schema,
        };

        // Create the response body variant
        let response =
            EntityCollectionEntityIdBulkDataGetResponse::Status200_TheBulkDataCategoriesSupportedByTheEntity(
                inline_response,
            );

        // Return the response
        Ok(response)
    }
}