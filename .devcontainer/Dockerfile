# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

ARG BAZELISK_VERSION=v1.29.0
ARG BAZELISK_SHA256_AMD64=5a408715e932c0250d28bd84555f12edbf70117de42f9181691c736eacc4a992
ARG BAZELISK_SHA256_ARM64=e20e8b0f4f240091b7a55bf17b9398bd4f40ee70ae0208dff95dd4c445fb4010
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
    amd64) sha256="$BAZELISK_SHA256_AMD64" ;; \
    arm64) sha256="$BAZELISK_SHA256_ARM64" ;; \
    *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /usr/local/bin/bazel \
    "https://github.com/bazelbuild/bazelisk/releases/download/${BAZELISK_VERSION}/bazelisk-linux-${arch}"; \
    echo "${sha256}  /usr/local/bin/bazel" | sha256sum -c -; \
    chmod 0755 /usr/local/bin/bazel

COPY --chown=vscode:vscode .bazelversion /tmp/.bazelversion
USER vscode
RUN USE_BAZEL_VERSION="$(cat /tmp/.bazelversion)" bazel version && rm /tmp/.bazelversion
USER root
