# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for JWT authentication and Rego policy authorization.

Tests are parametrized across HS512 (symmetric) and RS512 (asymmetric) algorithms.
"""

import base64
import os
import time
from pathlib import Path

import jwt
import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from fixtures import default_gateway_args

PROJECT_ROOT = Path(__file__).parent.parent.parent
POLICY_FILE = PROJECT_ROOT / "examples" / "server" / "auth" / "sovd_authz.rego"
POLICY_DATA = PROJECT_ROOT / "examples" / "server" / "auth" / "sovd_data.json"

# -- Key material -------------------------------------------------------------

HMAC_SECRET = os.urandom(64)
HMAC_SECRET_B64 = base64.b64encode(HMAC_SECRET).decode()

# Generate RSA key pair once at module import
_rsa_private_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
_rsa_public_der = _rsa_private_key.public_key().public_bytes(
    serialization.Encoding.DER, serialization.PublicFormat.PKCS1
)
RSA_PUBKEY_B64 = base64.b64encode(_rsa_public_der).decode()

ALGORITHMS = [
    pytest.param(("HS512", HMAC_SECRET_B64, HMAC_SECRET, "HS512"), id="HS512"),
    pytest.param(("RS512", RSA_PUBKEY_B64, _rsa_private_key, "RS512"), id="RS512"),
]


def make_token(signing_key, algorithm, sub="testuser", roles=None, exp=None):
    """Create a signed JWT token."""
    if roles is None:
        roles = []
    if exp is None:
        exp = int(time.time()) + 3600
    payload = {"sub": sub, "roles": roles, "exp": exp, "iss": "OpenSOVD"}
    return jwt.encode(payload, signing_key, algorithm=algorithm)


# -- Fixtures -----------------------------------------------------------------


@pytest.fixture(scope="module", params=ALGORITHMS)
def auth_config(request):
    """Yield (algo_name, gateway_key_b64, signing_key, jwt_algo_str) per algorithm."""
    return request.param


@pytest.fixture(scope="module")
def gateway_args(request, auth_config):
    """Start gateway with JWT auth + Rego policy + mock entities."""
    algo, key_b64, _, _ = auth_config
    return default_gateway_args(
        request.config,
        "--mock",
        "--auth-jwt-algo",
        algo,
        "--auth-jwt-secret",
        key_b64,
        "--auth-policy",
        str(POLICY_FILE),
        "--auth-policy-data",
        str(POLICY_DATA),
    )


# -- Authentication Tests -----------------------------------------------------


def test_missing_token_returns_401(gateway):
    """Request without Authorization header should get 401."""
    response = gateway.get("/v1/components")
    assert response.status_code == 401


def test_invalid_token_returns_401(gateway):
    """Request with malformed token should get 401."""
    response = gateway.get("/v1/components", headers={"Authorization": "Bearer not-a-valid-jwt"})
    assert response.status_code == 401


def test_expired_token_returns_401(gateway, auth_config):
    """Request with expired token should get 401."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo, roles=["reader"], exp=1)
    response = gateway.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 401


# -- Authorization Tests ------------------------------------------------------


def test_reader_get_allowed(gateway, auth_config):
    """Reader role can GET components."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo, roles=["reader"])
    response = gateway.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200
    data = response.json()
    assert "items" in data


def test_reader_put_denied(gateway, auth_config):
    """Reader role cannot PUT data."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo, roles=["reader"])
    response = gateway.put(
        "/v1/components/ecu/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    assert response.status_code == 403
    data = response.json()
    assert data["error_code"] == "insufficient-access-rights"


def test_admin_get_allowed(gateway, auth_config):
    """Admin role can GET anything."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo, roles=["admin"])
    response = gateway.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200


def test_admin_put_allowed(gateway, auth_config):
    """Admin role can PUT data (gets past authz, may 404 on mock)."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo, roles=["admin"])
    response = gateway.put(
        "/v1/components/ecu/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    # Admin is authorized -- endpoint may reject the body (422) or not exist (404)
    assert response.status_code != 403, "admin should not be denied by policy"
    assert response.status_code != 401, "admin should not be unauthenticated"


def test_no_roles_denied(gateway, auth_config):
    """Token with no roles is denied."""
    _, _, signing_key, algo = auth_config
    token = make_token(signing_key, algo)
    response = gateway.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 403
