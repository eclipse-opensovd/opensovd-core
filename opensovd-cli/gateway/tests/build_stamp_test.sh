#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

stamp=0123456789abcdef0123456789abcdef01234567
setting=//opensovd-cli/gateway:build_stamp
gateway=//:opensovd-gateway

default_version=$(bazel run --"${setting}"=//opensovd-cli/gateway:empty_build_stamp "$gateway" -- --version)
if [[ "$default_version" == *"$stamp"* ]]; then
    echo "Default build unexpectedly contains the test stamp" >&2
    exit 1
fi

stamped_version=$(bazel run --"${setting}"=//opensovd-cli/gateway:test_build_stamp "$gateway" -- --version)
if [[ "$stamped_version" != *"$stamp"* || "$stamped_version" != *"2024-01-01"* ]]; then
    echo "Gateway version does not contain the supplied revision and date: $stamped_version" >&2
    exit 1
fi
