use crate::ServerImpl;
pub mod bulk_data;
pub mod capabilities;
pub mod communication_logs;
pub mod configurations;
pub mod data_retrieval;
pub mod discovery;
pub mod fault_handling;
pub mod locking;
pub mod logging;
pub mod operations_control;
pub mod target_modes;
pub mod updates;

use axum::http::StatusCode;
use log::info;
use async_trait::async_trait;


#[async_trait]
impl openapi::apis::ErrorHandler<()> for ServerImpl {
    async fn handle_error(
            &self,
            _method: &axum::http::Method,
            _host: &axum_extra::extract::Host,
            _cookies: &axum_extra::extract::CookieJar,
            error: ()
        ) -> Result<axum::response::Response, StatusCode> {
            info!("Unhandled error: {:?}", error);
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::empty())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
}

