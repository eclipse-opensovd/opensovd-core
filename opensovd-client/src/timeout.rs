// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tower::Layer;
use tower::Service;

/// Per-attempt request timeout error (internal).
#[derive(Debug)]
pub(crate) struct TimeoutError(pub Duration);

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "request timed out after {:?}", self.0)
    }
}

impl std::error::Error for TimeoutError {}

/// Tower layer that applies a per-request timeout.
/// If `timeout` is `None`, requests pass through without timeout.
#[derive(Clone, Debug)]
pub(crate) struct RequestTimeoutLayer {
    timeout: Option<Duration>,
}

impl RequestTimeoutLayer {
    pub(crate) fn new(timeout: Option<Duration>) -> Self {
        Self { timeout }
    }
}

impl<S> Layer<S> for RequestTimeoutLayer {
    type Service = RequestTimeout<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestTimeout {
            inner,
            timeout: self.timeout,
        }
    }
}

/// Service that applies a per-request timeout to an inner service.
#[derive(Clone, Debug)]
pub(crate) struct RequestTimeout<S> {
    inner: S,
    timeout: Option<Duration>,
}

impl<S, Req, Res, E> Service<Req> for RequestTimeout<S>
where
    S: Service<Req, Response = Res, Error = E> + Clone + Send + 'static,
    S::Future: Send + 'static,
    Req: Send + 'static,
    Res: 'static,
    E: From<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    type Response = Res;
    type Error = E;
    type Future = Pin<Box<dyn Future<Output = Result<Res, E>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let timeout = self.timeout;

        Box::pin(async move {
            let fut = inner.call(req);
            match timeout {
                Some(d) => tokio::time::timeout(d, fut).await.map_err(|_| {
                    E::from(Box::new(TimeoutError(d)) as Box<dyn std::error::Error + Send + Sync>)
                })?,
                None => fut.await,
            }
        })
    }
}
