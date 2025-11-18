use once_cell::sync::Lazy;
use reqwest::Client;
use sovd_handlers::get_process_pid;
use sovd_server::server_config::ServerConfig;
use sovd_server::sovd_server::spawn_test_server;
use tokio::task::JoinHandle;
use std::sync::Mutex;
use std::time::Duration;

static SERVER_HANDLE: Lazy<Mutex<Option<JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));
// Static configuration for the test server using Lazy initialization
static SERVER_CONFIG: Lazy<ServerConfig> = Lazy::new(|| {
    ServerConfig::create_server_settings(
        "../config/sovd_server_apps.conf", // Path to server config file
        "http".to_string(),                // Protocol
        "127.0.0.1".to_string(),           // Host
        "0".to_string(),                   // Port (0 means auto-assign)
        "standalone".to_string(),          // Mode
        "chassis-hpc".to_string(),         // Component name
    )
    .expect("Failed to create server config")
});

// Static variable to store the server address once started
static SERVER_ADDR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

// Static HTTP client used for sending requests
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .no_proxy() // Disable proxy
        .build()
        .expect("Failed to build reqwest client")
});

// Starts the test server if not already started

async fn start_server() {
    let mut addr_lock = SERVER_ADDR.lock().unwrap();
    if addr_lock.is_none() {
        let (addr, handle) = spawn_test_server(&SERVER_CONFIG).await;
        *addr_lock = Some(addr.to_string());

        let mut handle_lock = SERVER_HANDLE.lock().unwrap();
        *handle_lock = Some(handle);

        drop(addr_lock);
        drop(handle_lock);

        wait_for_server_ready(&addr.to_string()).await;
    }
}


// Retrieves the server address from the static variable
fn get_server_addr() -> String {
    SERVER_ADDR
        .lock()
        .unwrap()
        .clone()
        .expect("Server address not set")
}

// Builds the path for accessing app-specific resources
fn build_app_path_resources(resource: &str) -> String {
    let pid = get_process_pid("sovd-server").expect("Fail to read pid");
    format!(
        "v1/apps/sovd-server-{}/data/sovd-server-{}-{}",
        pid, pid, resource
    )
}

// Sends a GET request to the specified endpoint and asserts success
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


async fn wait_for_server_ready(addr: &str) {
    let urls = [
        format!("http://{}/v1/components", addr),
        format!("http://{}/v1/apps", addr),
    ];
    let max_attempts = 50; // ~10s
    for _ in 0..max_attempts {
        let mut all_ready = true;
        for url in &urls {
            if let Ok(resp) = CLIENT.get(url).send().await {
                if !resp.status().is_success() {
                    all_ready = false;
                    break;
                }
            } else {
                all_ready = false;
                break;
            }
        }
        if all_ready {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("Server did not become ready in time");
}


// Integration test: Get general component information
#[tokio::test]
async fn get_component_info() {
    get_and_assert_endpoint("v1/components").await;
}

// Integration test: Get data for a specific component
#[tokio::test]
async fn get_component_data() {
    get_and_assert_endpoint("v1/components/chassis-hpc").await;
}

// Integration test: Get detailed data for a specific component
#[tokio::test]
async fn get_component_specific_data() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data").await;
}

// Integration test: Get CPU usage for a specific component
#[tokio::test]
async fn get_component_specific_cpu_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-cpu").await;
}

// Integration test: Get disk usage for a specific component
#[tokio::test]
async fn get_component_specific_disk_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-disk").await;
}

// Integration test: Get memory usage for a specific component
#[tokio::test]
async fn get_component_specific_memory_usage() {
    get_and_assert_endpoint("v1/components/chassis-hpc/data/chassis-hpc-memory").await;
}

// Integration test: Get related applications for a component
#[tokio::test]
async fn get_related_apps() {
    get_and_assert_endpoint("v1/components/chassis-hpc/related-apps").await;
}

// Integration test: Get information about a specific app using its PID
#[tokio::test]
async fn get_specific_app() {
    let path = format!(
        "v1/apps/sovd-server-{}",
        get_process_pid("sovd-server").expect("Fail to read pid")
    );
    get_and_assert_endpoint(&path).await;
}

// Integration test: Get data for a specific app
#[tokio::test]
async fn get_specific_app_data() {
    let path = format!(
        "v1/apps/sovd-server-{}/data",
        get_process_pid("sovd-server").expect("Fail to read pid")
    );
    get_and_assert_endpoint(&path).await;
}

// Integration test: Get CPU usage data for a specific app
#[tokio::test]
async fn get_specific_app_cpu() {
    let path = build_app_path_resources("cpu");
    get_and_assert_endpoint(&path).await;
}

// Integration test: Get memory usage data for a specific app
#[tokio::test]
async fn get_specific_app_memory() {
    let path = build_app_path_resources("memory");
    get_and_assert_endpoint(&path).await;
}

// Integration test: Get disk usage data for a specific app
#[tokio::test]
async fn get_specific_app_disk() {
    let path = build_app_path_resources("disk");
    get_and_assert_endpoint(&path).await;
}

// Integration test: Get all resource data for a specific app
#[tokio::test]
async fn get_specific_app_all() {
    let path = build_app_path_resources("all");
    get_and_assert_endpoint(&path).await;
}
