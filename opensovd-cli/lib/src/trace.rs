// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP request tracing middleware.

use std::time::Duration;

use http::{Request, Response};
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::{MakeSpan, OnFailure, OnRequest, OnResponse, TraceLayer};
use tracing::Span;

const TARGET: &str = "srv";

#[must_use]
pub fn trace_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    HttpMakeSpan,
    HttpOnRequest,
    HttpOnResponse,
    (),
    (),
    HttpOnFailure,
> {
    TraceLayer::new_for_http()
        .make_span_with(HttpMakeSpan)
        .on_request(HttpOnRequest)
        .on_response(HttpOnResponse)
        .on_failure(HttpOnFailure)
        .on_body_chunk(())
        .on_eos(())
}

#[derive(Clone, Copy)]
pub struct HttpMakeSpan;

impl<B> MakeSpan<B> for HttpMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> Span {
        tracing::info_span!(
            target: TARGET,
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
pub struct HttpOnRequest;

impl<B> OnRequest<B> for HttpOnRequest {
    fn on_request(&mut self, _req: &Request<B>, _span: &Span) {
        tracing::trace!(target: TARGET, "Request started");
    }
}

#[derive(Clone, Copy)]
pub struct HttpOnResponse;

impl<B> OnResponse<B> for HttpOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        span.record("status", response.status().as_u16());
        span.record(
            "latency_us",
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        );
        tracing::info!(target: TARGET, "Request finished");
    }
}

#[derive(Clone, Copy)]
pub struct HttpOnFailure;

impl OnFailure<ServerErrorsFailureClass> for HttpOnFailure {
    fn on_failure(&mut self, error: ServerErrorsFailureClass, latency: Duration, span: &Span) {
        span.record(
            "latency_us",
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        );
        if let ServerErrorsFailureClass::StatusCode(status) = error {
            span.record("status", status.as_u16());
        }
        span.record("error", tracing::field::display(error));
        tracing::error!(target: TARGET, "Request failed");
    }
}
