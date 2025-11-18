use async_trait::async_trait;
use axum::http::Method;
use axum_extra::extract::{CookieJar, Host};
use log::info;
use openapi::{apis::capabilities::AnyPathDocsGetResponse, models};

use crate::ServerImpl;

#[allow(unused_variables)]
#[async_trait]
impl openapi::apis::capabilities::Capabilities<()> for ServerImpl {
    /// AnyPathDocsGet - GET /v1/{any_path}/docs
    async fn any_path_docs_get(
        &self,
        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::AnyPathDocsGetPathParams,
    ) -> Result<AnyPathDocsGetResponse, ()> {
        info!("any_path_docs_get({} {:?} {:?} {:?})", method, host, cookies, path_params);
        Err(())
    }
}