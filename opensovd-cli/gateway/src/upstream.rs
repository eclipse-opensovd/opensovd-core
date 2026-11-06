// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Proxy to an upstream SOVD server, such as a classic diagnostic adapter (CDA).
//!
//! The server hands the proxy what it does not serve, with the URI relative
//! to its API version. The proxy sends the request to the upstream below the
//! configured base URL and returns the upstream's response: status, headers and
//! body, streamed. Hop-by-hop headers are dropped, as for any proxy.
//!
//! The upstream refers to its own API with absolute URLs, which a client may not
//! reach and which would bypass the gateway's auth, and with its path in error
//! messages, which means nothing to a client. These references are the one
//! thing the proxy changes: in `Location` and in the text of JSON bodies, they
//! refer to the gateway.

use std::convert::Infallible;
use std::time::Duration;

use anyhow::Context;
use http::header::{self, HeaderMap, HeaderName};
use http::response::Parts;
use http::{HeaderValue, Request, Response, StatusCode, Uri};
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use opensovd_extra::{ForwardRoute, ServerUri, Upstream};
use opensovd_models::error::GenericError;
use opensovd_server::Body;

use crate::cli::{UpstreamConfig, UpstreamUrl};

/// The largest JSON body whose references to the upstream are rewritten.
const JSON_LIMIT: usize = 64 * 1024 * 1024;

/// Headers that apply to a single connection, which a proxy must not pass on.
const HOP_BY_HOP: [HeaderName; 7] = [
    header::CONNECTION,
    HeaderName::from_static("keep-alive"),
    header::PROXY_AUTHENTICATE,
    header::PROXY_AUTHORIZATION,
    header::TE,
    header::TRAILER,
    header::UPGRADE,
];

/// The upstreams of `config`. All serve their entities and take part in vehicle
/// locks. Only the first takes what has a single answer, but for authorize,
/// which stays with the gateway unless `config` names its upstream.
///
/// # Errors
///
/// Returns an error if a URL has no host.
pub fn configured(config: &UpstreamConfig) -> anyhow::Result<Vec<Upstream>> {
    config
        .upstreams
        .iter()
        .enumerate()
        .map(|(index, upstream)| {
            let UpstreamUrl { url, authorize } = upstream;
            let routes = routes(index == 0, *authorize);
            tracing::debug!(target: "forwarding", ?routes, "{url}");
            let proxy = UpstreamProxy::new(url, config.connect_timeout)?;
            Ok(Upstream::new(proxy.into_service())
                .routes(routes)
                .discovery_timeout(config.discovery_timeout))
        })
        .collect()
}

/// The routes of an upstream: the `first` takes what has a single answer, and
/// `authorize` says whether it takes `/authorize`.
fn routes(first: bool, authorize: bool) -> Vec<ForwardRoute> {
    let mut routes = vec![ForwardRoute::Entities, ForwardRoute::Locks];
    if first {
        routes.extend([
            ForwardRoute::Functions,
            ForwardRoute::Collections,
            ForwardRoute::CustomCollections,
        ]);
    }
    if authorize {
        routes.push(ForwardRoute::Authorize);
    }
    routes
}

#[derive(Clone)]
pub struct UpstreamProxy {
    /// Scheme and authority of the upstream.
    origin: Uri,
    /// Path of the upstream's API, without a trailing `/`, such as `/vehicle/v15`.
    base: String,
    /// Pooled HTTP client used to forward requests to the upstream.
    client: Client<HttpConnector, Body>,
}

impl UpstreamProxy {
    /// A proxy to the upstream API at `url`, such as `http://cda:20002/vehicle/v15`.
    ///
    /// # Errors
    ///
    /// Returns an error if `url` has no host.
    pub fn new(url: &Uri, connect_timeout: Duration) -> anyhow::Result<Self> {
        let authority = url
            .authority()
            .with_context(|| format!("no host in {url}"))?;
        let origin = Uri::builder()
            .scheme("http")
            .authority(authority.clone())
            .path_and_query("/")
            .build()?;

        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(connect_timeout));
        connector.set_nodelay(true);
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Ok(Self {
            origin,
            base: url.path().trim_end_matches('/').to_owned(),
            client,
        })
    }

    /// The proxy as the service the server forwards to.
    pub fn into_service(
        self,
    ) -> impl tower::Service<
        Request<Body>,
        Response = Response<Body>,
        Error = Infallible,
        Future = impl Future<Output = Result<Response<Body>, Infallible>> + Send,
    > + Clone
    + Send
    + Sync
    + 'static {
        tower::service_fn(move |request: Request<Body>| {
            let proxy = self.clone();
            async move { Ok(proxy.call(request).await) }
        })
    }

    /// The upstream URI of a request whose URI is relative to the API version. A
    /// trailing slash makes no difference, as the upstream's routes have none.
    fn target(&self, uri: &Uri) -> anyhow::Result<Uri> {
        let path = format!("{}{}", self.base, uri.path().trim_end_matches('/'));
        let path = if path.is_empty() { "/" } else { &path };
        let query = uri.query().map_or_else(String::new, |q| format!("?{q}"));
        let mut parts = self.origin.clone().into_parts();
        parts.path_and_query = Some(
            format!("{path}{query}")
                .parse()
                .context("invalid upstream path")?,
        );
        Uri::from_parts(parts).context("invalid upstream URI")
    }

    async fn call(&self, request: Request<Body>) -> Response<Body> {
        let (mut parts, body) = request.into_parts();
        let target = match self.target(&parts.uri) {
            Ok(target) => target,
            Err(error) => return bad_gateway(&format!("{error:#}")),
        };
        let server = parts.extensions.get::<ServerUri>().cloned();
        tracing::debug!(target: "forwarding", "{} {target}", parts.method);
        parts.uri = target;
        strip_hop_by_hop_headers(&mut parts.headers);
        // the client sets the upstream's authority
        parts.headers.remove(header::HOST);

        match self.client.request(Request::from_parts(parts, body)).await {
            Ok(response) => {
                let (mut parts, body) = response.into_parts();
                strip_hop_by_hop_headers(&mut parts.headers);
                let body = Body::new(body);
                match server {
                    Some(ServerUri(server)) => self.point_at(&server, parts, body).await,
                    None => Response::from_parts(parts, body),
                }
            }
            Err(error) => {
                tracing::warn!(target: "forwarding", error = %error, "Upstream unreachable");
                bad_gateway("The upstream is unreachable")
            }
        }
    }

    /// The response with its references to the upstream referring to `server`.
    /// Bodies other than JSON stay streamed.
    async fn point_at(&self, server: &str, mut parts: Parts, body: Body) -> Response<Body> {
        let location = parts
            .headers
            .get(header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .and_then(|location| self.at_server(location, server))
            .and_then(|location| HeaderValue::try_from(location).ok());
        if let Some(location) = location {
            parts.headers.insert(header::LOCATION, location);
        }
        if !is_json(&parts.headers) {
            return Response::from_parts(parts, body);
        }

        let body = match Limited::new(body, JSON_LIMIT).collect().await {
            Ok(body) => body.to_bytes(),
            Err(error) => {
                tracing::warn!(target: "forwarding", error = %error, "Invalid upstream response");
                return bad_gateway("The upstream sent an invalid response");
            }
        };
        let rewritten = std::str::from_utf8(&body)
            .ok()
            .and_then(|text| self.at_server(text, server));
        let body = match rewritten {
            Some(rewritten) => {
                parts.headers.remove(header::CONTENT_LENGTH);
                bytes::Bytes::from(rewritten)
            }
            None => body,
        };
        Response::from_parts(parts, Body::new(Full::new(body)))
    }

    /// `text` with what refers to the upstream's API referring to `server`, or
    /// nothing if there is no such reference: URLs of the API become URLs of
    /// `server`, and its path alone, as in error messages, the path of
    /// `server`.
    ///
    /// The API path identifies the URLs, not the host: an upstream need not build
    /// them from the URL it is reached by. It is matched ignoring case, as a
    /// CDA also spells it `/Vehicle/v15`. Without a path to go by, only URLs
    /// of the configured host match.
    fn at_server(&self, text: &str, server: &str) -> Option<String> {
        let authority = self.origin.authority().map_or("", |a| a.as_str());
        let reference = if self.base.is_empty() {
            authority
        } else {
            self.base.as_str()
        };
        let server_path = server
            .split_once("://")
            .and_then(|(_, rest)| rest.find('/').and_then(|path| rest.get(path..)))
            .unwrap_or_default();

        let mut rewritten = String::new();
        let mut copied = 0;
        for start in matches_ignoring_case(text, reference) {
            let end = start.saturating_add(reference.len());
            let (Some(before), Some(after)) = (text.get(..start), text.get(end..)) else {
                continue;
            };
            if start < copied || after.starts_with(is_path_char) {
                continue;
            }
            let (from, target) = match url_start(before) {
                Some(url) => (url, server),
                None if self.base.is_empty() => continue,
                None if before.ends_with(|c| is_path_char(c) || c == '/') => continue,
                None => (start, server_path),
            };
            rewritten.push_str(text.get(copied..from)?);
            rewritten.push_str(target);
            copied = end;
        }
        if copied == 0 {
            return None;
        }
        rewritten.push_str(text.get(copied..)?);
        Some(rewritten)
    }
}

/// The offsets of `needle`, an ASCII string, in `text`, ignoring ASCII case.
fn matches_ignoring_case<'a>(text: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.as_bytes()
        .windows(needle.len().max(1))
        .enumerate()
        .filter(|(_, window)| window.eq_ignore_ascii_case(needle.as_bytes()))
        .map(|(offset, _)| offset)
}

/// Whether `c` continues a path segment, so that `/vehicle/v150` is not
/// `/vehicle/v15`.
fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Where the `http` URL starts whose scheme and host `before` ends with.
fn url_start(before: &str) -> Option<usize> {
    let (scheme, host) = before.rsplit_once("://")?;
    if !host
        .chars()
        .all(|c| is_path_char(c) || matches!(c, '.' | ':' | '[' | ']'))
    {
        return None;
    }
    ["https", "http"]
        .iter()
        .find_map(|name| scheme.strip_suffix(name))
        .map(str::len)
}

/// Whether `headers` announce a JSON body.
fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|content_type| content_type.to_str().ok())
        .and_then(|content_type| content_type.split(';').next())
        .is_some_and(|mime| mime.trim() == "application/json" || mime.trim().ends_with("+json"))
}

/// Drops the hop-by-hop headers and those the `Connection` header names.
fn strip_hop_by_hop_headers(headers: &mut HeaderMap) {
    let named: Vec<HeaderName> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::try_from(name.trim()).ok())
        .collect();
    for name in HOP_BY_HOP.iter().chain(&named) {
        headers.remove(name);
    }
}

/// The answer when the upstream cannot be reached: the only one the proxy makes
/// itself, as a SOVD `GenericError`.
fn bad_gateway(message: &str) -> Response<Body> {
    let error = GenericError::with_vendor_code("upstream-unreachable", message);
    let body = serde_json::to_vec(&error).unwrap_or_default();
    let mut response = Response::new(Body::new(Full::new(bytes::Bytes::from(body))));
    *response.status_mut() = StatusCode::BAD_GATEWAY;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bytes::Bytes;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    use super::*;

    fn proxy(url: &str) -> UpstreamProxy {
        UpstreamProxy::new(&url.parse().unwrap(), Duration::from_secs(1)).unwrap()
    }

    #[test]
    fn references_to_the_cda_refer_to_the_gateway() {
        let proxy = proxy("http://cda:20002/vehicle/v15");
        let at_server = |text| proxy.at_server(text, "http://gw:7690/sovd/v1");

        for (text, expected) in [
            (
                "http://localhost:20002/Vehicle/v15/components/flxc1000",
                "http://gw:7690/sovd/v1/components/flxc1000",
            ),
            ("https://cda/vehicle/v15", "http://gw:7690/sovd/v1"),
            (
                "http://[::1]:20002/vehicle/v15/x?a=1",
                "http://gw:7690/sovd/v1/x?a=1",
            ),
            (
                r#"{"a":"http://cda/vehicle/v15/x","b":["http://cda/Vehicle/v15/y"]}"#,
                r#"{"a":"http://gw:7690/sovd/v1/x","b":["http://gw:7690/sovd/v1/y"]}"#,
            ),
            (
                r#"{"message":"Resource not found: /vehicle/v15/x-vendor"}"#,
                r#"{"message":"Resource not found: /sovd/v1/x-vendor"}"#,
            ),
            (
                "Resource not found: /vehicle/v15.",
                "Resource not found: /sovd/v1.",
            ),
            ("ä /vehicle/v15 ü", "ä /sovd/v1 ü"),
        ] {
            assert_eq!(at_server(text).as_deref(), Some(expected), "{text}");
        }
    }

    #[test]
    fn other_text_stays_as_it_is() {
        let proxy = proxy("http://cda:20002/vehicle/v15");

        for text in [
            "",
            "http://example.org/other",
            "http://cda/vehicle/v150/x",
            "/vehicle/v15-old",
            "/vehicle/v15_old",
            "http://cda/prefix/vehicle/v15/x",
            "/prefix/vehicle/v15",
            "not a link",
        ] {
            assert_eq!(proxy.at_server(text, "http://gw/sovd/v1"), None, "{text}");
        }
    }

    #[test]
    fn without_a_path_only_urls_of_the_host_are_references() {
        let proxy = proxy("http://cda:20002");
        let server = "http://gw/sovd/v1";

        assert_eq!(
            proxy.at_server("http://cda:20002/x", server).as_deref(),
            Some("http://gw/sovd/v1/x")
        );
        assert_eq!(proxy.at_server("http://example.org/x", server), None);
        assert_eq!(proxy.at_server("cda:20002 is down", server), None);
    }

    #[derive(Debug, Clone)]
    struct Seen {
        method: http::Method,
        uri: String,
        host: Option<String>,
        connection: bool,
        authorization: Option<String>,
        body: Bytes,
    }

    /// A CDA that records requests and answers 202 with a `Location`, a
    /// hop-by-hop header and a binary body.
    async fn mock_cda() -> (Uri, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        let service = hyper::service::service_fn(move |request: Request<hyper::body::Incoming>| {
            let recorded = Arc::clone(&recorded);
            async move {
                let (parts, body) = request.into_parts();
                let body = body.collect().await.unwrap().to_bytes();
                let header = |name: &str| {
                    parts
                        .headers
                        .get(name)
                        .map(|v| v.to_str().unwrap().to_owned())
                };
                recorded.lock().unwrap().push(Seen {
                    method: parts.method.clone(),
                    uri: parts.uri.to_string(),
                    host: header("host"),
                    connection: parts.headers.contains_key("x-conn-only"),
                    authorization: header("authorization"),
                    body,
                });
                Ok::<_, Infallible>(
                    Response::builder()
                        .status(202)
                        .header("location", "http://localhost:20002/vehicle/v15/x/1")
                        .header("content-type", "application/octet-stream")
                        .header("keep-alive", "timeout=5")
                        .body(Full::new(Bytes::from_static(b"\x00\xffraw")))
                        .unwrap(),
                )
            }
        });
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let service = service.clone();
                tokio::spawn(async move {
                    hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                        .await
                        .ok();
                });
            }
        });
        (format!("http://{addr}/vehicle/v15/").parse().unwrap(), seen)
    }

    fn request(method: &str, uri: &str, body: &'static [u8]) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "gateway:7690")
            .header("authorization", "Bearer cda-token")
            .header("connection", "x-conn-only")
            .header("x-conn-only", "1")
            .body(Body::new(Full::new(Bytes::from_static(body))))
            .unwrap()
    }

    #[tokio::test]
    async fn forwards_below_the_base_and_returns_the_response() {
        let (url, seen) = mock_cda().await;
        let proxy = UpstreamProxy::new(&url, Duration::from_secs(1)).unwrap();

        let response = proxy
            .into_service()
            .oneshot(request(
                "PUT",
                "/components/flxc1000/genericservice?x=1",
                b"\x22\x10",
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), 202);
        assert_eq!(
            response.headers()["location"],
            "http://localhost:20002/vehicle/v15/x/1"
        );
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
        assert!(!response.headers().contains_key("keep-alive"));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"\x00\xffraw");

        let seen = seen.lock().unwrap().clone();
        let seen = seen.first().unwrap();
        assert_eq!(seen.method, http::Method::PUT);
        assert_eq!(
            seen.uri,
            "/vehicle/v15/components/flxc1000/genericservice?x=1"
        );
        assert_ne!(seen.host.as_deref(), Some("gateway:7690"));
        assert!(!seen.connection);
        assert_eq!(seen.authorization.as_deref(), Some("Bearer cda-token"));
        assert_eq!(&seen.body[..], b"\x22\x10");
    }

    #[tokio::test]
    async fn an_unreachable_cda_answers_bad_gateway() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url: Uri = format!("http://{addr}/vehicle/v15").parse().unwrap();
        let proxy = UpstreamProxy::new(&url, Duration::from_secs(1)).unwrap();

        let response = proxy
            .into_service()
            .oneshot(request("GET", "/locks", b""))
            .await
            .unwrap();

        assert_eq!(response.status(), 502);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["vendor_code"], "upstream-unreachable");
    }

    #[test]
    fn all_upstreams_take_locks() {
        assert!(routes(true, false).contains(&ForwardRoute::Locks));
        assert_eq!(
            routes(false, false),
            [ForwardRoute::Entities, ForwardRoute::Locks]
        );
    }

    #[test]
    fn authorize_stays_with_the_gateway_unless_an_upstream_is_named() {
        assert!(!routes(true, false).contains(&ForwardRoute::Authorize));
        assert!(routes(false, true).contains(&ForwardRoute::Authorize));
    }

    #[test]
    fn the_target_joins_the_base_and_the_relative_uri() {
        let url: Uri = "http://cda:20002/vehicle/v15/".parse().unwrap();
        let proxy = UpstreamProxy::new(&url, Duration::from_secs(1)).unwrap();
        for (relative, target) in [
            (
                "/components/flxc1000/faults",
                "http://cda:20002/vehicle/v15/components/flxc1000/faults",
            ),
            ("/locks?x=1", "http://cda:20002/vehicle/v15/locks?x=1"),
            ("/", "http://cda:20002/vehicle/v15"),
            ("/?x=1", "http://cda:20002/vehicle/v15?x=1"),
            ("/components/", "http://cda:20002/vehicle/v15/components"),
        ] {
            let relative: Uri = relative.parse().unwrap();
            assert_eq!(
                proxy.target(&relative).unwrap(),
                target.parse::<Uri>().unwrap()
            );
        }

        let url: Uri = "http://cda:20002".parse().unwrap();
        let proxy = UpstreamProxy::new(&url, Duration::from_secs(1)).unwrap();
        let root = proxy.target(&"/".parse().unwrap()).unwrap();
        assert_eq!(root, "http://cda:20002/".parse::<Uri>().unwrap());
    }
}
