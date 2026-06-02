// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use opensovd_models::version::{SovdInfo, VersionInfo};
use serde::de::DeserializeOwned;
use tokio::sync::OnceCell;

#[cfg(unix)]
use crate::client::BuilderError;
use crate::client::Client;
use crate::error::{Error, Result};

/// Client for the unversioned SOVD `/version-info` endpoint.
///
/// Built via [`ClientBuilder::discovery`](crate::ClientBuilder::discovery); select a version
/// to reach a [`Client`]. Cheap to clone; clones share one cached `/version-info` fetch.
#[derive(Clone)]
pub struct Discovery {
    /// Rooted client carrying the transport (connector and layers).
    pub(crate) inner: Client,
    /// Cached `/version-info` body, shared across clones. Vendor metadata kept as raw
    /// `Value` so one cache serves any concrete `V`.
    pub(crate) cache: Arc<OnceCell<VersionInfo<serde_json::Value>>>,
}

impl Discovery {
    /// List the advertised SOVD instances.
    ///
    /// The first call fetches `/version-info`; later calls reuse the cached response.
    /// `V` is the `vendor_info` payload type
    /// ([`VendorInfo`](opensovd_models::version::VendorInfo) for the default shape).
    #[allow(clippy::result_large_err)]
    pub async fn versions<V: DeserializeOwned>(&self) -> Result<Vec<SovdInfo<V>>> {
        let info = self
            .cache
            .get_or_try_init(|| {
                self.inner
                    .get::<VersionInfo<serde_json::Value>>("/version-info", &[])
            })
            .await?;
        // Re-type vendor_info from the cached raw Value into the requested V.
        info.sovd_info
            .iter()
            .map(|s| {
                Ok(SovdInfo {
                    version: s.version.clone(),
                    base_uri: s.base_uri.clone(),
                    vendor_info: s
                        .vendor_info
                        .clone()
                        .map(serde_json::from_value)
                        .transpose()?,
                })
            })
            .collect()
    }

    /// Select the first advertised instance matching `pred` and return a ready, versioned
    /// [`Client`] that reuses this transport.
    ///
    /// Match on `version`, or on typed `vendor_info` by choosing the `V` payload type
    /// (use `serde_json::Value` when matching on `version` only).
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoMatchingVersion`] if no advertised instance matches.
    pub async fn select<V: DeserializeOwned>(
        &self,
        mut pred: impl FnMut(&SovdInfo<V>) -> bool,
    ) -> Result<Client> {
        let advertised = self
            .versions::<V>()
            .await?
            .into_iter()
            .find(|s| pred(s))
            .ok_or(Error::NoMatchingVersion)?
            .base_uri
            .0;
        Ok(Client {
            base_uri: advertised.parse()?,
            http: self.inner.http.clone(),
        })
    }
}

#[cfg(unix)]
impl Discovery {
    /// Build a [`Discovery`] reached over a Unix domain socket (filesystem path).
    ///
    /// `uri` is the unversioned discovery root, e.g. `http://localhost/sovd`; its host is
    /// ignored and all requests are routed to the socket at `path`. Mirrors
    /// [`Client::connect_unix`](crate::Client::connect_unix), but targets `/version-info`.
    ///
    /// # Errors
    ///
    /// Returns [`BuilderError::InvalidUri`] if the URI is invalid.
    pub fn connect_unix(
        uri: &str,
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<Self, BuilderError> {
        let connector = crate::unix::UnixConnector::new(path);
        Client::builder()
            .base_uri(uri)?
            .connector(connector)
            .discovery()
    }

    /// Build a [`Discovery`] reached over a Linux abstract Unix socket.
    ///
    /// `name` is the abstract socket name (without a leading null byte); `uri` is the
    /// unversioned discovery root, e.g. `http://localhost/sovd`.
    ///
    /// # Errors
    ///
    /// Returns [`BuilderError::InvalidUri`] if the URI is invalid.
    #[cfg(target_os = "linux")]
    pub fn connect_unix_abstract(
        uri: &str,
        name: impl AsRef<[u8]>,
    ) -> std::result::Result<Self, BuilderError> {
        let connector = crate::unix::UnixConnector::abstract_name(name);
        Client::builder()
            .base_uri(uri)?
            .connector(connector)
            .discovery()
    }
}
