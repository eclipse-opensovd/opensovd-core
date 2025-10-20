use sovd_server::sovd_server::spawn_test_server;
use sovd_server::server_config::ServerConfig;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::time::Duration;
use reqwest::Client;
use sovd_handlers::get_process_pid;

static SERVER_CONFIG: Lazy<ServerConfig> = Lazy::new(|| {
    ServerConfig::create_server_settings(
        "../target/debug/sovd_server_apps.conf",
        "http".to_string(),
        "127.0.0.1".to_string(),
        "0".to_string(),
        "standalone".to_string(),
        "chassis-hpc".to_string(),
    ).expect("Failed to create server config")
});

static SERVER_ADDR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .no_proxy()
        .build()
        .expect("Failed to build reqwest client")
});

async fn start_server() {
    let mut addr_lock = SERVER_ADDR.lock().unwrap();
    if addr_lock.is_none() {
        let (addr, _handle) = spawn_test_server(&SERVER_CONFIG).await;
        *addr_lock = Some(addr.to_string());
    }
}

fn get_server_addr() -> String {
    SERVER_ADDR.lock().unwrap().clone().expect("Server address not set")
}

async fn get_and_assert_endpoint(path: &str) {
    start_server().await;
    let url = format!("http://{}/{}", get_server_addr(), path);
    let response = CLIENT
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success(), "Request to {} failed", url);

    let body = response.text().await.expect("Failed to read response body");
    assert!(!body.is_empty(), "Response body should not be empty");
}

#[tokio::test]
async fn get_component_info() {
    get_and_assert_endpoint("v1/components").await;
}

#[tokio::test]
async fn get_component_data() {
    get_and_assert_endpoint("v1/components/chassis-hpc").await;
}

#[tokio::test]
async fn get_component_specific_data() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data").await;
}

#[tokio::test]
async fn get_component_specific_cpu_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-cpu").await;
}

#[tokio::test]
async fn get_component_specific_disk_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-disk").await;
}

#[tokio::test]
async fn get_component_specific_memory_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-memory").await;
}

#[tokio::test]
async fn get_related_apps() {
    get_and_assert_endpoint("v1/components/chassis-hpc/related-apps").await;
}

#[tokio::test]
async fn get_specific_app() {
    let path = format!("v1/apps/sovd-server-{}", get_process_pid("sovd-server"));
    get_and_assert_endpoint(&path).await;
}

#[tokio::test]
async fn get_specific_app_data() {
    let path = format!("v1/apps/sovd-server-{}/data", get_process_pid("sovd-server"));
    get_and_assert_endpoint(&path).await;
}

#[tokio::test]
async fn get_specific_app_cpu() {
    let pid = get_process_pid("sovd-server");
    let path = format!("v1/apps/sovd-server-{}/data/sovd-server-{}-cpu", pid, pid );
    get_and_assert_endpoint(&path).await;
}

#[tokio::test]
async fn get_specific_app_memory() {
    let pid = get_process_pid("sovd-server");
    let path = format!("v1/apps/sovd-server-{}/data/sovd-server-{}-memory", pid, pid );
    get_and_assert_endpoint(&path).await;
}

#[tokio::test]
async fn get_specific_app_disk() {
    let pid = get_process_pid("sovd-server");
    let path = format!("v1/apps/sovd-server-{}/data/sovd-server-{}-disk", pid, pid );
    get_and_assert_endpoint(&path).await;
}

#[tokio::test]
async fn get_specific_app_all() {
    let pid = get_process_pid("sovd-server");
    let path = format!("v1/apps/sovd-server-{}/data/sovd-server-{}-all", pid, pid );
    get_and_assert_endpoint(&path).await;
}