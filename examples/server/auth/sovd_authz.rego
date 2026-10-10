# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

# Example SOVD authorization policy for the OpenSOVD gateway.
#
# Grants come from external data (sovd_data.json). Rules match the template of
# the matched route, relative to the mount path, with glob.match:
#   *  = exactly one segment
#   ** = one or more trailing segments, so grant `/x` and `/x/**` for both
# Glob reads braces as alternatives; match a template literally as \{component_id\}.
# Route parameters are compared as values, never matched as paths.

package sovd.authz
import rego.v1

default allow := false

allow if {
    input.method == "GET"
    input.route == "/version-info"
}

allow if {
    some role in input.principal.roles
    some rule in data.role_permissions[role]
    input.method in rule.methods
    some pattern in rule.routes
    glob.match(pattern, ["/"], input.route)
}

# Technicians write data only on the components assigned to them.
allow if {
    input.method == "PUT"
    "technician" in input.principal.roles
    glob.match("/v1/components/*/data/*", ["/"], input.route)
    input.params.component_id in data.assignments[input.principal.subject]
}
