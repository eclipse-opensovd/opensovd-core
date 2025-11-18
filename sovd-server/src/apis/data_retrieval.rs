use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::data_retrieval::{
        EntityCollectionEntityIdDataCategoriesGetResponse,
        EntityCollectionEntityIdDataDataIdGetResponse,
        EntityCollectionEntityIdDataDataIdPutResponse, EntityCollectionEntityIdDataGetResponse,
        EntityCollectionEntityIdDataGroupsGetResponse,
        EntityCollectionEntityIdDataListsDataListIdDeleteResponse,
        EntityCollectionEntityIdDataListsDataListIdGetResponse,
        EntityCollectionEntityIdDataListsGetResponse,
        EntityCollectionEntityIdDataListsPostResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::data_retrieval::DataRetrieval<()> for ServerImpl {
    /// EntityCollectionEntityIdDataCategoriesGet - GET /v1/{entity_collection}/{entity_id}/data-categories
    async fn entity_collection_entity_id_data_categories_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataCategoriesGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataCategoriesGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataDataIdGet - GET /v1/{entity_collection}/{entity_id}/data/{data_id}
    async fn entity_collection_entity_id_data_data_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataDataIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataDataIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataDataIdGetResponse, ()> {
        Ok(sovd_handlers::handle_system_resource(
            &path_params.data_id,
            &path_params.entity_id,
            &path_params.data_id,
        ))
    }

    /// EntityCollectionEntityIdDataDataIdPut - PUT /v1/{entity_collection}/{entity_id}/data/{data_id}
    async fn entity_collection_entity_id_data_data_id_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataDataIdPutPathParams,
        body: &models::EntityCollectionEntityIdDataDataIdPutRequest,
    ) -> Result<EntityCollectionEntityIdDataDataIdPutResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataGet - GET /v1/{entity_collection}/{entity_id}/data
    async fn entity_collection_entity_id_data_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataGetResponse, ()> {
        let resource_names = ["CPU", "Disk", "Memory", "All"];
        let mut items: Vec<models::EntityCollectionEntityIdDataGet200ResponseItemsInner> =
            Vec::new();
        for resource_name in resource_names {
            items.push(
                models::EntityCollectionEntityIdDataGet200ResponseItemsInner::new(
                    resource_name.to_lowercase(),
                    format!(
                        "Current {} usage for {}",
                        resource_name, path_params.entity_id
                    ),
                    "sysInfo".to_owned(),
                ),
            );
        }
        Ok(
            EntityCollectionEntityIdDataGetResponse::Status200_TheRequestWasSuccessful(
                models::EntityCollectionEntityIdDataGet200Response::new(items),
            ),
        )
    }

    /// EntityCollectionEntityIdDataGroupsGet - GET /v1/{entity_collection}/{entity_id}/data-groups
    async fn entity_collection_entity_id_data_groups_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataGroupsGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataGroupsGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataListsDataListIdDelete - DELETE /v1/{entity_collection}/{entity_id}/data-lists/{data_list_id}
    async fn entity_collection_entity_id_data_lists_data_list_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsDataListIdDeletePathParams,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdDeleteResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataListsDataListIdGet - GET /v1/{entity_collection}/{entity_id}/data-lists/{data_list_id}
    async fn entity_collection_entity_id_data_lists_data_list_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsDataListIdGetPathParams,
        query_params: &models::EntityCollectionEntityIdDataListsDataListIdGetQueryParams,
    ) -> Result<EntityCollectionEntityIdDataListsDataListIdGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataListsGet - GET /v1/{entity_collection}/{entity_id}/data-lists
    async fn entity_collection_entity_id_data_lists_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsGetPathParams,
    ) -> Result<EntityCollectionEntityIdDataListsGetResponse, ()> {
        todo!();
    }

    /// EntityCollectionEntityIdDataListsPost - POST /v1/{entity_collection}/{entity_id}/data-lists
    async fn entity_collection_entity_id_data_lists_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::EntityCollectionEntityIdDataListsPostPathParams,
        body: &models::EntityCollectionEntityIdDataListsPostRequest,
    ) -> Result<EntityCollectionEntityIdDataListsPostResponse, ()> {
        todo!();
    }
}