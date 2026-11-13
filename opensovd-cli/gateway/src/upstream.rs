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
//! thing the proxy changes: in `Location` and in the strings of JSON bodies,
//! they refer to the gateway. Of a JSON body only these strings change; the
//! rest, numbers, whitespace and members, stays as the upstream wrote it.
//!
//! A JSON body has to be read for that, so the upstream may only compress it
//! with a content coding the proxy decodes (RFC 9110 8.4): the client's
//! `Accept-Encoding` reaches the upstream without the others, and a JSON body
//! reaches the client decoded. Other bodies stay as the upstream encoded them.

use std::convert::Infallible;
use std::io::Read;
use std::ops::Range;
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
use opensovd_server::Body;

use crate::cli::{UpstreamConfig, UpstreamUrl};

/// The largest JSON body whose references to the upstream are rewritten, as
/// the upstream sends it and decoded.
const JSON_LIMIT: usize = 64 * 1024 * 1024;

/// The content codings the proxy decodes: those of RFC 9110 8.4.1 but
/// `compress`, and Brotli (RFC 7932).
const DECODED: [&str; 4] = ["gzip", "x-gzip", "deflate", "br"];

/// Headers that apply to a single connection, which a proxy must not pass on.
const HOP_BY_HOP: [HeaderName; 8] = [
    header::CONNECTION,
    HeaderName::from_static("keep-alive"),
    header::PROXY_AUTHENTICATE,
    header::PROXY_AUTHORIZATION,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
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
        let accepted = decodable(&parts.headers);
        parts.headers.insert(header::ACCEPT_ENCODING, accepted);

        match self.client.request(Request::from_parts(parts, body)).await {
            Ok(response) => {
                let (mut parts, body) = response.into_parts();
                strip_hop_by_hop_headers(&mut parts.headers);
                let body = Body::new(body);
                let server = server.as_ref().map(|ServerUri(server)| server.as_str());
                self.point_at(server, parts, body).await
            }
            Err(error) => {
                tracing::warn!(target: "forwarding", error = %error, "Upstream unreachable");
                bad_gateway("The upstream is unreachable")
            }
        }
    }

    /// The response with a JSON body decoded and, for a client of `server`,
    /// with its references to the upstream referring to `server`. Other bodies
    /// stay streamed.
    async fn point_at(&self, server: Option<&str>, mut parts: Parts, body: Body) -> Response<Body> {
        let location = parts
            .headers
            .get(header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .zip(server)
            .and_then(|(location, server)| self.at_server(location, server))
            .and_then(|location| HeaderValue::try_from(location).ok());
        if let Some(location) = location {
            parts.headers.insert(header::LOCATION, location);
        }
        let encoded = parts.headers.contains_key(header::CONTENT_ENCODING);
        if !is_json(&parts.headers) || (server.is_none() && !encoded) {
            return Response::from_parts(parts, body);
        }

        let body = match Limited::new(body, JSON_LIMIT).collect().await {
            Ok(body) => body.to_bytes(),
            Err(error) => {
                tracing::warn!(target: "forwarding", error = %error, "Invalid upstream response");
                return bad_gateway("The upstream sent an invalid response");
            }
        };
        // Decoding and rewriting up to `JSON_LIMIT` takes a while, which the
        // gateway's single thread must not wait for.
        let proxy = self.clone();
        let headers = parts.headers.clone();
        let server = server.map(str::to_owned);
        let length = body.len();
        let readable = tokio::task::spawn_blocking(move || {
            proxy.for_client(&headers, encoded, body, server.as_deref())
        })
        .await;
        let body = match readable {
            Ok(Some(body)) => body,
            Ok(None) => {
                tracing::warn!(target: "forwarding", "Upstream response does not decode");
                return bad_gateway("The upstream sent a response it did not encode as declared");
            }
            Err(error) => {
                tracing::warn!(target: "forwarding", error = %error, "Reading the upstream response failed");
                return bad_gateway("The upstream response could not be read");
            }
        };
        // A body of HEAD or 304 is empty and keeps the headers of the one it
        // stands for.
        if length != 0 {
            parts.headers.remove(header::CONTENT_ENCODING);
            if body.len() != length {
                parts.headers.remove(header::CONTENT_LENGTH);
            }
        }
        Response::from_parts(parts, Body::new(Full::new(body)))
    }

    /// The JSON `body` of a response with `headers`, decoded if it is
    /// `encoded`, and for a client of `server` with its references to the
    /// upstream referring to `server`; nothing if it does not decode.
    fn for_client(
        &self,
        headers: &HeaderMap,
        encoded: bool,
        body: bytes::Bytes,
        server: Option<&str>,
    ) -> Option<bytes::Bytes> {
        // A body of HEAD or 304 is empty whatever its coding says.
        let body = if encoded && !body.is_empty() {
            decode(headers, &body)?
        } else {
            body
        };
        let rewritten = server.and_then(|server| self.json_at_server(&body, server));
        Some(rewritten.unwrap_or(body))
    }

    /// The JSON `body` with what refers to the upstream's API in its strings
    /// referring to `server`, or nothing if there is no such reference. Only
    /// these strings change: they are matched decoded, so that escapes such as
    /// `\/` do not hide a reference, and written anew. Everything else stays as
    /// the upstream wrote it. Object keys are names, no references, and stay. A
    /// body that is no JSON is matched as text.
    fn json_at_server(&self, body: &[u8], server: &str) -> Option<bytes::Bytes> {
        // Checked without building it, so that the strings found below are
        // those of JSON.
        if serde_json::from_slice::<serde::de::IgnoredAny>(body).is_err() {
            let text = std::str::from_utf8(body).ok()?;
            return self.at_server(text, server).map(bytes::Bytes::from);
        }
        let reference = self.reference().as_bytes();
        let mut rewritten = Vec::new();
        let mut copied = 0;
        for (string, key) in json_strings(body) {
            let raw = body.get(string.clone())?;
            // A reference shows as it is, or it is escaped.
            if key
                || !(raw.contains(&b'\\') || matches_ignoring_case(raw, reference).next().is_some())
            {
                continue;
            }
            let Some(text) = serde_json::from_slice::<String>(raw)
                .ok()
                .and_then(|text| self.at_server(&text, server))
            else {
                continue;
            };
            rewritten.extend_from_slice(body.get(copied..string.start)?);
            rewritten.extend(serde_json::to_vec(&text).ok()?);
            copied = string.end;
        }
        if copied == 0 {
            return None;
        }
        rewritten.extend_from_slice(body.get(copied..)?);
        Some(rewritten.into())
    }

    /// What identifies a reference to the upstream's API: its path, or its
    /// host without a path to go by. See [`Self::at_server`].
    fn reference(&self) -> &str {
        if self.base.is_empty() {
            self.origin.authority().map_or("", |a| a.as_str())
        } else {
            self.base.as_str()
        }
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
        let reference = self.reference();
        let server_path = server
            .split_once("://")
            .and_then(|(_, rest)| rest.find('/').and_then(|path| rest.get(path..)))
            .unwrap_or_default();

        let mut rewritten = String::new();
        let mut copied = 0;
        for start in matches_ignoring_case(text.as_bytes(), reference.as_bytes()) {
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
fn matches_ignoring_case<'a>(text: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    text.windows(needle.len().max(1))
        .enumerate()
        .filter(|(_, window)| window.eq_ignore_ascii_case(needle))
        .map(|(offset, _)| offset)
}

/// The strings of the JSON `body`, each as the range from its opening to its
/// closing quote, and whether it is the key of a member. Outside a string, a
/// quote of JSON starts one.
fn json_strings(body: &[u8]) -> Vec<(Range<usize>, bool)> {
    let mut strings = Vec::new();
    let mut bytes = body.iter().enumerate();
    while let Some((start, _)) = bytes.find(|(_, byte)| **byte == b'"') {
        let mut escaped = false;
        let closing = bytes.find(|(_, byte)| {
            let closes = !escaped && **byte == b'"';
            escaped = !escaped && **byte == b'\\';
            closes
        });
        let Some((last, _)) = closing else {
            break;
        };
        let end = last.saturating_add(1);
        let key = body
            .get(end..)
            .unwrap_or_default()
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(&b':');
        strings.push((start..end, key));
    }
    strings
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

/// The client's `Accept-Encoding` without the codings the proxy does not
/// decode, and without `*`, which would allow them; `identity` if nothing
/// is left, as no `Accept-Encoding` would allow any coding (RFC 9110 12.5.3).
fn decodable(headers: &HeaderMap) -> HeaderValue {
    let accepted: Vec<_> = headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|coding| {
            let name = coding.split(';').next().unwrap_or_default().trim();
            name.eq_ignore_ascii_case("identity")
                || DECODED.iter().any(|known| name.eq_ignore_ascii_case(known))
        })
        .collect();
    HeaderValue::try_from(accepted.join(", "))
        .ok()
        .filter(|accepted| !accepted.is_empty())
        .unwrap_or_else(|| HeaderValue::from_static("identity"))
}

/// `body` without the content codings that `headers` say were applied, in
/// the order they were (RFC 9110 8.4), or nothing if one is not
/// [`DECODED`], the body does not decode, or it is larger than [`JSON_LIMIT`].
fn decode(headers: &HeaderMap, body: &[u8]) -> Option<bytes::Bytes> {
    let codings: Vec<_> = headers
        .get_all(header::CONTENT_ENCODING)
        .iter()
        .map(|value| value.to_str().ok())
        .collect::<Option<_>>()?;
    let codings: Vec<_> = codings
        .iter()
        .flat_map(|value| value.split(','))
        .map(|coding| coding.trim().to_ascii_lowercase())
        .filter(|coding| !coding.is_empty())
        .collect();
    let mut decoded = body.to_vec();
    for coding in codings.iter().rev() {
        let encoded = decoded.as_slice();
        let reader: Box<dyn Read + '_> = match coding.as_str() {
            "identity" => continue,
            "gzip" | "x-gzip" => Box::new(flate2::read::MultiGzDecoder::new(encoded)),
            // HTTP's deflate is the zlib format (RFC 9110 8.4.1.2).
            "deflate" => Box::new(flate2::read::ZlibDecoder::new(encoded)),
            "br" => Box::new(brotli_decompressor::Decompressor::new(encoded, 4096)),
            _ => return None,
        };
        let limit = u64::try_from(JSON_LIMIT).unwrap_or(u64::MAX);
        let mut next = Vec::new();
        reader
            .take(limit.saturating_add(1))
            .read_to_end(&mut next)
            .ok()?;
        if next.len() > JSON_LIMIT {
            return None;
        }
        decoded = next;
    }
    Some(decoded.into())
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
    opensovd_extra::federation::vendor_error(
        StatusCode::BAD_GATEWAY,
        "upstream-unreachable",
        message,
    )
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
        accept_encoding: Option<String>,
        body: Bytes,
    }

    /// A CDA that records requests and answers 202 with a `Location`, a
    /// hop-by-hop header and a binary body.
    async fn mock_cda() -> (Uri, Arc<Mutex<Vec<Seen>>>) {
        mock_upstream(|| {
            Response::builder()
                .status(202)
                .header("location", "http://localhost:20002/vehicle/v15/x/1")
                .header("content-type", "application/octet-stream")
                .header("keep-alive", "timeout=5")
                .body(Full::new(Bytes::from_static(b"\x00\xffraw")))
                .unwrap()
        })
        .await
    }

    /// An upstream below `/vehicle/v15` that records requests and gives each
    /// the `answer`.
    async fn mock_upstream(
        answer: impl Fn() -> Response<Full<Bytes>> + Clone + Send + Sync + 'static,
    ) -> (Uri, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        let service = hyper::service::service_fn(move |request: Request<hyper::body::Incoming>| {
            let recorded = Arc::clone(&recorded);
            let answer = answer.clone();
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
                    accept_encoding: header("accept-encoding"),
                    body,
                });
                Ok::<_, Infallible>(answer())
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
            .header("accept-encoding", "gzip, br")
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
        assert_eq!(seen.accept_encoding.as_deref(), Some("gzip, br"));
        assert_eq!(&seen.body[..], b"\x22\x10");
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// `{"a":1}` as Brotli, by `brotli -c`.
    const BROTLI: &[u8] = &[
        0x0f, 0x03, 0x80, 0x7b, 0x22, 0x61, 0x22, 0x3a, 0x31, 0x7d, 0x03,
    ];

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                )
            })
            .collect()
    }

    #[test]
    fn upstreams_may_only_use_codings_the_proxy_decodes() {
        for (client, upstream) in [
            (None, "identity"),
            (Some("zstd"), "identity"),
            (Some("*"), "identity"),
            (Some("gzip, br;q=0.5, zstd, *;q=0.1"), "gzip, br;q=0.5"),
            (Some("identity;q=0, GZIP"), "identity;q=0, GZIP"),
            (Some("deflate , x-gzip"), "deflate, x-gzip"),
        ] {
            let headers = client.map_or_else(HeaderMap::new, |client| {
                headers(&[("accept-encoding", client)])
            });
            assert_eq!(decodable(&headers), upstream, "{client:?}");
        }
    }

    #[test]
    fn decodes_the_codings_in_the_order_they_were_applied() {
        let json = br#"{"a":1}"#;
        let deflate_then_gzip = gzip(&zlib(json));
        for (coding, body) in [
            ("gzip", gzip(json)),
            ("x-gzip", gzip(json)),
            ("deflate", zlib(json)),
            ("br", BROTLI.to_vec()),
            ("deflate, gzip", deflate_then_gzip),
            ("identity", json.to_vec()),
        ] {
            let headers = HeaderMap::from_iter([(
                header::CONTENT_ENCODING,
                HeaderValue::from_static(coding),
            )]);
            let decoded = decode(&headers, &body);
            assert_eq!(decoded.as_deref(), Some(&json[..]), "{coding}");
        }
    }

    #[test]
    fn an_unknown_coding_or_a_broken_body_does_not_decode() {
        let unknown = headers(&[("content-encoding", "zstd")]);
        assert_eq!(decode(&unknown, b"x"), None);
        let gzip = headers(&[("content-encoding", "gzip")]);
        assert_eq!(decode(&gzip, b"not gzip"), None);
    }

    /// The response of the proxy to a GET of a client of the gateway, whose
    /// upstream answers with `answer`, and what the upstream saw.
    async fn through_the_gateway(
        answer: impl Fn() -> Response<Full<Bytes>> + Clone + Send + Sync + 'static,
    ) -> (Response<Body>, Seen) {
        let (url, seen) = mock_upstream(answer).await;
        let proxy = UpstreamProxy::new(&url, Duration::from_secs(1)).unwrap();
        let mut request = request("GET", "/components", b"");
        request
            .extensions_mut()
            .insert(ServerUri("http://gw/sovd/v1".to_owned()));
        let response = proxy.into_service().oneshot(request).await.unwrap();
        let seen = seen.lock().unwrap().first().cloned().unwrap();
        (response, seen)
    }

    #[tokio::test]
    async fn a_compressed_json_body_reaches_the_client_decoded_and_rewritten() {
        let (response, seen) = through_the_gateway(|| {
            let json = br#"{"href":"http://cda/vehicle/v15/components/ecu"}"#;
            Response::builder()
                .header("content-type", "application/json")
                .header("content-encoding", "gzip")
                .header("content-length", gzip(json).len())
                .body(Full::new(Bytes::from(gzip(json))))
                .unwrap()
        })
        .await;

        assert_eq!(seen.accept_encoding.as_deref(), Some("gzip, br"));
        assert_eq!(response.status(), 200);
        assert!(!response.headers().contains_key("content-encoding"));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], br#"{"href":"http://gw/sovd/v1/components/ecu"}"#);
    }

    #[test]
    fn references_in_json_strings_are_matched_decoded() {
        let proxy = proxy("http://cda:20002/vehicle/v15");
        let at_server = |json: &str| {
            proxy
                .json_at_server(json.as_bytes(), "http://gw/sovd/v1")
                .map(|body| String::from_utf8(body.to_vec()).unwrap())
        };

        for (json, expected) in [
            (
                r#"{"href":"http:\/\/cda\/vehicle\/v15\/components\/ecu\/faults"}"#,
                r#"{"href":"http://gw/sovd/v1/components/ecu/faults"}"#,
            ),
            (
                r#"{"href":"http://cda/vehicle/v15/x"}"#,
                r#"{"href":"http://gw/sovd/v1/x"}"#,
            ),
            (
                r#"{"z":1, "/vehicle/v15/x":"/vehicle/v15/x", "a":[true, null]}"#,
                r#"{"z":1, "/vehicle/v15/x":"/sovd/v1/x", "a":[true, null]}"#,
            ),
            (
                r#"{"m":"say \"http://cda/vehicle/v15/x\"", "k" : "/Vehicle/V15/y"}"#,
                r#"{"m":"say \"http://gw/sovd/v1/x\"", "k" : "/sovd/v1/y"}"#,
            ),
            // no JSON, as an upstream may answer anyway
            (
                r#"{"href":"http://cda/vehicle/v15/x""#,
                r#"{"href":"http://gw/sovd/v1/x""#,
            ),
        ] {
            assert_eq!(at_server(json).as_deref(), Some(expected), "{json}");
        }
        // without a reference, the body stays as the upstream wrote it
        assert_eq!(at_server(r#"{ "a" : 1.10, "/vehicle/v15" : 1 }"#), None);
    }

    #[test]
    fn framing_and_connection_headers_stay_with_their_hop() {
        let mut headers = headers(&[
            ("transfer-encoding", "chunked"),
            ("connection", "x-hop"),
            ("x-hop", "1"),
            ("content-type", "application/json"),
        ]);

        strip_hop_by_hop_headers(&mut headers);

        let left: Vec<_> = headers.keys().map(HeaderName::as_str).collect();
        assert_eq!(left, ["content-type"]);
    }

    #[test]
    fn only_the_strings_with_a_reference_change() {
        let proxy = proxy("http://cda:20002/vehicle/v15");
        let json = concat!(
            r#"{ "n" : 1.10, "big":123456789012345678901234567890, "e":1e2,"#,
            r#" "href":"/vehicle/v15/x", "href":"/vehicle/v15/y", "s":"\u00e9" }"#,
        );
        let expected = concat!(
            r#"{ "n" : 1.10, "big":123456789012345678901234567890, "e":1e2,"#,
            r#" "href":"/sovd/v1/x", "href":"/sovd/v1/y", "s":"\u00e9" }"#,
        );

        let rewritten = proxy.json_at_server(json.as_bytes(), "http://gw/sovd/v1");

        assert_eq!(rewritten.as_deref(), Some(expected.as_bytes()));
    }

    #[test]
    fn finds_the_strings_of_json_and_their_keys() {
        let json = br#"{"a" : "x\"y", "b":["c\\"], "d":{"e":"f"}}"#;

        let strings: Vec<_> = json_strings(json)
            .into_iter()
            .map(|(range, key)| {
                (
                    String::from_utf8(json.get(range).unwrap().to_vec()).unwrap(),
                    key,
                )
            })
            .collect();

        let expected = [
            (r#""a""#, true),
            (r#""x\"y""#, false),
            (r#""b""#, true),
            (r#""c\\""#, false),
            (r#""d""#, true),
            (r#""e""#, true),
            (r#""f""#, false),
        ];
        assert_eq!(
            strings,
            expected.map(|(text, key)| (text.to_owned(), key)).to_vec()
        );
    }

    #[tokio::test]
    async fn other_bodies_stay_compressed() {
        let (response, _) = through_the_gateway(|| {
            Response::builder()
                .header("content-type", "application/octet-stream")
                .header("content-encoding", "gzip")
                .body(Full::new(Bytes::from(gzip(b"\x00\xff"))))
                .unwrap()
        })
        .await;

        assert_eq!(response.headers()["content-encoding"], "gzip");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], &gzip(b"\x00\xff")[..]);
    }

    #[tokio::test]
    async fn a_json_body_that_does_not_decode_answers_bad_gateway() {
        let (response, _) = through_the_gateway(|| {
            Response::builder()
                .header("content-type", "application/json")
                .header("content-encoding", "gzip")
                .body(Full::new(Bytes::from_static(b"{}")))
                .unwrap()
        })
        .await;

        assert_eq!(response.status(), 502);
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
