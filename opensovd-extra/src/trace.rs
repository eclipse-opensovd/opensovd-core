// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP request tracing middleware.

use tower::Layer;
use tower_http::classify::SharedClassifier;
use tower_http::trace::TraceLayer;

use self::logs::{Classify, Hooks, Logs};

type Trace<L> =
    TraceLayer<SharedClassifier<Classify>, Hooks<L>, Hooks<L>, Hooks<L>, (), (), Hooks<L>>;

fn trace_layer<L: Logs>() -> Trace<L> {
    let hooks = Hooks::default();
    TraceLayer::new(SharedClassifier::new(Classify))
        .make_span_with(hooks)
        .on_request(hooks)
        .on_response(hooks)
        .on_failure(hooks)
        .on_body_chunk(())
        .on_eos(())
}

/// Traces the requests a server handles.
#[must_use]
pub fn server_layer() -> ServerLayer {
    ServerLayer
}

/// The layer [`server_layer`] returns.
#[derive(Clone, Copy, Debug, Default)]
pub struct ServerLayer;

impl<S> Layer<S> for ServerLayer {
    type Service = <Trace<Self> as Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        trace_layer::<Self>().layer(inner)
    }
}

/// Traces the requests a client sends: finished ones at debug, failed ones at
/// error.
#[must_use]
pub fn client_layer() -> ClientLayer {
    ClientLayer
}

/// The layer [`client_layer`] returns.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientLayer;

impl<S> Layer<S> for ClientLayer {
    type Service = <Trace<Self> as Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        trace_layer::<Self>().layer(inner)
    }
}

mod logs {
    use std::fmt::Display;
    use std::marker::PhantomData;
    use std::time::Duration;

    use axum::extract::MatchedPath;
    use http::{HeaderMap, Request, Response};
    use tower_http::classify::{ClassifiedResponse, ClassifyEos, ClassifyResponse};
    use tower_http::trace::{MakeSpan, OnFailure, OnRequest, OnResponse};
    use tracing::Span;
    use tracing::field::Empty;

    use super::{ClientLayer, ServerLayer};

    const SERVER_TARGET: &str = "srv";
    const CLIENT_TARGET: &str = "cli";

    /// The log calls of a layer. Targets and levels are fixed per call, so
    /// each layer has its own.
    pub trait Logs: Copy + Default {
        fn span<B>(req: &Request<B>) -> Span;
        fn started() {}
        fn finished();
        fn failed();
    }

    impl Logs for ServerLayer {
        fn span<B>(req: &Request<B>) -> Span {
            let route = req.extensions().get::<MatchedPath>();
            tracing::info_span!(
                target: SERVER_TARGET,
                "http",
                method = %req.method(),
                route = route.map(|r| tracing::field::display(r.as_str())),
                uri = %req.uri(),
                status = Empty,
                latency_us = Empty,
                error = Empty,
            )
        }

        fn started() {
            tracing::trace!(target: SERVER_TARGET, "Request started");
        }

        fn finished() {
            tracing::info!(target: SERVER_TARGET, "Request finished");
        }

        fn failed() {
            tracing::error!(target: SERVER_TARGET, "Request failed");
        }
    }

    impl Logs for ClientLayer {
        fn span<B>(req: &Request<B>) -> Span {
            tracing::info_span!(
                target: CLIENT_TARGET,
                "http",
                method = %req.method(),
                uri = %req.uri(),
                status = Empty,
                latency_us = Empty,
                error = Empty,
            )
        }

        fn finished() {
            tracing::debug!(target: CLIENT_TARGET, "Request finished");
        }

        fn failed() {
            tracing::error!(target: CLIENT_TARGET, "Request failed");
        }
    }

    /// Why a request failed after `on_response` or instead of it.
    #[derive(Debug)]
    pub enum Failure {
        /// The service returned an error, e.g. a refused connection.
        Service(String),
        /// The response body failed after its headers were logged.
        Body(String),
    }

    /// Checks every response at the end of its body, so that body errors
    /// reach `on_failure`. `on_response` already logs server errors.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Classify;

    impl ClassifyResponse for Classify {
        type FailureClass = Failure;
        type ClassifyEos = Self;

        fn classify_response<B>(self, _res: &Response<B>) -> ClassifiedResponse<Failure, Self> {
            ClassifiedResponse::RequiresEos(self)
        }

        fn classify_error<E: Display + 'static>(self, error: &E) -> Failure {
            Failure::Service(error.to_string())
        }
    }

    impl ClassifyEos for Classify {
        type FailureClass = Failure;

        fn classify_eos(self, _trailers: Option<&HeaderMap>) -> Result<(), Failure> {
            Ok(())
        }

        fn classify_error<E: Display + 'static>(self, error: &E) -> Failure {
            Failure::Body(error.to_string())
        }
    }

    /// The tower-http callbacks, shared by both layers.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Hooks<L>(PhantomData<L>);

    impl<L: Logs, B> MakeSpan<B> for Hooks<L> {
        fn make_span(&mut self, req: &Request<B>) -> Span {
            L::span(req)
        }
    }

    impl<L: Logs, B> OnRequest<B> for Hooks<L> {
        fn on_request(&mut self, _req: &Request<B>, _span: &Span) {
            L::started();
        }
    }

    fn record_latency(span: &Span, latency: Duration) {
        span.record(
            "latency_us",
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        );
    }

    impl<L: Logs, B> OnResponse<B> for Hooks<L> {
        fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
            let status = response.status();
            span.record("status", status.as_u16());
            record_latency(span, latency);
            if status.is_server_error() {
                L::failed();
            } else {
                L::finished();
            }
        }
    }

    /// Logs service and response body errors. A body error keeps the latency
    /// that `on_response` recorded.
    impl<L: Logs> OnFailure<Failure> for Hooks<L> {
        fn on_failure(&mut self, failure: Failure, latency: Duration, span: &Span) {
            match failure {
                Failure::Service(error) => {
                    span.record("error", error);
                    record_latency(span, latency);
                }
                Failure::Body(error) => {
                    span.record("error", error);
                }
            }
            L::failed();
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::convert::Infallible;
    use std::fmt;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::Poll;

    use bytes::Bytes;
    use http::{Request, Response, StatusCode};
    use http_body::{Body, Frame};
    use http_body_util::BodyExt;
    use tower::{Layer, ServiceExt, service_fn};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::subscriber::DefaultGuard;
    use tracing::{Event, Level, Subscriber};
    use tracing_subscriber::layer::{Context, SubscriberExt};

    use super::{client_layer, server_layer};

    type Logged = (Level, &'static str, String);

    /// Records events and span fields while its guard lives.
    #[derive(Clone, Default)]
    struct Capture {
        events: Arc<Mutex<Vec<Logged>>>,
        fields: Arc<Mutex<Vec<(&'static str, String)>>>,
    }

    impl Capture {
        fn install() -> (Self, DefaultGuard) {
            let capture = Self::default();
            let subscriber = tracing_subscriber::registry().with(capture.clone());
            (capture, tracing::subscriber::set_default(subscriber))
        }

        fn events(&self) -> Vec<Logged> {
            self.events.lock().unwrap().clone()
        }

        fn count(&self, name: &str) -> usize {
            let fields = self.fields.lock().unwrap();
            fields.iter().filter(|(field, _)| *field == name).count()
        }

        fn field(&self, name: &str) -> Option<String> {
            let fields = self.fields.lock().unwrap();
            let field = fields.iter().rev().find(|(field, _)| *field == name);
            field.map(|(_, value)| value.clone())
        }
    }

    struct Collect<'a>(&'a mut Vec<(&'static str, String)>);

    impl Visit for Collect<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            self.0.push((field.name(), format!("{value:?}")));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.push((field.name(), value.to_owned()));
        }
    }

    impl<S: Subscriber> tracing_subscriber::Layer<S> for Capture {
        fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
            attrs.record(&mut Collect(&mut self.fields.lock().unwrap()));
        }

        fn on_record(&self, _id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
            values.record(&mut Collect(&mut self.fields.lock().unwrap()));
        }

        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            let mut fields = Vec::new();
            event.record(&mut Collect(&mut fields));
            let message = fields
                .into_iter()
                .find(|(field, _)| *field == "message")
                .map(|(_, value)| value)
                .unwrap_or_default();
            let metadata = event.metadata();
            let logged = (*metadata.level(), metadata.target(), message);
            self.events.lock().unwrap().push(logged);
        }
    }

    fn request() -> Request<String> {
        Request::new(String::new())
    }

    async fn ok(_req: Request<String>) -> Result<Response<String>, Infallible> {
        Ok(Response::new(String::new()))
    }

    async fn internal_error(_req: Request<String>) -> Result<Response<String>, Infallible> {
        let mut response = Response::new(String::new());
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        Ok(response)
    }

    async fn refused(_req: Request<String>) -> Result<Response<String>, &'static str> {
        Err("connection refused")
    }

    /// A body that fails on its first frame.
    struct Broken;

    impl Body for Broken {
        type Data = Bytes;
        type Error = &'static str;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, &'static str>>> {
            Poll::Ready(Some(Err("body broke")))
        }
    }

    async fn broken(_req: Request<String>) -> Result<Response<Broken>, Infallible> {
        Ok(Response::new(Broken))
    }

    fn logged(level: Level, target: &'static str, message: &str) -> Logged {
        (level, target, message.to_owned())
    }

    #[tokio::test]
    async fn server_layer_logs_finished_requests_at_info() {
        let (capture, _guard) = Capture::install();
        let service = server_layer().layer(service_fn(ok));
        let response = service.oneshot(request()).await.unwrap();
        response.into_body().collect().await.unwrap();
        assert_eq!(
            capture.events(),
            [
                logged(Level::TRACE, "srv", "Request started"),
                logged(Level::INFO, "srv", "Request finished"),
            ]
        );
        assert_eq!(capture.field("status").as_deref(), Some("200"));
    }

    #[tokio::test]
    async fn server_layer_logs_body_errors() {
        let (capture, _guard) = Capture::install();
        let service = server_layer().layer(service_fn(broken));
        let response = service.oneshot(request()).await.unwrap();
        assert!(response.into_body().collect().await.is_err());
        assert_eq!(
            capture.events(),
            [
                logged(Level::TRACE, "srv", "Request started"),
                logged(Level::INFO, "srv", "Request finished"),
                logged(Level::ERROR, "srv", "Request failed"),
            ]
        );
        assert_eq!(capture.field("error").as_deref(), Some("body broke"));
        assert_eq!(capture.count("latency_us"), 1);
    }

    #[tokio::test]
    async fn server_layer_logs_server_errors_once() {
        let (capture, _guard) = Capture::install();
        let service = server_layer().layer(service_fn(internal_error));
        service.oneshot(request()).await.unwrap();
        assert_eq!(
            capture.events(),
            [
                logged(Level::TRACE, "srv", "Request started"),
                logged(Level::ERROR, "srv", "Request failed"),
            ]
        );
        assert_eq!(capture.field("status").as_deref(), Some("500"));
    }

    #[tokio::test]
    async fn server_layer_logs_service_errors() {
        let (capture, _guard) = Capture::install();
        let service = server_layer().layer(service_fn(refused));
        assert!(service.oneshot(request()).await.is_err());
        assert_eq!(
            capture.events(),
            [
                logged(Level::TRACE, "srv", "Request started"),
                logged(Level::ERROR, "srv", "Request failed"),
            ]
        );
        assert_eq!(
            capture.field("error").as_deref(),
            Some("connection refused")
        );
        assert!(capture.field("latency_us").is_some());
    }

    #[tokio::test]
    async fn client_layer_logs_finished_requests_at_debug() {
        let (capture, _guard) = Capture::install();
        let service = client_layer().layer(service_fn(ok));
        service.oneshot(request()).await.unwrap();
        assert_eq!(
            capture.events(),
            [logged(Level::DEBUG, "cli", "Request finished")]
        );
        assert_eq!(capture.field("status").as_deref(), Some("200"));
    }

    #[tokio::test]
    async fn client_layer_logs_server_errors() {
        let (capture, _guard) = Capture::install();
        let service = client_layer().layer(service_fn(internal_error));
        service.oneshot(request()).await.unwrap();
        assert_eq!(
            capture.events(),
            [logged(Level::ERROR, "cli", "Request failed")]
        );
    }

    #[tokio::test]
    async fn client_layer_logs_connection_errors() {
        let (capture, _guard) = Capture::install();
        let service = client_layer().layer(service_fn(refused));
        assert!(service.oneshot(request()).await.is_err());
        assert_eq!(
            capture.events(),
            [logged(Level::ERROR, "cli", "Request failed")]
        );
        assert_eq!(
            capture.field("error").as_deref(),
            Some("connection refused")
        );
        assert!(capture.field("latency_us").is_some());
    }
}
