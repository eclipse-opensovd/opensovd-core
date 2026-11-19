// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Authentication and authorization middleware.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::extract::rejection::RawPathParamsRejection;
use axum::extract::{
    FromRequestParts, MatchedPath, OptionalFromRequestParts, RawPathParams, Request, State,
};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use http::request::Parts;
use opensovd_models::{ErrorCode, GenericError};
use serde::Serialize;

use crate::routes::extract::Json;

/// An authenticated caller.
#[derive(Clone, Debug, Serialize)]
pub struct Principal {
    pub subject: String,
    pub roles: Vec<String>,
    pub scopes: Vec<String>,
    /// All verified claims.
    pub claims: serde_json::Map<String, serde_json::Value>,
}

/// Establishes who sent a request.
pub trait Authenticator: Send + Sync + 'static {
    /// Returns `Ok(None)` when the request carries no credentials.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] when the credentials are rejected.
    fn authenticate(&self, parts: &Parts) -> Result<Option<Principal>, AuthError>;
}

/// A request as seen by an [`Authorizer`].
#[derive(Debug, Serialize)]
pub struct Access<'a> {
    /// Request method; HEAD is reported as GET, whose handler serves it.
    pub method: &'a str,
    /// Template of the matched route relative to the mount path, such as
    /// `/v1/components/{component_id}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<&'a str>,
    /// Decoded parameters of the matched route.
    pub params: BTreeMap<&'a str, &'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<&'a Principal>,
}

/// Decides whether a request may proceed.
pub trait Authorizer: Send + Sync + 'static {
    fn authorize(&self, access: &Access<'_>) -> bool;
}

/// Authorizer that allows every authenticated caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct Authenticated;

impl Authorizer for Authenticated {
    fn authorize(&self, access: &Access<'_>) -> bool {
        access.principal.is_some()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No credentials, but the request needs them (401).
    #[error("authentication required")]
    Unauthenticated,
    /// Credentials were sent and rejected (401), for the given reason.
    #[error("invalid token: {0}")]
    InvalidToken(Cow<'static, str>),
    /// The caller is known but not allowed (403).
    #[error("insufficient access rights")]
    Forbidden,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let challenge = |value| {
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, HeaderValue::from_static(value))],
            )
                .into_response()
        };
        match self {
            Self::Unauthenticated => challenge("Bearer"),
            Self::InvalidToken(_) => challenge(r#"Bearer error="invalid_token""#),
            Self::Forbidden => {
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

/// An authenticator and an authorizer, checked on every request after routing.
///
/// A denied request gets 401 when no caller was identified and 403 otherwise.
/// The caller's `subject`, or the `reason` its credentials were rejected, is
/// recorded on the current span if the span declares that field.
#[derive(Clone)]
pub struct Auth {
    authenticator: Arc<dyn Authenticator>,
    authorizer: Arc<dyn Authorizer>,
}

impl Auth {
    pub fn new(authenticator: impl Authenticator, authorizer: impl Authorizer) -> Self {
        Self {
            authenticator: Arc::new(authenticator),
            authorizer: Arc::new(authorizer),
        }
    }

    pub(crate) fn apply(self, router: Router, base: Option<&str>) -> Router {
        let base: Arc<str> = base.unwrap_or_default().into();
        router.layer(middleware::from_fn_with_state((self, base), enforce))
    }
}

async fn enforce(
    State((auth, base)): State<(Auth, Arc<str>)>,
    request: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = request.into_parts();
    // Without a matched route there are no params; undecodable ones answer a
    // JSON 400 before the policy runs.
    let raw = match RawPathParams::from_request_parts(&mut parts, &()).await {
        Ok(raw) => Some(raw),
        Err(RawPathParamsRejection::MissingPathParams(_)) => None,
        Err(rejection) => {
            let error = GenericError::new(ErrorCode::IncompleteRequest, rejection.body_text());
            return (rejection.status(), Json(error)).into_response();
        }
    };
    let span = tracing::Span::current();
    let principal = match auth.authenticator.authenticate(&parts) {
        Ok(principal) => principal,
        Err(error) => {
            if let AuthError::InvalidToken(reason) = &error {
                span.record("reason", reason.as_ref());
            }
            return error.into_response();
        }
    };
    if let Some(principal) = &principal {
        span.record("subject", principal.subject.as_str());
    }
    let access = Access {
        method: if parts.method == Method::HEAD {
            Method::GET.as_str()
        } else {
            parts.method.as_str()
        },
        route: parts
            .extensions
            .get::<MatchedPath>()
            .map(|matched| relative(matched.as_str(), &base)),
        params: raw.iter().flat_map(|p| p.iter()).collect(),
        principal: principal.as_ref(),
    };
    match (auth.authorizer.authorize(&access), principal) {
        (true, Some(principal)) => {
            parts.extensions.insert(principal);
        }
        (true, None) => {}
        (false, Some(_)) => return AuthError::Forbidden.into_response(),
        (false, None) => return AuthError::Unauthenticated.into_response(),
    }
    next.run(Request::from_parts(parts, body)).await
}

/// Strips the mount path so policies hold wherever the server is mounted.
fn relative<'a>(route: &'a str, base: &str) -> &'a str {
    route
        .strip_prefix(base)
        .filter(|rest| rest.starts_with('/'))
        .unwrap_or(route)
}

impl<S: Send + Sync> FromRequestParts<S> for Principal {
    type Rejection = AuthError;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<Self>()
                .cloned()
                .ok_or(AuthError::Unauthenticated),
        )
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Principal {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        std::future::ready(Ok(parts.extensions.get::<Self>().cloned()))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::routing::get;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    struct Header;

    impl Authenticator for Header {
        fn authenticate(&self, parts: &Parts) -> Result<Option<Principal>, AuthError> {
            match parts.headers.get("x-user").map(HeaderValue::to_str) {
                None => Ok(None),
                Some(Ok("bad") | Err(_)) => Err(AuthError::InvalidToken("bad user".into())),
                Some(Ok(subject)) => Ok(Some(Principal {
                    subject: subject.to_owned(),
                    roles: Vec::new(),
                    scopes: Vec::new(),
                    claims: serde_json::Map::new(),
                })),
            }
        }
    }

    struct Only(&'static str);

    impl Authorizer for Only {
        fn authorize(&self, access: &Access<'_>) -> bool {
            access.principal.is_some_and(|p| p.subject == self.0)
        }
    }

    fn app(auth: Auth) -> Router {
        let router = Router::new().route(
            "/items/{item_id}",
            get(|principal: Option<Principal>| async move {
                principal.map(|p| p.subject).unwrap_or_default()
            }),
        );
        auth.apply(router, None)
    }

    async fn get_as(app: Router, user: Option<&str>) -> Response {
        let mut request = http::Request::get("/items/1");
        if let Some(user) = user {
            request = request.header("x-user", user);
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn allowed_principal_reaches_the_handler() {
        let response = get_as(app(Auth::new(Header, Only("alice"))), Some("alice")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"alice");
    }

    #[tokio::test]
    async fn denied_principal_is_forbidden() {
        let response = get_as(app(Auth::new(Header, Only("alice"))), Some("bob")).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "insufficient-access-rights");
    }

    #[tokio::test]
    async fn denied_anonymous_caller_is_challenged() {
        let response = get_as(app(Auth::new(Header, Only("alice"))), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn rejected_credentials_are_named_in_the_challenge() {
        let response = get_as(app(Auth::new(Header, Only("alice"))), Some("bad")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            r#"Bearer error="invalid_token""#
        );
    }

    #[tokio::test]
    async fn authenticated_allows_any_principal() {
        let app = app(Auth::new(Header, Authenticated));
        assert_eq!(
            get_as(app.clone(), Some("bob")).await.status(),
            StatusCode::OK
        );
        assert_eq!(get_as(app, None).await.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn anonymous_callers_reach_handlers_the_policy_allows() {
        struct Everyone;
        impl Authorizer for Everyone {
            fn authorize(&self, _: &Access<'_>) -> bool {
                true
            }
        }
        let response = get_as(app(Auth::new(Header, Everyone)), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty(), "anonymous callers have no principal");
    }

    #[tokio::test]
    async fn head_is_authorized_as_get() {
        struct GetOnly;
        impl Authorizer for GetOnly {
            fn authorize(&self, access: &Access<'_>) -> bool {
                access.method == "GET"
            }
        }
        let request = http::Request::head("/items/1").body(Body::empty()).unwrap();
        let response = app(Auth::new(Header, GetOnly))
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn routes_are_relative_to_the_base() {
        assert_eq!(relative("/sovd/v1/components", "/sovd"), "/v1/components");
        assert_eq!(relative("/sovd/v1/components", ""), "/sovd/v1/components");
        assert_eq!(relative("/sovdx/v1", "/sovd"), "/sovdx/v1");
        assert_eq!(relative("/ui/{*rest}", "/sovd"), "/ui/{*rest}");
    }

    #[tokio::test]
    async fn principal_extractor_rejects_without_auth() {
        let app = Router::new().route("/", get(|_: Principal| async {}));
        let response = app
            .oneshot(http::Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
