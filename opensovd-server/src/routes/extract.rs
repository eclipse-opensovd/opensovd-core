// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Extractors that reject with [`Error`], so every rejection answers with a
//! `GenericError` body.

#![expect(clippy::disallowed_types, reason = "wraps axum's extractors")]

use axum::extract::{FromRequest, FromRequestParts};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::error::Error;

/// [`axum::extract::Path`] rejecting with [`Error`].
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(Error))]
pub(crate) struct Path<T>(pub T);

/// [`axum_extra::extract::Query`] rejecting with [`Error`].
#[derive(FromRequestParts)]
#[from_request(via(axum_extra::extract::Query), rejection(Error))]
pub(crate) struct Query<T>(pub T);

/// [`axum_extra::TypedHeader`] rejecting with [`Error`].
#[derive(FromRequestParts)]
#[from_request(via(axum_extra::TypedHeader), rejection(Error))]
pub(crate) struct TypedHeader<T>(pub T);

/// [`axum::Json`] rejecting with [`Error`], and answering like it.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(Error))]
pub(crate) struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
