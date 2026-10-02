// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP request tracing middleware.

use std::time::Duration;

use http::{Request, Response};
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::{MakeSpan, OnFailure, OnRequest, OnResponse, TraceLayer};
use tracing::Span;

const SERVER_TARGET: &str = "srv";

#[must_use]
pub fn server_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    ServerMakeSpan,
    ServerOnRequest,
    ServerOnResponse,
    (),
    (),
    ServerOnFailure,
> {
    TraceLayer::new_for_http()
        .make_span_with(ServerMakeSpan)
        .on_request(ServerOnRequest)
        .on_response(ServerOnResponse)
        .on_failure(ServerOnFailure)
        .on_body_chunk(())
        .on_eos(())
}

#[derive(Clone, Copy)]
pub struct ServerMakeSpan;

impl<B> MakeSpan<B> for ServerMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> Span {
        tracing::info_span!(
            target: SERVER_TARGET,
            "http",
            method = %req.method(),
            uri = %req.uri(),
            status = tracing::field::Empty,
            latency_us = tracing::field::Empty,
            error = tracing::field::Empty,
        )
    }
}

#[derive(Clone, Copy)]
pub struct ServerOnRequest;

impl<B> OnRequest<B> for ServerOnRequest {
    fn on_request(&mut self, _req: &Request<B>, _span: &Span) {
        tracing::trace!(target: SERVER_TARGET, "Request started");
    }
}

#[derive(Clone, Copy)]
pub struct ServerOnResponse;

impl<B> OnResponse<B> for ServerOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        span.record("status", response.status().as_u16());
        span.record(
            "latency_us",
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        );
        tracing::info!(target: SERVER_TARGET, "Request finished");
    }
}

#[derive(Clone, Copy)]
pub struct ServerOnFailure;

impl OnFailure<ServerErrorsFailureClass> for ServerOnFailure {
    fn on_failure(&mut self, error: ServerErrorsFailureClass, latency: Duration, span: &Span) {
        span.record(
            "latency_us",
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        );
        if let ServerErrorsFailureClass::StatusCode(status) = error {
            span.record("status", status.as_u16());
        }
        span.record("error", tracing::field::display(error));
        tracing::error!(target: SERVER_TARGET, "Request failed");
    }
}
