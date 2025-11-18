use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use openapi::{
    apis::fault_handling::{
        DeleteAllFaultsResponse, DeleteFaultByIdResponse, GetFaultByIdResponse, GetFaultsResponse,
    },
    models,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::fault_handling::FaultHandling<()> for ServerImpl {
    /// DeleteAllFaults - DELETE /v1/{entity_collection}/{entity_id}/faults
    async fn delete_all_faults(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteAllFaultsPathParams,
        query_params: &models::DeleteAllFaultsQueryParams,
    ) -> Result<DeleteAllFaultsResponse, ()> {
        todo!();
    }

    /// DeleteFaultById - DELETE /v1/{entity_collection}/{entity_id}/faults/{fault_code}
    async fn delete_fault_by_id(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteFaultByIdPathParams,
    ) -> Result<DeleteFaultByIdResponse, ()> {
        todo!();
    }

    /// GetFaultById - GET /v1/{entity_collection}/{entity_id}/faults/{fault_code}
    async fn get_fault_by_id(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetFaultByIdPathParams,
        query_params: &models::GetFaultByIdQueryParams,
    ) -> Result<GetFaultByIdResponse, ()> {
        todo!();
    }

    /// GetFaults - GET /v1/{entity_collection}/{entity_id}/faults
    async fn get_faults(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::GetFaultsPathParams,
        query_params: &models::GetFaultsQueryParams,
    ) -> Result<GetFaultsResponse, ()> {
        todo!();
    }
}