# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

FROM ghcr.io/jdx/mise:2026.11.9@sha256:9a182adba2584a6f5b29ddc1f2c7651c93694a5f829d74452b90214616baacf4 AS mise

FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04@sha256:d94c97dd9cacf183d0a6fd12a8e87b526e9e928307674ae9c94139139c0c6eae

COPY --from=mise /usr/local/bin/mise /usr/local/bin/mise
