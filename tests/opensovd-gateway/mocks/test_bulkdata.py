# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Gateway integration tests for the bulk-data endpoints.

Uses the ``--mock`` topology which exposes the ``ota_manager`` app backed
by an in-memory bulk-data provider.  Tests cover: API traversal, category
listing, list with filter parameters, multipart upload + download roundtrip,
single-file and category-level delete, missing-provider 404, and
malformed-multipart rejection.
"""

BULK_DATA_HOST = "ota_manager"
BASE = f"/v1/apps/{BULK_DATA_HOST}/bulk-data"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _build_multipart(filename: str, content: bytes) -> tuple[bytes, str]:
    """Return ``(body, boundary)`` for a single-part octet-stream upload."""
    boundary = b"opensovdboundary"
    body = (
        b"--" + boundary + b"\r\n"
        b"Content-Type: application/octet-stream\r\n"
        b"\r\n" + content + b"\r\n"
        b"--" + boundary + b"--\r\n"
    )
    return body, boundary.decode()


def _upload(client, category: str, filename: str, content: bytes) -> None:
    """POST a multipart binary upload and assert 201 Created."""
    body, boundary = _build_multipart(filename, content)
    resp = client.http.request(
        "POST",
        f"{BASE}/{category}",
        content=body,
        headers={
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "Content-Disposition": f'attachment; filename="{filename}"',
            "Content-Length": str(len(body)),
        },
    )
    assert resp.status_code == 201, f"upload failed: {resp.status_code} {resp.text}"


# ---------------------------------------------------------------------------
# Tests: API traversal includes bulk-data link
# ---------------------------------------------------------------------------


def test_traversal_includes_bulk_data_link(client):
    """App capabilities response includes a bulk-data link for the OTA manager."""
    resp = client.get(f"/v1/apps/{BULK_DATA_HOST}")
    assert resp.status_code == 200
    body = resp.json()
    assert "bulk-data" in body, f"missing bulk-data in capabilities: {list(body.keys())}"


# ---------------------------------------------------------------------------
# Tests: categories endpoint
# ---------------------------------------------------------------------------


def test_categories_after_upload(client):
    """After uploading a file a category entry appears in the listing."""
    category = "cat-after-upload"
    _upload(client, category, "fw.bin", b"bytes")

    resp = client.get(BASE)
    assert resp.status_code == 200
    items = resp.json()["items"]
    assert category in items, f"expected {category!r} in categories {items}"


def test_categories_missing_provider_returns_404(client):
    """Requesting bulk-data for an entity without a provider returns 404."""
    resp = client.get("/v1/components/ecu/bulk-data")
    assert resp.status_code == 404
    body = resp.json()
    assert body.get("vendor_code") == "provider-not-available"


# ---------------------------------------------------------------------------
# Tests: list (descriptors) endpoint
# ---------------------------------------------------------------------------


def test_list_returns_uploaded_descriptor(client):
    """Uploaded file descriptor appears in the category listing."""
    category = "cat-list-descriptor"
    _upload(client, category, "desc-test.bin", b"descriptor-test")

    resp = client.get(f"{BASE}/{category}")
    assert resp.status_code == 200
    ids = [item["id"] for item in resp.json()["items"]]
    assert "desc-test.bin" in ids, f"expected desc-test.bin in {ids}"


def test_list_with_tags_query_parameter(client):
    """tags query parameter is accepted without error."""
    category = "cat-list-tags"
    _upload(client, category, "tagged.bin", b"data")

    resp = client.get(f"{BASE}/{category}", params={"tags": "sensor"})
    assert resp.status_code == 200


def test_list_with_date_filter_parameters(client):
    """created-before and created-after are accepted without error."""
    category = "cat-list-dates"
    _upload(client, category, "dated.bin", b"data")

    resp = client.get(
        f"{BASE}/{category}",
        params={
            "created-before": "2099-01-01T00:00:00Z",
            "created-after": "2000-01-01T00:00:00Z",
        },
    )
    assert resp.status_code == 200


def test_list_invalid_date_returns_400(client):
    """An unparseable date in created-before returns 400."""
    resp = client.get(f"{BASE}/any-cat", params={"created-before": "not-a-date"})
    assert resp.status_code == 400


def test_list_with_include_schema(client):
    """include-schema=true returns a schema object alongside items."""
    category = "cat-list-schema"
    _upload(client, category, "schema.bin", b"data")

    resp = client.get(f"{BASE}/{category}", params={"include-schema": "true"})
    assert resp.status_code == 200
    body = resp.json()
    assert "schema" in body, "expected schema key when include-schema=true"


# ---------------------------------------------------------------------------
# Tests: upload → download roundtrip
# ---------------------------------------------------------------------------


def test_upload_and_download_roundtrip(client):
    """Uploading binary data then downloading it returns the exact same bytes."""
    payload = b"\x00\x01\x02\x03binary-content"
    category = "cat-roundtrip"
    filename = "roundtrip.bin"
    _upload(client, category, filename, payload)

    resp = client.get(f"{BASE}/{category}/{filename}")
    assert resp.status_code == 200
    assert resp.content == payload, (
        f"downloaded content mismatch: got {resp.content!r}, expected {payload!r}"
    )


# ---------------------------------------------------------------------------
# Tests: delete endpoints
# ---------------------------------------------------------------------------


def test_delete_single_file_removes_descriptor(client):
    """Deleting a single file removes it from the descriptor list."""
    category = "cat-delete-single"
    _upload(client, category, "delete-me.bin", b"to-be-deleted")

    del_resp = client.delete(f"{BASE}/{category}/delete-me.bin")
    assert del_resp.status_code == 204

    ids = [item["id"] for item in client.get(f"{BASE}/{category}").json()["items"]]
    assert "delete-me.bin" not in ids, f"file still present after delete: {ids}"


def test_delete_category_removes_all_entries(client):
    """Deleting a category removes all its files but leaves other categories intact."""
    _upload(client, "cat-to-delete", "f1.bin", b"f1")
    _upload(client, "cat-to-delete", "f2.bin", b"f2")
    _upload(client, "keeper-cat", "keep.bin", b"keep")

    del_resp = client.delete(f"{BASE}/cat-to-delete")
    assert del_resp.status_code == 200

    cats = client.get(BASE).json()["items"]
    assert "cat-to-delete" not in cats, f"deleted category still present: {cats}"
    assert "keeper-cat" in cats, f"keeper category missing: {cats}"


# ---------------------------------------------------------------------------
# Tests: undeletable (permanent) entries
# ---------------------------------------------------------------------------

PERMANENT_CATEGORY = "logs"
PERMANENT_ID = "cannot_delete"


def test_delete_permanent_entry_returns_409(client):
    """Deleting the protected logs entry is rejected with 409 Conflict."""
    resp = client.delete(f"{BASE}/{PERMANENT_CATEGORY}/{PERMANENT_ID}")
    assert resp.status_code == 409, f"unexpected status: {resp.status_code} {resp.text}"

    ids = [item["id"] for item in client.get(f"{BASE}/{PERMANENT_CATEGORY}").json()["items"]]
    assert PERMANENT_ID in ids, f"protected entry disappeared: {ids}"


def test_delete_permanent_category_is_partial_success(client):
    """Deleting the logs category removes uploads but reports the protected entry."""
    _upload(client, PERMANENT_CATEGORY, "boot.log", b"boot")
    _upload(client, PERMANENT_CATEGORY, "kernel.log", b"kernel")

    resp = client.delete(f"{BASE}/{PERMANENT_CATEGORY}")
    assert resp.status_code == 200
    body = resp.json()

    assert sorted(body["deleted_ids"]) == ["boot.log", "kernel.log"], (
        f"unexpected deleted ids: {body['deleted_ids']}"
    )
    errors = {err["id"]: err["error"] for err in body["errors"]}
    assert PERMANENT_ID in errors, f"expected error for {PERMANENT_ID}: {body['errors']}"
    assert errors[PERMANENT_ID].get("vendor_code") == "unable-to-delete"

    ids = [item["id"] for item in client.get(f"{BASE}/{PERMANENT_CATEGORY}").json()["items"]]
    assert ids == [PERMANENT_ID], f"expected only the protected entry, got {ids}"


# ---------------------------------------------------------------------------
# Tests: error paths
# ---------------------------------------------------------------------------


def test_upload_malformed_missing_content_type_returns_400(client):
    """POST without a Content-Type header is rejected with 400."""
    resp = client.http.request(
        "POST",
        f"{BASE}/any-cat",
        content=b"raw-body",
    )
    assert resp.status_code == 400


def test_download_nonexistent_file_returns_404(client):
    """Downloading a file that does not exist returns 404 (Not Found)."""
    resp = client.get(f"{BASE}/any-cat/no-such-file.bin")
    assert resp.status_code == 404


def test_upload_direct_octet_stream_roundtrip(client):
    """Uploading raw bytes as application/octet-stream (no multipart) succeeds."""
    payload = b"direct-upload-content"
    category = "cat-direct-upload"
    filename = "direct.bin"

    resp = client.http.request(
        "POST",
        f"{BASE}/{category}",
        content=payload,
        headers={
            "Content-Type": "application/octet-stream",
            "Content-Disposition": f'attachment; filename="{filename}"',
            "Content-Length": str(len(payload)),
        },
    )
    assert resp.status_code == 201, f"upload failed: {resp.status_code} {resp.text}"

    dl = client.get(f"{BASE}/{category}/{filename}")
    assert dl.status_code == 200
    assert dl.content == payload
