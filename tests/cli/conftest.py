# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures for CLI integration tests."""

import datetime
import ipaddress

import pytest
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID


def _gen_key() -> rsa.RSAPrivateKey:
    return rsa.generate_private_key(public_exponent=65537, key_size=2048)


def _key_pem(key: rsa.RSAPrivateKey) -> bytes:
    return key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.TraditionalOpenSSL,
        serialization.NoEncryption(),
    )


def _cert_pem(cert: x509.Certificate) -> bytes:
    return cert.public_bytes(serialization.Encoding.PEM)


def generate_test_certs(tmp) -> dict:
    """Generate CA, server, and client certs into *tmp* and return their paths."""
    now = datetime.datetime.now(datetime.UTC)
    day = datetime.timedelta(days=1)

    # CA — self-signed
    ca_key = _gen_key()
    ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "OpenSOVD Test CA")])
    ca_cert = (
        x509.CertificateBuilder()
        .subject_name(ca_name)
        .issuer_name(ca_name)
        .public_key(ca_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now)
        .not_valid_after(now + 365 * day)
        .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True)
        .add_extension(
            x509.KeyUsage(
                digital_signature=True, key_cert_sign=True, crl_sign=True,
                content_commitment=False, key_encipherment=False, data_encipherment=False,
                key_agreement=False, encipher_only=False, decipher_only=False,
            ),
            critical=True,
        )
        .add_extension(
            x509.SubjectKeyIdentifier.from_public_key(ca_key.public_key()), critical=False
        )
        .add_extension(
            x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()),
            critical=False,
        )
        .sign(ca_key, hashes.SHA256())
    )

    # Server cert — signed by CA, SAN covers 127.0.0.1 and localhost
    srv_key = _gen_key()
    srv_cert = (
        x509.CertificateBuilder()
        .subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "127.0.0.1")]))
        .issuer_name(ca_cert.subject)
        .public_key(srv_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now)
        .not_valid_after(now + 365 * day)
        .add_extension(
            x509.SubjectAlternativeName([
                x509.IPAddress(ipaddress.IPv4Address("127.0.0.1")),
                x509.DNSName("localhost"),
            ]),
            critical=False,
        )
        .add_extension(
            x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()),
            critical=False,
        )
        .sign(ca_key, hashes.SHA256())
    )

    # Client cert — signed by CA, clientAuth EKU required by rustls mTLS
    cli_key = _gen_key()
    cli_cert = (
        x509.CertificateBuilder()
        .subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "test-client")]))
        .issuer_name(ca_cert.subject)
        .public_key(cli_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now)
        .not_valid_after(now + 365 * day)
        .add_extension(
            x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), critical=False
        )
        .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
        .add_extension(
            x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()),
            critical=False,
        )
        .sign(ca_key, hashes.SHA256())
    )

    (tmp / "ca.crt").write_bytes(_cert_pem(ca_cert))
    (tmp / "server.crt").write_bytes(_cert_pem(srv_cert))
    (tmp / "server.key").write_bytes(_key_pem(srv_key))
    (tmp / "client.crt").write_bytes(_cert_pem(cli_cert))
    (tmp / "client.key").write_bytes(_key_pem(cli_key))

    return {
        "ca_crt": tmp / "ca.crt",
        "server_crt": tmp / "server.crt",
        "server_key": tmp / "server.key",
        "client_crt": tmp / "client.crt",
        "client_key": tmp / "client.key",
    }


@pytest.fixture(scope="session")
def tls_certs(tmp_path_factory):
    """Generate test certificates once per session into a temp directory."""
    return generate_test_certs(tmp_path_factory.mktemp("tls_certs"))
