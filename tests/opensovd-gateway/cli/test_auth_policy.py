# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Command-line checks for the auth options."""

import json

import jwt
from cryptography.hazmat.primitives.asymmetric import ec
from opensovd_e2e import spawn_process


def run_rejected(request, crate_binary, args, code=2):
    proc = spawn_process(request.config, args, None, crate=crate_binary)
    assert proc.process is not None
    proc.process.wait(timeout=10.0)
    out = proc.stdout
    proc.close()
    assert proc.process.returncode == code
    return out


def test_private_key_is_refused(request, crate_binary, tmp_path):
    private = ec.generate_private_key(ec.SECP256R1())
    jwk = jwt.get_algorithm_by_name("ES256").to_jwk(private, as_dict=True)
    jwks = tmp_path / "jwks.json"
    jwks.write_text(json.dumps({"keys": [{**jwk, "kid": "ec"}]}))
    args = ["--auth-jwks", str(jwks), "--auth-issuer", "OpenSOVD"]
    out = run_rejected(request, crate_binary, args, code=1)
    assert "private key ec" in out


def test_policy_requires_jwks(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_JWKS", raising=False)
    policy = tmp_path / "policy.rego"
    policy.write_text("package sovd.authz\n")
    out = run_rejected(request, crate_binary, ["--auth-policy", str(policy)])
    assert "--auth-jwks" in out


def test_jwks_requires_issuer(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_ISSUER", raising=False)
    jwks = tmp_path / "jwks.json"
    jwks.write_text('{"keys": []}')
    out = run_rejected(request, crate_binary, ["--auth-jwks", str(jwks)])
    assert "--auth-issuer" in out


def test_issuer_requires_jwks(request, crate_binary, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_JWKS", raising=False)
    out = run_rejected(request, crate_binary, ["--auth-issuer", "issuer"])
    assert "--auth-jwks" in out


def test_audience_requires_jwks(request, crate_binary, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_JWKS", raising=False)
    out = run_rejected(request, crate_binary, ["--auth-audience", "sovd"])
    assert "--auth-jwks" in out


def test_issuer_must_not_be_empty(request, crate_binary):
    out = run_rejected(request, crate_binary, ["--auth-issuer", ""])
    assert "--auth-issuer" in out


def test_policy_env_requires_jwks(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_JWKS", raising=False)
    policy = tmp_path / "policy.rego"
    policy.write_text("package sovd.authz\n")
    monkeypatch.setenv("SOVD_AUTH_POLICY", str(policy))
    out = run_rejected(request, crate_binary, [])
    assert "--auth-jwks" in out


def test_policy_data_requires_policy(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.delenv("SOVD_AUTH_POLICY", raising=False)
    data = tmp_path / "data.json"
    data.write_text("{}")
    out = run_rejected(request, crate_binary, ["--auth-policy-data", str(data)])
    assert "--auth-policy" in out
