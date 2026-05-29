# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Reusable pytest plugin and helpers for OpenSOVD end-to-end binary tests."""

from opensovd_e2e.process import ProcessUnderTest, spawn_process

__all__ = [
    "ProcessUnderTest",
    "spawn_process",
]
