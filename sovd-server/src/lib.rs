pub mod server_config;
pub mod sovd_server;
pub mod apis;

#[derive(Clone)]
pub struct ServerImpl {
    pub id: String,
    pub name: String,
}

