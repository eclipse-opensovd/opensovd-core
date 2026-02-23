# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

# Example SOVD authorization policy for the OpenSOVD gateway.
#
# Uses role_permissions from external data (sovd_data.json).
# Matches request path against glob patterns via the built-in glob.match:
#   ** = zero or more trailing segments
#   *  = exactly one segment

package sovd.authz
import rego.v1

default allow := false

allow if {
    some role in input.identity.roles
    some rule in data.role_permissions[role]
    input.method in rule.methods
    some pattern in rule.paths
    glob.match(pattern, ["/"], input.path)
}
