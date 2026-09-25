# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Validation of the --tls-* options; every case must exit before listening."""

from opensovd_e2e import spawn_process


def run(request, crate_binary, *args):
    proc = spawn_process(request.config, list(args), None, crate=crate_binary)
    assert proc.process is not None
    proc.process.wait(timeout=10.0)
    out = proc.stdout
    proc.close()
    return proc.process.returncode, out


def test_key_requires_cert(request, crate_binary, tls_certs):
    code, out = run(request, crate_binary, "--tls-key", str(tls_certs["server_key"]))
    assert code == 2
    assert "--tls-cert" in out


def test_cert_requires_key(request, crate_binary, tls_certs):
    code, out = run(request, crate_binary, "--tls-cert", str(tls_certs["server_crt"]))
    assert code == 2
    assert "--tls-key" in out


def test_client_ca_requires_cert(request, crate_binary, tls_certs):
    code, out = run(request, crate_binary, "--tls-client-ca", str(tls_certs["ca_crt"]))
    assert code == 2
    assert "--tls-cert" in out


def test_client_auth_requires_client_ca(request, crate_binary):
    code, out = run(request, crate_binary, "--tls-client-auth", "optional")
    assert code == 2
    assert "--tls-client-ca" in out


def test_cert_requires_https_url(request, crate_binary, tls_certs):
    code, out = run(
        request,
        crate_binary,
        "--url",
        "http://127.0.0.1:0/sovd",
        "--tls-cert",
        str(tls_certs["server_crt"]),
        "--tls-key",
        str(tls_certs["server_key"]),
    )
    assert code == 1
    assert "https://" in out
