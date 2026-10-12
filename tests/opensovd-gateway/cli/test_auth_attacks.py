# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Crafted tokens the gateway must reject.

The gateway trusts an RSA and an EC key, requires the audience `sovd` and
allows every valid token, so a 200 means a token got through.
"""

import base64
import hashlib
import hmac
import json
import socket
import threading
import time

import jwt
import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from fixtures import default_binary_args

RSA = rsa.generate_private_key(public_exponent=65537, key_size=2048)
EC = ec.generate_private_key(ec.SECP256R1())
ATTACKER = ec.generate_private_key(ec.SECP256R1())


def b64u(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def public_jwk(private, alg: str, kid: str) -> dict:
    jwk = jwt.get_algorithm_by_name(alg).to_jwk(private.public_key(), as_dict=True)
    return {**jwk, "kid": kid}


def claims(**extra) -> dict:
    """Valid claims, changed by `extra`; a `None` value drops the claim."""
    base = {
        "sub": "user",
        "iss": "OpenSOVD",
        "aud": "sovd",
        "exp": int(time.time()) + 3600,
        **extra,
    }
    return {name: value for name, value in base.items() if value is not None}


def sign(private, alg: str, kid: str, headers=None, **extra) -> str:
    return jwt.encode(
        claims(**extra), private, algorithm=alg, headers={"kid": kid, **(headers or {})}
    )


def unsigned(header: dict) -> str:
    return f"{b64u(json.dumps(header).encode())}.{b64u(json.dumps(claims()).encode())}."


def status(client, token: str) -> int:
    return client.get("/v1/components", headers={"Authorization": f"Bearer {token}"}).status_code


@pytest.fixture(scope="module")
def binary_args(request, tmp_path_factory):
    """Start the gateway with both keys, the audience `sovd` and no policy."""
    jwks = tmp_path_factory.mktemp("attacks") / "jwks.json"
    jwks.write_text(
        json.dumps({"keys": [public_jwk(RSA, "RS256", "rsa"), public_jwk(EC, "ES256", "ec")]})
    )
    return default_binary_args(
        request.config,
        "--auth-jwks",
        str(jwks),
        "--auth-issuer",
        "OpenSOVD",
        "--auth-audience",
        "sovd",
    )


@pytest.fixture
def sink():
    """A listener that counts connections, to see whether the gateway fetches a key URL."""
    server = socket.create_server(("127.0.0.1", 0))
    hits = []

    def accept():
        while True:
            try:
                conn, _ = server.accept()
            except OSError:
                return
            hits.append(conn)
            conn.close()

    threading.Thread(target=accept, daemon=True).start()
    yield f"http://127.0.0.1:{server.getsockname()[1]}/jwks.json", hits
    server.close()


@pytest.mark.parametrize(
    ("private", "alg", "kid"), [(RSA, "RS256", "rsa"), (RSA, "PS512", "rsa"), (EC, "ES256", "ec")]
)
def test_valid_token_is_accepted(client, private, alg, kid):
    """A properly signed token gets through, so the rejections below are meaningful."""
    assert status(client, sign(private, alg, kid)) == 200


@pytest.mark.parametrize("alg", ["none", "None", "NONE"])
def test_unsigned_token_is_rejected(client, alg):
    """A token without a signature is never accepted."""
    assert status(client, unsigned({"alg": alg, "kid": "rsa"})) == 401


@pytest.mark.parametrize(
    "secret",
    [
        RSA.public_key().public_bytes(
            serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo
        ),
        RSA.public_key().public_bytes(
            serialization.Encoding.DER, serialization.PublicFormat.SubjectPublicKeyInfo
        ),
        json.dumps(public_jwk(RSA, "RS256", "rsa")).encode(),
    ],
    ids=["pem", "der", "jwk"],
)
def test_hmac_with_the_public_key_as_secret_is_rejected(client, secret):
    """The public RSA key cannot be used as an HMAC secret."""
    header = b64u(json.dumps({"alg": "HS256", "kid": "rsa"}).encode())
    payload = b64u(json.dumps(claims()).encode())
    signature = hmac.new(secret, f"{header}.{payload}".encode(), hashlib.sha256).digest()
    assert status(client, f"{header}.{payload}.{b64u(signature)}") == 401


def test_key_embedded_in_the_header_is_ignored(client):
    """An attacker's key in the `jwk` header does not verify the token."""
    token = sign(ATTACKER, "ES256", "ec", headers={"jwk": public_jwk(ATTACKER, "ES256", "ec")})
    assert status(client, token) == 401


@pytest.mark.parametrize("header", ["jku", "x5u"])
def test_key_url_in_the_header_is_not_fetched(client, sink, header):
    """A key URL in the header is neither trusted nor fetched."""
    url, hits = sink
    assert status(client, sign(ATTACKER, "ES256", "ec", headers={header: url})) == 401
    assert hits == []


def test_token_signed_by_another_key_is_rejected(client):
    """A key that is not in the set cannot sign tokens for one of its `kid`s."""
    assert status(client, sign(ATTACKER, "ES256", "ec")) == 401


def test_algorithm_of_another_key_is_rejected(client):
    """An RSA signature does not verify under the EC key's `kid`."""
    assert status(client, sign(RSA, "RS256", "ec")) == 401


def test_critical_extension_is_rejected(client):
    """A token naming an extension in `crit` is rejected, as none is understood."""
    assert status(client, sign(EC, "ES256", "ec", headers={"crit": ["b64"], "b64": True})) == 401


def test_tampered_payload_is_rejected(client):
    """Changing the claims breaks the signature."""
    header, _, signature = sign(EC, "ES256", "ec").split(".")
    payload = b64u(json.dumps(claims(roles=["admin"])).encode())
    assert status(client, f"{header}.{payload}.{signature}") == 401


def test_stripped_signature_is_rejected(client):
    """A token without its signature is rejected."""
    header, payload, _ = sign(EC, "ES256", "ec").split(".")
    assert status(client, f"{header}.{payload}.") == 401


@pytest.mark.parametrize("aud", ["other", ["other"], None], ids=["other", "other-list", "missing"])
def test_token_for_another_audience_is_rejected(client, aud):
    """With an audience set, a token must be meant for it."""
    assert status(client, sign(EC, "ES256", "ec", aud=aud)) == 401


def test_kid_with_a_newline_is_rejected(client):
    """A `kid` crafted to forge log lines is rejected like any unknown one."""
    assert status(client, sign(EC, "ES256", "ec\nE gw forged")) == 401
