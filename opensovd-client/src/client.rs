// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use http_body::Body;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper_util::{
    client::legacy::{self, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tower::{
    Layer, Service,
    layer::util::{Identity, Stack},
    util::{BoxCloneSyncService, MapErrLayer, MapResponseLayer},
};

use crate::entities::{App, Area, Component};
use crate::error::{Error, Result};
use crate::list::ListEntitiesRequest;

/// Boxed error type for HTTP service flexibility with layers.
pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Boxed body type for HTTP responses.
pub(crate) type BoxResponseBody = BoxBody<Bytes, BoxError>;

/// Type-erased HTTP service used by the client.
pub(crate) type HttpService =
    BoxCloneSyncService<http::Request<Full<Bytes>>, http::Response<BoxResponseBody>, BoxError>;

/// Error returned when building a client fails.
#[derive(Debug, Error)]
pub enum BuilderError {
    /// Base URI was not provided.
    #[error("base URI is required")]
    NoBaseUri,
    /// Invalid base URI.
    #[error("invalid base URI")]
    InvalidUri,
}

/// Builder for constructing a [`Client`] with custom configuration.
#[must_use]
pub struct ClientBuilder<Conn = HttpConnector, Layers = Identity> {
    base_uri: Option<http::Uri>,
    connector: Conn,
    layer: Layers,
}

impl ClientBuilder<HttpConnector, Identity> {
    fn new() -> Self {
        Self {
            base_uri: None,
            connector: HttpConnector::new(),
            layer: Identity::new(),
        }
    }
}

impl<Conn, Layers> ClientBuilder<Conn, Layers> {
    /// Set the base URI for the SOVD server.
    ///
    /// The URI should include the SOVD version prefix,
    /// e.g. `http://localhost:7690/sovd/v1`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is malformed.
    pub fn base_uri(
        mut self,
        uri: impl TryInto<http::Uri>,
    ) -> std::result::Result<Self, BuilderError> {
        let uri: http::Uri = uri.try_into().map_err(|_| BuilderError::InvalidUri)?;
        self.base_uri = Some(uri);
        Ok(self)
    }

    /// Set a custom connector for the HTTP transport.
    pub fn connector<NewConn>(self, connector: NewConn) -> ClientBuilder<NewConn, Layers> {
        ClientBuilder {
            base_uri: self.base_uri,
            connector,
            layer: self.layer,
        }
    }

    /// Add a Tower layer to the HTTP client stack.
    ///
    /// Layers are applied in order: the first layer added is the outermost.
    pub fn layer<NewLayer>(self, layer: NewLayer) -> ClientBuilder<Conn, Stack<NewLayer, Layers>> {
        ClientBuilder {
            base_uri: self.base_uri,
            connector: self.connector,
            layer: Stack::new(layer, self.layer),
        }
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns an error if the base URI has not been set.
    pub fn build<ResBody>(self) -> std::result::Result<Client, BuilderError>
    where
        Conn: hyper_util::client::legacy::connect::Connect + Clone + Send + Sync + 'static,
        Layers: Layer<legacy::Client<Conn, Full<Bytes>>> + Clone + Send + Sync + 'static,
        Layers::Service: Service<http::Request<Full<Bytes>>, Response = http::Response<ResBody>>
            + Clone
            + Send
            + Sync
            + 'static,
        <Layers::Service as Service<http::Request<Full<Bytes>>>>::Future: Send,
        <Layers::Service as Service<http::Request<Full<Bytes>>>>::Error: Into<BoxError>,
        ResBody: Body<Data = Bytes> + Send + Sync + 'static,
        ResBody::Error: Into<BoxError>,
    {
        let base_uri = self.base_uri.ok_or(BuilderError::NoBaseUri)?;
        let http = legacy::Client::builder(TokioExecutor::new()).build(self.connector);
        let service = self.layer.layer(http);
        // Map response body to BoxBody and service error to BoxError
        let service = MapErrLayer::new(Into::<BoxError>::into).layer(
            MapResponseLayer::new(|resp: http::Response<ResBody>| {
                resp.map(|body| body.map_err(Into::into).boxed())
            })
            .layer(service),
        );
        Ok(Client {
            base_uri,
            http: BoxCloneSyncService::new(service),
        })
    }
}

/// SOVD REST client with a type-erased HTTP transport.
#[derive(Clone)]
pub struct Client {
    pub(crate) base_uri: http::Uri,
    pub(crate) http: HttpService,
}

impl Client {
    /// Create a new client builder.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Connect to an SOVD server over TCP.
    ///
    /// The `uri` should include the SOVD version prefix,
    /// e.g. `http://localhost:7690/sovd/v1`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is invalid.
    pub fn connect(uri: &str) -> std::result::Result<Self, BuilderError> {
        Self::builder().base_uri(uri)?.build()
    }

    /// Returns a request builder for listing components.
    #[must_use]
    pub fn list_components(&self) -> ListEntitiesRequest<'_> {
        ListEntitiesRequest {
            client: self,
            path: "/components".into(),
            schema: false,
        }
    }

    /// Returns a request builder for listing apps.
    #[must_use]
    pub fn list_apps(&self) -> ListEntitiesRequest<'_> {
        ListEntitiesRequest {
            client: self,
            path: "/apps".into(),
            schema: false,
        }
    }

    /// Returns a request builder for listing areas.
    #[must_use]
    pub fn list_areas(&self) -> ListEntitiesRequest<'_> {
        ListEntitiesRequest {
            client: self,
            path: "/areas".into(),
            schema: false,
        }
    }

    /// Returns a reference to a specific component by ID.
    #[must_use]
    pub fn component(&self, id: &str) -> Component<'_> {
        Component {
            client: self,
            id: encode(id),
        }
    }

    /// Returns a reference to a specific app by ID.
    #[must_use]
    pub fn app(&self, id: &str) -> App<'_> {
        App {
            client: self,
            id: encode(id),
        }
    }

    /// Returns a reference to a specific area by ID.
    #[must_use]
    pub fn area(&self, id: &str) -> Area<'_> {
        Area {
            client: self,
            id: encode(id),
        }
    }

    /// GET a JSON resource at `path` (relative to the base URI) with optional query parameters.
    pub async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let uri = build_uri_with_query(&self.base_uri, path, query)?;
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(&uri)
            .body(Full::new(Bytes::new()))?;
        let bytes = self.request(req).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// PUT a JSON body at `path` (relative to the base URI) with optional query parameters.
    pub async fn put(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: &impl Serialize,
    ) -> Result<()> {
        let uri = build_uri_with_query(&self.base_uri, path, query)?;
        let json = serde_json::to_vec(body)?;
        let req = http::Request::builder()
            .method(http::Method::PUT)
            .uri(&uri)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(json)))?;
        self.request(req).await?;
        Ok(())
    }

    /// Send an HTTP request and return the response body bytes.
    ///
    /// Returns an [`Error::ApiError`] if the server responds with a non-success status.
    pub(crate) async fn request(&self, req: http::Request<Full<Bytes>>) -> Result<Bytes> {
        let resp = self
            .http
            .clone()
            .call(req)
            .await
            .map_err(|e| Error::Service { source: e })?;
        let status = resp.status();
        let body = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| Error::Service { source: e })?
            .to_bytes();
        if !status.is_success() {
            let error = serde_json::from_slice(&body).ok();
            return Err(Error::ApiError { status, error });
        }
        Ok(body)
    }
}

#[cfg(unix)]
impl Client {
    /// Connect to an SOVD server over a Unix domain socket (filesystem path).
    ///
    /// `uri` is the full URI including the path prefix,
    /// e.g. `http://localhost/sovd/v1`. The host is ignored;
    /// all requests are routed to the socket at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is invalid.
    pub fn connect_unix(
        uri: &str,
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<Self, BuilderError> {
        let connector = crate::unix::UnixConnector::new(path);
        Self::builder().base_uri(uri)?.connector(connector).build()
    }

    /// Connect to an SOVD server over a Linux abstract Unix socket.
    ///
    /// `name` is the abstract socket name (without a leading null byte).
    /// `uri` is the full URI including the path prefix,
    /// e.g. `http://localhost/sovd/v1`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is invalid.
    #[cfg(target_os = "linux")]
    pub fn connect_unix_abstract(
        uri: &str,
        name: impl AsRef<[u8]>,
    ) -> std::result::Result<Self, BuilderError> {
        let connector = crate::unix::UnixConnector::abstract_name(name);
        Self::builder().base_uri(uri)?.connector(connector).build()
    }
}

/// Return query pairs for the `include-schema` parameter.
pub(crate) fn schema_query(include: bool) -> &'static [(&'static str, &'static str)] {
    if include {
        &[("include-schema", "true")]
    } else {
        &[]
    }
}

/// Percent-encode a path segment.
pub(crate) fn encode(segment: &str) -> String {
    percent_encoding::utf8_percent_encode(segment, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Build a URI by appending a path and optional query parameters to a base URI.
#[allow(clippy::result_large_err)]
pub(crate) fn build_uri_with_query(
    base_uri: &http::Uri,
    path: &str,
    query: &[(&str, &str)],
) -> Result<http::Uri> {
    let base = format!("{base_uri}{path}");
    Ok(build_uri_query_string(&base, query).parse()?)
}

/// Append query parameters to an already-formed URI string.
pub(crate) fn build_uri_query_string(base: &str, query: &[(&str, &str)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }
    let qs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{base}?{}", qs.join("&"))
}
