# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures for tests that run the gateway with mock entities."""

import pytest
from fixtures import default_binary_args


@pytest.fixture(scope="module")
def binary_args(request):
    """Enable mock entities for all tests in this directory."""
    return default_binary_args(request.config, "--mock")
