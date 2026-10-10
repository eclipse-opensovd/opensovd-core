# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Command-line checks for the Rego policy options."""

from opensovd_e2e import spawn_process


def run_rejected(request, crate_binary, args):
    proc = spawn_process(request.config, args, None, crate=crate_binary)
    assert proc.process is not None
    proc.process.wait(timeout=10.0)
    out = proc.stdout
    proc.close()
    assert proc.process.returncode == 2
    return out


def test_policy_requires_jwt_secret(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.delenv("SOVD_JWT_SECRET", raising=False)
    policy = tmp_path / "policy.rego"
    policy.write_text("package sovd.authz\n")
    out = run_rejected(request, crate_binary, ["--auth-policy", str(policy)])
    assert "--auth-jwt-secret" in out


def test_policy_data_requires_policy(request, crate_binary, tmp_path):
    data = tmp_path / "data.json"
    data.write_text("{}")
    out = run_rejected(request, crate_binary, ["--auth-policy-data", str(data)])
    assert "--auth-policy" in out
