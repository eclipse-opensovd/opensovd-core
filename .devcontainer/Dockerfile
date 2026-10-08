# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

FROM ghcr.io/jdx/mise:2026.10.4@sha256:1e0a39bc5a3b089b9b9305b6cd53e5d32ed5327e6241db5ec412fc2eabd44b02 AS mise

FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04@sha256:d94c97dd9cacf183d0a6fd12a8e87b526e9e928307674ae9c94139139c0c6eae

COPY --from=mise /usr/local/bin/mise /usr/local/bin/mise
