// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// SOVD error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// The Component receiving the request has answered with an error.
    ErrorResponse,
    /// The request does not provide all information required to complete the method.
    IncompleteRequest,
    /// The SOVD client does not have the right to access the resource.
    InsufficientAccessRights,
    /// The response provided by the Component contains information which could not be processed.
    InvalidResponseContent,
    /// The signature of the data in the payload is invalid.
    InvalidSignature,
    /// The lock of the client has been broken by another client.
    LockBroken,
    /// The Component which handles the request has been queried but did not respond.
    NotResponding,
    /// The preconditions to execute the method are not fulfilled.
    PreconditionNotFulfilled,
    /// The SOVD server is able to answer requests, but an internal error occurred.
    SovdServerFailure,
    /// The SOVD server is not configured correctly.
    SovdServerMisconfigured,
    /// Another update is currently being executed in automated mode.
    UpdateAutomatedNotSupported,
    /// Another update is currently executed and not yet done or aborted.
    UpdateExecutionInProgress,
    /// An update is already in preparation and not yet done or aborted.
    UpdatePreparationInProgress,
    /// An update is already in progress and not yet done or aborted.
    UpdateProcessInProgress,
    /// Details are specified in the `vendor_code`.
    VendorSpecific,
}

/// Generic error type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct GenericError {
    pub error_code: ErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_code: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

impl GenericError {
    /// Creates a new error with the given code and message.
    pub fn new(error_code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            error_code,
            vendor_code: None,
            message: message.into(),
            translation_id: None,
            parameters: None,
        }
    }

    /// Creates a vendor-specific error with the given vendor code and message.
    pub fn with_vendor_code(vendor_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error_code: ErrorCode::VendorSpecific,
            vendor_code: Some(vendor_code.into()),
            message: message.into(),
            translation_id: None,
            parameters: None,
        }
    }
}
