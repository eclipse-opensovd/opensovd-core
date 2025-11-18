use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
use openapi::{
    apis::updates::{
        UpdatesGetResponse, UpdatesPostResponse, UpdatesUpdatePackageIdAutomatedPutResponse,
        UpdatesUpdatePackageIdDeleteResponse, UpdatesUpdatePackageIdExecutePutResponse,
        UpdatesUpdatePackageIdGetResponse, UpdatesUpdatePackageIdPreparePutResponse,
        UpdatesUpdatePackageIdStatusGetResponse,
    },
    models, types,
};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::updates::Updates<()> for ServerImpl {
    /// UpdatesGet - GET /v1/updates
    async fn updates_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        query_params: &models::UpdatesGetQueryParams,
    ) -> Result<UpdatesGetResponse, ()> {
        info!("updates_get({} {:?} {:?} {:?})", method, host, cookies, query_params,);
        Err(())
    }

    /// UpdatesPost - POST /v1/updates
    async fn updates_post(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        header_params: &models::UpdatesPostHeaderParams,
        body: &Option<types::Object>,
    ) -> Result<UpdatesPostResponse, ()> {    
        info!("updates_post({} {:?} {:?} {:?} {:?})", method, host, cookies, header_params, body);
        Err(())
    }

    /// UpdatesUpdatePackageIdAutomatedPut - PUT /v1/updates/{update_package_id}/automated
    async fn updates_update_package_id_automated_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdAutomatedPutPathParams,
    ) -> Result<UpdatesUpdatePackageIdAutomatedPutResponse, ()> {
        info!("updates_update_package_id_automated_put({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// UpdatesUpdatePackageIdDelete - DELETE /v1/updates/{update_package_id}
    async fn updates_update_package_id_delete(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdDeletePathParams,
    ) -> Result<UpdatesUpdatePackageIdDeleteResponse, ()> {
        info!("updates_update_package_id_delete({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// UpdatesUpdatePackageIdExecutePut - PUT /v1/updates/{update_package_id}/execute
    async fn updates_update_package_id_execute_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdExecutePutPathParams,
    ) -> Result<UpdatesUpdatePackageIdExecutePutResponse, ()> {
        info!("updates_update_package_id_execute_put({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// UpdatesUpdatePackageIdGet - GET /v1/updates/{update_package_id}
    async fn updates_update_package_id_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdGetPathParams,
        query_params: &models::UpdatesUpdatePackageIdGetQueryParams,
    ) -> Result<UpdatesUpdatePackageIdGetResponse, ()> {
        info!("updates_update_package_id_get({} {:?} {:?} {:?} {:?})", method, host, cookies, path_params, query_params);
        Err(())
    }

    /// UpdatesUpdatePackageIdPreparePut - PUT /v1/updates/{update_package_id}/prepare
    async fn updates_update_package_id_prepare_put(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdPreparePutPathParams,
    ) -> Result<UpdatesUpdatePackageIdPreparePutResponse, ()> {
        info!("updates_update_package_id_prepare_put({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }

    /// UpdatesUpdatePackageIdStatusGet - GET /v1/updates/{update_package_id}/status
    async fn updates_update_package_id_status_get(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::UpdatesUpdatePackageIdStatusGetPathParams,
    ) -> Result<UpdatesUpdatePackageIdStatusGetResponse, ()> {
        info!("updates_update_package_id_status_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }
}