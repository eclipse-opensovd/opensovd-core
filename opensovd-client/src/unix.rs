// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::Uri;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

/// Address for a Unix domain socket.
#[derive(Debug, Clone)]
enum SocketAddr {
    /// Filesystem path (e.g. `/tmp/opensovd.sock`).
    Path(PathBuf),
    /// Abstract socket name (Linux only, no filesystem entry).
    /// Stored with the leading null byte prefix that tokio expects.
    #[cfg(target_os = "linux")]
    Abstract(Vec<u8>),
}

/// A hyper connector that routes all requests to a Unix domain socket.
///
/// Implements [`tower_service::Service<Uri>`] so it can be used with
/// `hyper_util::client::legacy::Client`.
#[derive(Debug, Clone)]
pub(crate) struct UnixConnector {
    addr: SocketAddr,
}

impl UnixConnector {
    /// Create a connector for a filesystem Unix socket path.
    pub(crate) fn new(path: impl AsRef<Path>) -> Self {
        Self {
            addr: SocketAddr::Path(path.as_ref().to_owned()),
        }
    }

    /// Create a connector for a Linux abstract socket.
    ///
    /// The `name` should be the abstract socket name **without** a leading null
    /// byte - the connector prepends it automatically.
    #[cfg(target_os = "linux")]
    pub(crate) fn abstract_name(name: impl AsRef<[u8]>) -> Self {
        let mut addr = vec![0u8];
        addr.extend_from_slice(name.as_ref());
        Self {
            addr: SocketAddr::Abstract(addr),
        }
    }
}

impl tower_service::Service<Uri> for UnixConnector {
    type Response = TokioIo<UnixStream>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let addr = self.addr.clone();
        Box::pin(async move {
            let stream = match addr {
                SocketAddr::Path(ref path) => UnixStream::connect(path).await?,
                #[cfg(target_os = "linux")]
                SocketAddr::Abstract(ref name) => {
                    // tokio's UnixStream::connect accepts a path starting with
                    // \0 as a Linux abstract socket address.
                    use std::os::unix::ffi::OsStrExt;
                    let path = Path::new(std::ffi::OsStr::from_bytes(name));
                    UnixStream::connect(path).await?
                }
            };
            Ok(TokioIo::new(stream))
        })
    }
}
