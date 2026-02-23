// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Opaque HTTP body type that avoids leaking `axum::body::Body` in the public API.
//!
//! Axum's `Router` is not generic over the body type - it always dispatches
//! requests as `Request<axum::body::Body>`. To keep axum out of our public API
//! we introduce this newtype and bridge with `.map_request()` /
//! `.map_response()` inside [`crate::ServerBuilder::service`].

use std::pin::Pin;
use std::task::{Context, Poll};

/// An opaque HTTP body type used by [`crate::ServerBuilder::service`].
///
/// `Body` implements [`http_body::Body`] and can be constructed from any
/// compatible body via [`Body::new`].
pub struct Body(axum::body::Body);

impl Body {
    /// Create a `Body` from any compatible [`http_body::Body`].
    pub fn new<B>(body: B) -> Self
    where
        B: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self(axum::body::Body::new(body))
    }

    /// Wrap an existing axum body.
    pub(crate) fn wrap(body: axum::body::Body) -> Self {
        Self(body)
    }
}

impl http_body::Body for Body {
    type Data = bytes::Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        // axum::body::Body is Unpin, so we can project safely.
        Pin::new(&mut self.get_mut().0)
            .poll_frame(cx)
            .map_err(Into::into)
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.0.size_hint()
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }
}

#[cfg(test)]
mod tests {
    use http_body::Body as _;
    use http_body_util::BodyExt;

    use super::*;

    #[tokio::test]
    async fn body_delegates_to_inner() {
        let inner = http_body_util::Full::new(bytes::Bytes::from("hello"));
        let body = Body::new(inner);

        assert!(!body.is_end_stream());

        let collected = BodyExt::collect(body).await.unwrap();
        assert_eq!(collected.to_bytes(), "hello");
    }
}
