use once_cell::sync::Lazy;
use reqwest::Client;
use sovd_handlers::get_process_pid;
use sovd_server::server_config::ServerConfig;
use sovd_server::sovd_server::spawn_test_server;
use std::net::SocketAddr;
use std::time::Duration;

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

// Static HTTP client used for sending requests
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .no_proxy() // Disable proxy
        .build()
        .expect("Failed to build reqwest client")
});

// Helper: Wait for server readiness
async fn wait_for_server_ready(addr: &str) {
    let urls = [
        format!("http://{}/v1/components", addr),
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

// Helper: Start server for each test, run test logic, then shutdown
async fn run_with_test_server<F, Fut>(test_fn: F)
where
    F: FnOnce(SocketAddr) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (addr, handle) = spawn_test_server(&SERVER_CONFIG).await;
    wait_for_server_ready(&addr.to_string()).await;
    test_fn(addr).await;
    handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await; // Allow cleanup
}

// Helper: Build app-specific resource path
fn build_app_path_resources(resource: &str) -> String {
    let pid = get_process_pid("sovd-server").expect("Fail to read pid");
    format!(
        "v1/apps/sovd-server-{}/data/sovd-server-{}-{}",
        pid, pid, resource
    )
}

// Integration tests
#[tokio::test]
async fn get_component_info() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_component_data() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_component_specific_data() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc/data", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_component_specific_cpu_usage() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc/data/chassis-hpc-cpu", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_component_specific_disk_usage() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc/data/chassis-hpc-disk", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_component_specific_memory_usage() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc/data/chassis-hpc-memory", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_related_apps() {
    run_with_test_server(|addr| async move {
        let url = format!("http://{}/v1/components/chassis-hpc/related-apps", addr);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app() {
    run_with_test_server(|addr| async move {
        let path = format!("v1/apps/sovd-server-{}", get_process_pid("sovd-server").expect("Fail to read pid"));
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app_data() {
    run_with_test_server(|addr| async move {
        let path = format!("v1/apps/sovd-server-{}/data", get_process_pid("sovd-server").expect("Fail to read pid"));
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app_cpu() {
    run_with_test_server(|addr| async move {
        let path = build_app_path_resources("cpu");
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app_memory() {
    run_with_test_server(|addr| async move {
        let path = build_app_path_resources("memory");
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app_disk() {
    run_with_test_server(|addr| async move {
        let path = build_app_path_resources("disk");
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}

#[tokio::test]
async fn get_specific_app_all() {
    run_with_test_server(|addr| async move {
        let path = build_app_path_resources("all");
        let url = format!("http://{}/{}", addr, path);
        let resp = CLIENT.get(&url).send().await.expect("Request failed");
        assert!(resp.status().is_success(), "{} failed", url);
    }).await;
}
