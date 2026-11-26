# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Give OpenSOVD's crate universe its own module extension identity."""

load("@rules_rust//crate_universe:extensions.bzl", _crate = "crate")

crate = _crate
