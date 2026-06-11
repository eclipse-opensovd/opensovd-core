<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Offline Capability (Current Phase)

This page documents the narrowed scope of offline capability support in the current phase of implementation.

## In Scope

The following endpoints are included in this phase:

- `GET /v1/components/{component_id}/data/docs` - Returns scoped OpenAPI 3.1.0 documentation for data operations on a component
- `GET /v1/apps/{app_id}/data/docs` - Returns scoped OpenAPI 3.1.0 documentation for data operations on an app

Each endpoint returns a minimal OpenAPI payload describing the list and read operations available for the entity's data collection.

## Exclusions

The following are explicitly **not** included in this phase:

- Server-side evaluation of `x-sovd-applicability` in data payloads
- Global `/openapi.json` endpoint
- Full offline artifact generation or caching
- Client-side applicability matching logic
- Artifact serialization or distribution

These features are deferred to future phases.

## Validation

Integration tests verify:

- Successful HTTP 200 responses with valid OpenAPI 3.1.0 payloads
- HTTP 404 responses when entity does not exist
- OpenAPI schema validity using `openapi_spec_validator`
