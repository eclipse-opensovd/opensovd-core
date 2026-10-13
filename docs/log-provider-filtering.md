<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0

AI Disclosure: This document was created with the assistance of OpenAI Codex.
Assisted-by: OpenAI Codex
-->

# Log-provider filtering implementation

This document describes how a concrete `LogProvider` implementation should
apply filtering for `GET /apps/{app-id}/logs/entries`.

## Responsibility boundary

The OpenSOVD server parses the HTTP query parameters and passes a `LogFilter`
to the provider:

```text
HTTP request
  -> query parsing
  -> LogFilter
  -> LogProvider::entries(filter)
  -> backend-specific log query
```

Filtering is provider-owned. The provider must apply the filter semantics
defined by this document, while using the native filtering facilities of its
backend where possible. The provider must not rely on the API client to filter
the response.

The common OpenSOVD implementation does not perform a second filtering pass.
This avoids retrieving all log entries unnecessarily and allows providers to
use indexes, backend-native filtering, journald predicates, or equivalent
backend mechanisms.

## API filters

The server maps these query parameters to `LogFilter`:

| Query parameter | Core field | Meaning |
| --- | --- | --- |
| `severity` | `severity` | Return entries at or above this severity |
| `created-after` | `created_after` | Return entries created after this timestamp |
| `created-before` | `created_before` | Return entries created before this timestamp |

`include-schema` controls response schema generation and is not a provider
filter.

The ISO API defines these filters for `GET /{entity-path}/logs/entries` in
ISO 17978-3:2026, section 7.21.3.

## Severity semantics

The supported SOVD severities are:

```text
fatal > error > warn > info > debug
```

For a requested threshold, the provider returns entries whose severity is
equal to or more severe than the threshold:

| Requested severity | Entries returned |
| --- | --- |
| `fatal` | `fatal` |
| `error` | `fatal`, `error` |
| `warn` | `fatal`, `error`, `warn` |
| `info` | `fatal`, `error`, `warn`, `info` |
| `debug` | `fatal`, `error`, `warn`, `info`, `debug` |

Do not rely on Rust's derived enum ordering unless the ordering has been
explicitly defined to match this table. Backend-specific levels must first be
mapped to the SOVD severity model.

The provider should apply the threshold using normalized `LogSeverity` values,
not by comparing backend-specific strings. The mapping from a backend's native
levels to these values belongs entirely to that provider and must not be added
to the common core or model crates.

## Time filtering

Use the log entry timestamp for both time filters.

Recommended comparisons:

```text
created-after  -> entry.timestamp > created_after
created-before -> entry.timestamp < created_before
```

The ISO wording is “after” and “before”, so the boundaries are exclusive.
All timestamps must be compared as instants, after parsing them into a common
timezone representation such as `DateTime<Utc>`.

If both bounds are present and `created-after >= created-before`, the provider
should return `LogError::InvalidRequest` rather than silently returning an
unexpected result.

## Provider implementation sequence

An implementation should follow this sequence:

1. Validate the filter combination.
2. Translate the SOVD severity threshold to the backend’s level representation.
3. Translate the time bounds to the backend’s timestamp query format.
4. Execute the narrowest backend query possible.
5. Map backend records to `opensovd_core::LogEntry`.
6. Normalize backend severity and context values.
7. Apply a final provider-local check after mapping if the backend has
   approximate filtering or inclusive boundary behavior.
8. Return entries in a documented deterministic order, preferably timestamp
   ascending or descending consistently.

The final provider-local check is still part of the provider implementation;
it is not a second filter in the generic OpenSOVD server.

## Rust implementation shape

```rust
#[async_trait]
impl LogProvider for RealtimeUnitLogProvider {
    async fn entries(&self, filter: LogFilter) -> Result<Vec<LogEntry>> {
        if let (Some(after), Some(before)) = (filter.created_after, filter.created_before) {
            if after >= before {
                return Err(LogError::InvalidRequest(
                    "created-after must be earlier than created-before".into(),
                ));
            }
        }

        let query = BackendQuery {
            minimum_level: filter.severity.map(to_backend_level),
            created_after: filter.created_after,
            created_before: filter.created_before,
        };

        let records = self.backend.query(query).await
            .map_err(|error| LogError::Internal(error.to_string()))?;

        records
            .into_iter()
            .map(map_record)
            .filter(|entry| matches_filter(entry, &filter))
            .collect()
    }
}
```

The names of `BackendQuery`, `to_backend_level`, `map_record`, and
`matches_filter` are illustrative. They should be adapted to the realtime
unit’s actual backend, IPC, journald, or file-log interface.

## Context handling

Filtering in this API is not context filtering. ISO specifies that context
selection is handled by the logging configuration rather than by
`GET /logs/entries` query parameters.

The provider should therefore:

- use configured application/context filters when querying the backend;
- return the normalized context in each `LogEntry`;
- preserve backend context fields where available;
- use an `x-sovd-*` context type for provider-specific extensions;
- include `href` when the original log file is available through bulk-data.

## Error behavior

Use these errors for provider-side filtering failures:

- `LogError::InvalidRequest` for inconsistent filter ranges or unsupported
  provider-specific filter input;
- `LogError::Internal` for backend connection, query, or decoding failures;
- `LogError::NotFound` only when the provider’s log resource itself is absent.

The server maps these to HTTP errors. A valid filter that produces no matches
is not an error; it returns:

```json
{
  "items": []
}
```

## Backend integration guidance

A backend provider should translate `LogFilter` into its native filters
instead of fetching the complete log stream and filtering only after transfer.
The provider should remain an adapter. The generic OpenSOVD crates should not
depend directly on backend logging types or transport APIs.

## Implementation checklist

- [ ] Validate `created-after` and `created-before` combinations.
- [ ] Map SOVD severity to backend severity.
- [ ] Apply equal-or-higher severity semantics.
- [ ] Apply exclusive timestamp bounds.
- [ ] Use backend-native filtering where available.
- [ ] Normalize backend records to `LogEntry`.
- [ ] Preserve context information.
- [ ] Return deterministic ordering.
- [ ] Map backend failures to `LogError`.
- [ ] Verify empty-result behavior.
- [ ] Add provider conformance tests before production deployment.
