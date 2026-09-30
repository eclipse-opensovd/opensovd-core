# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

FROM ghcr.io/jdx/mise:2026.9.18@sha256:67514a972b2bddd0eca59b8c4baf4e106fc4e9f9de11233bc2305414bf672ee3 AS mise

FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04@sha256:d94c97dd9cacf183d0a6fd12a8e87b526e9e928307674ae9c94139139c0c6eae

COPY --from=mise /usr/local/bin/mise /usr/local/bin/mise
