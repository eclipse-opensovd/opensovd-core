// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Authentication and authorization middleware.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use http::request::Parts;
use opensovd_models::{ErrorCode, GenericError};
use tower::{Layer, Service};

/// Authentication/Authorization error with structured JSON response.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Identity not established (401)
    #[error("authentication required")]
    Unauthenticated,
    /// Permission denied - valid identity but insufficient permissions (403)
    #[error("insufficient access rights")]
    Unauthorized,
}

impl AuthError {
    /// Create an unauthorized error.
    #[must_use]
    pub fn unauthorized() -> Self {
        Self::Unauthorized
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            // 401: No response body
            Self::Unauthenticated => StatusCode::UNAUTHORIZED.into_response(),
            // 403: GenericError with insufficient-access-rights
            Self::Unauthorized => {
                let body = GenericError {
                    error_code: ErrorCode::InsufficientAccessRights,
                    vendor_code: None,
                    message: "insufficient access rights".to_owned(),
                    translation_id: None,
                    parameters: None,
                };
                (StatusCode::FORBIDDEN, Json(body)).into_response()
            }
        }
    }
}

/// Extractor for the authenticated identity stored by [`AuthenticationLayer`].
///
/// Unlike Axum's built-in `Extension<T>` (which returns 500 when missing),
/// this returns 401 - the correct semantic for missing authentication.
///
/// # Example
///
/// ```ignore
/// async fn handler(Identity(claims): Identity<MyClaims>) -> impl IntoResponse {
///     // use claims
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Identity<T>(pub T);

impl<T, S> axum::extract::FromRequestParts<S> for Identity<T>
where
    T: Clone + Send + Sync + 'static,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<T>()
            .cloned()
            .map(Identity)
            .ok_or(AuthError::Unauthenticated)
    }
}

impl<T> std::ops::Deref for Identity<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Pluggable authentication backend trait.
///
/// Implement this trait to integrate any authentication mechanism:
/// JWT, mTLS, API keys, `OAuth2`, Biscuit, etc.
pub trait Authenticator: Clone + Send + Sync + 'static {
    /// The identity type extracted from valid credentials.
    type Identity: Clone + Send + Sync + 'static;

    /// Authenticate a request and extract the identity.
    ///
    /// # Returns
    /// * `Ok(Identity)` - Authentication succeeded
    /// * `Err(AuthError::Unauthenticated)` - Authentication failed
    fn authenticate(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<Self::Identity, AuthError>> + Send;
}

/// No-op authenticator that allows all requests.
#[derive(Clone, Debug, Default)]
pub struct NoAuth;

impl Authenticator for NoAuth {
    type Identity = ();

    async fn authenticate(&self, _parts: &Parts) -> Result<Self::Identity, AuthError> {
        Ok(())
    }
}

/// Optional authenticator wrapper - returns None for anonymous requests.
impl<A: Authenticator> Authenticator for Option<A> {
    type Identity = Option<A::Identity>;

    async fn authenticate(&self, parts: &Parts) -> Result<Self::Identity, AuthError> {
        match self {
            Some(authenticator) => authenticator.authenticate(parts).await.map(Some),
            None => Ok(None),
        }
    }
}

// Authentication Middleware Layer
//

/// Tower layer that applies authentication middleware.
#[derive(Clone)]
pub struct AuthenticationLayer<A> {
    authenticator: A,
}

impl<A> AuthenticationLayer<A> {
    /// Create a new authentication layer.
    pub fn new(authenticator: A) -> Self {
        Self { authenticator }
    }
}

impl<S, A: Clone> Layer<S> for AuthenticationLayer<A> {
    type Service = AuthenticationMiddleware<S, A>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthenticationMiddleware {
            inner,
            authenticator: self.authenticator.clone(),
        }
    }
}

/// Authentication middleware service.
#[derive(Clone)]
pub struct AuthenticationMiddleware<S, A> {
    inner: S,
    authenticator: A,
}

impl<S, A> Service<axum::http::Request<Body>> for AuthenticationMiddleware<S, A>
where
    S: Service<axum::http::Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
    A: Authenticator,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<Body>) -> Self::Future {
        let authenticator = self.authenticator.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            match authenticator.authenticate(&parts).await {
                Ok(identity) => {
                    // Store identity in request extensions
                    parts.extensions.insert(identity);

                    // Reconstruct request and continue
                    let req = axum::http::Request::from_parts(parts, body);
                    inner.call(req).await
                }
                Err(auth_error) => {
                    // Return 401 response immediately
                    Ok(auth_error.into_response())
                }
            }
        })
    }
}

// Authorization Middleware Layer
//

/// Pluggable authorization backend trait.
///
/// Implement this trait to integrate any authorization mechanism:
/// RBAC, ABAC, policy engines (OPA, Casbin), custom logic.
///
/// The middleware passes the full request [`Parts`] so the authorizer can
/// inspect the URI path, method, headers, or any other request metadata.
pub trait Authorizer<I>: Clone + Send + Sync + 'static {
    /// Check if identity is authorized for the given request.
    ///
    /// # Arguments
    /// * `identity` - The authenticated identity
    /// * `parts` - The request head (method, URI, headers, extensions, ...)
    ///
    /// # Returns
    /// * `Ok(())` - Authorization granted
    /// * `Err(AuthError::Unauthorized(_))` - Authorization denied
    fn authorize(
        &self,
        identity: &I,
        parts: &Parts,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;
}

/// No-op authorizer that allows all authenticated requests.
#[derive(Clone, Debug, Default)]
pub struct AllowAll;

impl<I: Sync> Authorizer<I> for AllowAll {
    async fn authorize(&self, _: &I, _: &Parts) -> Result<(), AuthError> {
        Ok(())
    }
}

/// Tower layer that applies authorization middleware.
#[derive(Clone)]
pub struct AuthorizationLayer<Z, I> {
    authorizer: Z,
    _identity: PhantomData<I>,
}

impl<Z, I> AuthorizationLayer<Z, I> {
    /// Create a new authorization layer.
    pub fn new(authorizer: Z) -> Self {
        Self {
            authorizer,
            _identity: PhantomData,
        }
    }
}

impl<S, Z: Clone, I> Layer<S> for AuthorizationLayer<Z, I> {
    type Service = AuthorizationMiddleware<S, Z, I>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthorizationMiddleware {
            inner,
            authorizer: self.authorizer.clone(),
            _identity: PhantomData,
        }
    }
}

/// Authorization middleware service.
#[derive(Clone)]
pub struct AuthorizationMiddleware<S, Z, I> {
    inner: S,
    authorizer: Z,
    _identity: PhantomData<I>,
}

impl<S, Z, I> Service<axum::http::Request<Body>> for AuthorizationMiddleware<S, Z, I>
where
    S: Service<axum::http::Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
    Z: Authorizer<I>,
    I: Clone + Send + Sync + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<Body>) -> Self::Future {
        let authorizer = self.authorizer.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (parts, body) = req.into_parts();

            // Get identity from extensions (set by AuthenticationLayer)
            let Some(identity) = parts.extensions.get::<I>() else {
                return Ok(AuthError::Unauthenticated.into_response());
            };

            // Check authorization
            match authorizer.authorize(identity, &parts).await {
                Ok(()) => {
                    let req = axum::http::Request::from_parts(parts, body);
                    inner.call(req).await
                }
                Err(auth_error) => Ok(auth_error.into_response()),
            }
        })
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::string_slice)]
mod tests {
    use axum::{Router, body::Body, http::Request, routing::get};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    /// Test that `NoAuth` authenticator with `AllowAll` policy allows all requests.
    #[tokio::test]
    async fn test_noauth_allows_all() {
        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(AuthorizationLayer::<AllowAll, ()>::new(AllowAll))
            .layer(AuthenticationLayer::new(NoAuth));

        let response = app
            .oneshot(Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[derive(Clone)]
    struct RejectAll;

    impl Authorizer<()> for RejectAll {
        async fn authorize(&self, (): &(), _: &Parts) -> Result<(), AuthError> {
            Err(AuthError::unauthorized())
        }
    }

    /// Test that a denied authorization returns 403 with structured JSON error.
    #[tokio::test]
    async fn test_authorization_denied_returns_403_json() {
        let app = Router::new()
            .route("/test", get(|| async { "unreachable" }))
            .layer(AuthorizationLayer::<RejectAll, ()>::new(RejectAll))
            .layer(AuthenticationLayer::new(NoAuth));

        let response = app
            .oneshot(Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "insufficient-access-rights");
        assert_eq!(json["message"], "insufficient access rights");
    }

    #[derive(Clone)]
    struct ExtractBearer;

    #[derive(Clone, Debug)]
    struct BearerClaims;

    impl Authenticator for ExtractBearer {
        type Identity = BearerClaims;

        async fn authenticate(&self, parts: &Parts) -> Result<BearerClaims, AuthError> {
            parts
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .ok_or(AuthError::Unauthenticated)?;

            Ok(BearerClaims)
        }
    }

    /// Test bearer token extraction using middleware layers.
    #[tokio::test]
    async fn test_bearer_authentication() {
        let app = Router::new()
            .route("/test", get(|| async { "authenticated" }))
            .layer(AuthorizationLayer::<AllowAll, BearerClaims>::new(AllowAll))
            .layer(AuthenticationLayer::new(ExtractBearer));

        // With token - should succeed
        let response = app
            .clone()
            .oneshot(
                Request::get("/test")
                    .header("authorization", "Bearer my-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());

        // Without token - should get 401 with empty body
        let response = app
            .oneshot(Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty(), "401 response must have no body");
    }

    /// Test that `Identity<T>` extractor retrieves the identity from extensions.
    #[tokio::test]
    async fn test_identity_extractor() {
        async fn handler(Identity(claims): Identity<BearerClaims>) -> String {
            format!("{claims:?}")
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(AuthenticationLayer::new(ExtractBearer));

        let response = app
            .oneshot(
                Request::get("/test")
                    .header("authorization", "Bearer my-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    /// Test that `Identity<T>` returns 401 when no identity is in extensions.
    #[tokio::test]
    async fn test_identity_extractor_missing() {
        async fn handler(Identity(_claims): Identity<BearerClaims>) -> &'static str {
            "unreachable"
        }

        // No authentication layer - nothing inserts BearerClaims into extensions.
        let app = Router::new().route("/test", get(handler));

        let response = app
            .oneshot(Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// Test that `Identity<()>` works with `NoAuth`.
    #[tokio::test]
    async fn test_identity_extractor_noauth() {
        async fn handler(Identity(()): Identity<()>) -> &'static str {
            "ok"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(AuthenticationLayer::new(NoAuth));

        let response = app
            .oneshot(Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.status().is_success());
    }
}
