// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Systemd socket activation example.
//!
//! Demonstrates receiving a socket from systemd and signaling readiness.
//!
//! Run with:
//! ```bash
//! cargo build -p opensovd-examples --example systemd
//! systemd-socket-activate -l 8080 target/debug/examples/systemd
//! ```
//!
//! Then test with: `curl http://127.0.0.1:8080/sovd/v1/components`

#[cfg(target_os = "linux")]
mod systemd {
    use std::os::fd::FromRawFd;

    use opensovd_mocks::create_mock_topology;
    use opensovd_server::Server;
    use sd_notify::NotifyState;
    use tokio::net::TcpListener;

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        libcli::init_tracing("info", None)?;

        let fd = sd_notify::listen_fds()?
            .next()
            .ok_or("no socket from systemd, use: systemd-socket-activate -l 8080")?;

        // SAFETY: fd is valid and owned, provided by systemd socket activation
        #[allow(unsafe_code)]
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        let addr = listener.local_addr()?;

        let server = Server::builder()
            .base_uri("http://127.0.0.1:0/sovd")?
            .listener(listener)
            .topology(create_mock_topology().await)
            .layer(libcli::trace::trace_layer())
            .build()?;

        sd_notify::notify(false, &[NotifyState::Ready])?;
        tracing::info!(addr = %addr, "Server running");
        server.serve().await?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    systemd::run().await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This example is only available on Linux");
}
