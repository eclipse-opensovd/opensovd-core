# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for JWT authentication and Rego policy authorization.

Tests are parametrized across an RSA and an EC signing key, published to the
gateway as a JSON Web Key Set.
"""

import json
import re
import time
from dataclasses import dataclass
from pathlib import Path

import jwt
import pytest
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from fixtures import default_binary_args

PROJECT_ROOT = Path(__file__).parents[3]
POLICY_FILE = PROJECT_ROOT / "examples" / "server" / "auth" / "sovd_authz.rego"
POLICY_DATA = PROJECT_ROOT / "examples" / "server" / "auth" / "sovd_data.json"


@dataclass(frozen=True)
class SigningKey:
    """A private key and the kid and algorithm of the tokens it signs."""

    kid: str
    alg: str
    private: rsa.RSAPrivateKey | ec.EllipticCurvePrivateKey

    def jwk(self) -> dict:
        """The public key as a JWK."""
        algorithm = jwt.get_algorithm_by_name(self.alg)
        jwk = algorithm.to_jwk(self.private.public_key(), as_dict=True)
        return {**jwk, "kid": self.kid, "alg": self.alg}


KEYS = [
    SigningKey("rsa", "RS512", rsa.generate_private_key(public_exponent=65537, key_size=2048)),
    SigningKey("ec", "ES256", ec.generate_private_key(ec.SECP256R1())),
]


def make_token(key, *, sub="testuser", roles=(), exp=None, headers=None) -> str:
    """Create a token signed by `key`, naming it with `kid` unless `headers` is given."""
    if exp is None:
        exp = int(time.time()) + 3600
    payload = {"sub": sub, "roles": list(roles), "exp": exp, "iss": "OpenSOVD"}
    if headers is None:
        headers = {"kid": key.kid}
    return jwt.encode(payload, key.private, algorithm=key.alg, headers=headers)


@pytest.fixture(scope="module", params=KEYS, ids=lambda key: key.alg)
def key(request) -> SigningKey:
    """The key that signs the tokens of a test."""
    return request.param


@pytest.fixture(scope="module")
def binary_args(request, tmp_path_factory):
    """Start gateway with a JWKS of all keys, the example Rego policy and mock entities."""
    jwks = tmp_path_factory.mktemp("auth") / "jwks.json"
    jwks.write_text(json.dumps({"keys": [key.jwk() for key in KEYS]}))
    return default_binary_args(
        request.config,
        "--mock",
        "--auth-jwks",
        str(jwks),
        "--auth-issuer",
        "OpenSOVD",
        "--auth-policy",
        str(POLICY_FILE),
        "--auth-policy-data",
        str(POLICY_DATA),
    )


def test_missing_token_returns_401(client):
    """Request without Authorization header should get 401 with a Bearer challenge."""
    response = client.get("/v1/components")
    assert response.status_code == 401
    assert response.headers["WWW-Authenticate"] == "Bearer"


def test_invalid_token_returns_401(client):
    """Request with malformed token should get 401 naming the invalid token."""
    response = client.get("/v1/components", headers={"Authorization": "Bearer not-a-valid-jwt"})
    assert response.status_code == 401
    assert response.headers["WWW-Authenticate"] == 'Bearer error="invalid_token"'


def test_token_without_kid_returns_401(client, key):
    """A token must name its signing key."""
    token = make_token(key, roles=["reader"], headers={})
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 401
    assert response.headers["WWW-Authenticate"] == 'Bearer error="invalid_token"'


def test_token_with_unknown_kid_returns_401(client, key):
    """A token naming a key outside the set is rejected."""
    token = make_token(key, roles=["reader"], headers={"kid": "unknown"})
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 401
    assert response.headers["WWW-Authenticate"] == 'Bearer error="invalid_token"'


def test_expired_token_returns_401(client, key):
    """Request with expired token should get 401."""
    token = make_token(key, roles=["reader"], exp=1)
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 401


def test_version_info_is_public(client):
    """The example policy lets anonymous callers read the version."""
    response = client.get("/version-info")
    assert response.status_code == 200


def test_subject_is_logged_with_the_request(client, gateway, key):
    """The finished request names the authenticated subject."""
    token = make_token(key, sub=f"logged-{key.kid}", roles=["reader"])
    client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    gateway.wait_for(re.compile(rf'Request finished .*subject="logged-{key.kid}"'))


def test_rejection_reason_is_logged_with_the_request(client, gateway, key):
    """The finished request says why its token was rejected."""
    token = make_token(key, roles=["reader"], headers={"kid": f"stale-{key.kid}"})
    client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    gateway.wait_for(re.compile(rf'Request finished .*reason="unknown kid stale-{key.kid}"'))


def test_reader_get_allowed(client, key):
    """Reader role can GET components."""
    token = make_token(key, roles=["reader"])
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200
    data = response.json()
    assert "items" in data


def test_reader_head_allowed(client, key):
    """Reader role can HEAD what it can GET."""
    token = make_token(key, roles=["reader"])
    response = client.http.head("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200


def test_reader_put_denied(client, key):
    """Reader role cannot PUT data."""
    token = make_token(key, roles=["reader"])
    response = client.put(
        "/v1/components/ecu/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    assert response.status_code == 403
    data = response.json()
    assert data["error_code"] == "insufficient-access-rights"


def test_admin_get_allowed(client, key):
    """Admin role can GET anything."""
    token = make_token(key, roles=["admin"])
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200


def test_admin_put_allowed(client, key):
    """Admin role can PUT data (gets past authz, may 404 on mock)."""
    token = make_token(key, roles=["admin"])
    response = client.put(
        "/v1/components/ecu/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    # Admin is authorized -- endpoint may reject the body (422) or not exist (404)
    assert response.status_code != 403, "admin should not be denied by policy"
    assert response.status_code != 401, "admin should not be unauthenticated"


def test_reader_get_root_allowed(client, key):
    """Reader role can GET the API root."""
    token = make_token(key, roles=["reader"])
    response = client.get("/v1", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 200


def test_technician_put_on_assigned_component_allowed(client, key):
    """Technician can PUT data on an assigned component (gets past authz, may 404 on mock)."""
    token = make_token(key, sub="technician@example.com", roles=["technician"])
    response = client.put(
        "/v1/components/ecu/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    assert response.status_code not in (401, 403)


def test_technician_put_on_other_component_denied(client, key):
    """Technician cannot PUT data on a component not assigned to them."""
    token = make_token(key, sub="technician@example.com", roles=["technician"])
    response = client.put(
        "/v1/components/gateway/data/speed",
        headers={"Authorization": f"Bearer {token}"},
        json={"value": 42},
    )
    assert response.status_code == 403


def test_no_roles_denied(client, key):
    """Token with no roles is denied."""
    token = make_token(key)
    response = client.get("/v1/components", headers={"Authorization": f"Bearer {token}"})
    assert response.status_code == 403
