use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
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
        info!("delete_all_faults({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// DeleteFaultById - DELETE /v1/{entity_collection}/{entity_id}/faults/{fault_code}
    async fn delete_fault_by_id(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::DeleteFaultByIdPathParams,
    ) -> Result<DeleteFaultByIdResponse, ()> {
        info!("delete_fault_by_id({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
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
        info!("get_fault_by_id({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
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
        info!("get_faults({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }
}