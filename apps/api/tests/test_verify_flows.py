"""Route-level email-verification flow for both upload and download links."""

import uuid
from typing import Any

import pytest_asyncio
from httpx import AsyncClient
from sqlalchemy import delete

from portal.db.models import (
    Destination,
    DownloadLink,
    EmailVerification,
    UploadLink,
    UploadSession,
)
from portal.db.session import get_sessionmaker
from portal.frameio.client import DownloadFile
from portal.routes import public as public_routes
from portal.routes import public_downloads as pubdl
from portal.uploads.links import generate_token
from portal.verify import service

CREDS = {"email": "admin@example.com", "password": "test-password"}


@pytest_asyncio.fixture(autouse=True)
async def _clear() -> None:
    async with get_sessionmaker()() as db:
        await db.execute(delete(EmailVerification))
        await db.execute(delete(UploadLink))
        await db.execute(delete(DownloadLink))
        await db.execute(delete(Destination))
        await db.commit()


def _enable_email(monkeypatch: Any) -> dict[str, str]:
    from portal.services.runtime_config import EmailConfig

    cfg = EmailConfig(
        smtp_host="smtp.test",
        smtp_port=587,
        smtp_username="",
        smtp_password="",
        smtp_from="portal@test",
        smtp_use_tls=False,
        smtp_starttls=True,
        notify_email="",
    )
    sent: dict[str, str] = {}

    async def fake_get_email_config(db: Any) -> EmailConfig:
        return cfg

    async def fake_send(config: Any, to: str, code: str, ttl: int, brand: Any = None) -> None:
        sent["to"] = to
        sent["code"] = code

    async def plausible(email: str, *, check_mx: bool) -> bool:
        return True

    monkeypatch.setattr(service, "get_email_config", fake_get_email_config)
    monkeypatch.setattr(service, "send_verification_code", fake_send)
    monkeypatch.setattr(service, "is_plausible_email", plausible)
    return sent


async def _login(client: AsyncClient) -> None:
    assert (await client.post("/api/auth/login", json=CREDS)).status_code == 200


async def _make_upload_link(*, verify_email: bool = True) -> str:
    token = generate_token()
    async with get_sessionmaker()() as db:
        dest = Destination(display_name="Dest", config={"type": "frameio", "account_id": "a1"})
        db.add(dest)
        await db.flush()
        db.add(
            UploadLink(
                token=token,
                destination_id=dest.id,
                uploader_fields_required={"email": True},
                verify_email=verify_email,
            )
        )
        await db.commit()
    return token


# ── upload flow ─────────────────────────────────────────────────────────────────


async def test_upload_verify_flow(client: AsyncClient, monkeypatch: Any) -> None:
    sent = _enable_email(monkeypatch)
    token = await _make_upload_link()

    # ze@ze.ze is rejected before any code is sent (invalid email).
    async def never(email: str, *, check_mx: bool) -> bool:
        return False

    async def always(email: str, *, check_mx: bool) -> bool:
        return True

    monkeypatch.setattr(service, "is_plausible_email", never)
    bad = await client.post(f"/api/public/links/{token}/request-code", json={"email": "ze@ze.ze"})
    assert bad.status_code == 400

    # A real address gets a code.
    monkeypatch.setattr(service, "is_plausible_email", always)
    r = await client.post(f"/api/public/links/{token}/request-code", json={"email": "c@x.com"})
    assert r.status_code == 200 and r.json() == {"trusted": False, "sent": True}
    code = sent["code"]

    # No code → blocked; wrong code → 403.
    assert (
        await client.post(f"/api/public/links/{token}/sessions", json={"email": "c@x.com"})
    ).status_code == 401
    wrong = "000000" if code != "000000" else "111111"
    assert (
        await client.post(
            f"/api/public/links/{token}/sessions", json={"email": "c@x.com", "code": wrong}
        )
    ).status_code == 403

    # Correct code → session + trust cookie.
    ok = await client.post(
        f"/api/public/links/{token}/sessions", json={"email": "c@x.com", "code": code}
    )
    assert ok.status_code == 200
    assert "portal_verified" in ok.headers.get("set-cookie", "")

    # Now trusted on this client: request-code reports trusted, and a session opens with no code.
    again = await client.post(f"/api/public/links/{token}/request-code", json={"email": "c@x.com"})
    assert again.json() == {"trusted": True, "sent": False}
    assert (
        await client.post(f"/api/public/links/{token}/sessions", json={"email": "c@x.com"})
    ).status_code == 200


async def test_upload_verify_code_endpoint_unlocks(client: AsyncClient, monkeypatch: Any) -> None:
    sent = _enable_email(monkeypatch)
    token = await _make_upload_link()

    async def always(email: str, *, check_mx: bool) -> bool:
        return True

    monkeypatch.setattr(service, "is_plausible_email", always)
    await client.post(f"/api/public/links/{token}/request-code", json={"email": "c@x.com"})
    code = sent["code"]

    wrong = "000000" if code != "000000" else "111111"
    bad = await client.post(
        f"/api/public/links/{token}/verify-code", json={"email": "c@x.com", "code": wrong}
    )
    assert bad.status_code == 403

    good = await client.post(
        f"/api/public/links/{token}/verify-code", json={"email": "c@x.com", "code": code}
    )
    assert good.status_code == 200 and good.json()["ok"] is True
    assert "portal_verified" in good.headers.get("set-cookie", "")

    # Device now trusted: a session opens without re-sending a code.
    assert (
        await client.post(f"/api/public/links/{token}/sessions", json={"email": "c@x.com"})
    ).status_code == 200


# ── download flow ─────────────────────────────────────────────────────────────────


class _StubClient:
    async def get_download_url(self, account_id: str, file_id: str) -> str:
        return f"https://s3/{file_id}"

    async def get_file(self, account_id: str, file_id: str) -> DownloadFile:
        return DownloadFile(
            id=file_id, name="f.mov", file_size=1, media_type="video/quicktime",
            download_url=None, thumbnail_url=None,
        )


async def _make_download_link() -> str:
    token = generate_token()
    async with get_sessionmaker()() as db:
        db.add(
            DownloadLink(
                token=token,
                source={"type": "file", "account_id": "a1", "file_id": "f1"},
                viewer_fields_required={"email": True},
                verify_email=True,
            )
        )
        await db.commit()
    return token


async def test_download_verify_flow(client: AsyncClient, monkeypatch: Any) -> None:
    sent = _enable_email(monkeypatch)
    monkeypatch.setattr(pubdl, "get_frameio_client", lambda: _StubClient())
    token = await _make_download_link()

    r = await client.post(f"/api/public/downloads/{token}/request-code", json={"email": "c@x.com"})
    assert r.status_code == 200 and r.json()["sent"] is True
    code = sent["code"]

    # No code → blocked.
    assert (
        await client.post(f"/api/public/downloads/{token}/sessions", json={"email": "c@x.com"})
    ).status_code == 401

    # Correct code → session (file listing) + trust cookie.
    ok = await client.post(
        f"/api/public/downloads/{token}/sessions", json={"email": "c@x.com", "code": code}
    )
    assert ok.status_code == 200
    assert "portal_verified" in ok.headers.get("set-cookie", "")
    assert len(ok.json()["files"]) == 1


async def test_download_does_not_mint_trust_for_an_unverified_viewer(
    client: AsyncClient, monkeypatch: Any
) -> None:
    """Downloading slides the trust window only for trust this browser already holds. Without it
    (cookie gone, or the link switched verification on after the session began) the viewer email
    is unproven here, and a cookie for it would also pass upload links' verification."""
    sent = _enable_email(monkeypatch)
    monkeypatch.setattr(pubdl, "get_frameio_client", lambda: _StubClient())
    token = await _make_download_link()
    await client.post(f"/api/public/downloads/{token}/request-code", json={"email": "c@x.com"})
    ok = await client.post(
        f"/api/public/downloads/{token}/sessions", json={"email": "c@x.com", "code": sent["code"]}
    )
    url = f"/api/public/downloads/{token}/sessions/{ok.json()['session_id']}/files/f1/url"

    trusted = await client.post(url)
    assert trusted.status_code == 200, trusted.text
    assert "portal_verified" in trusted.headers.get("set-cookie", "")

    client.cookies.clear()
    untrusted = await client.post(url)
    assert untrusted.status_code == 200, untrusted.text
    assert "portal_verified" not in untrusted.headers.get("set-cookie", "")


# ── admin: verify_email persists and forces email-required ──────────────────────


async def test_admin_download_link_verify_forces_email(client: AsyncClient) -> None:
    await _login(client)
    resp = await client.post(
        "/api/download-links",
        json={
            "source": {"type": "file", "account_id": "a1", "file_id": "f1"},
            "verify_email": True,
            "viewer_fields_required": {"name": False, "email": False},
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert body["verify_email"] is True
    assert body["viewer_fields_required"]["email"] is True  # forced on


async def test_admin_upload_link_verify_forces_email(client: AsyncClient) -> None:
    await _login(client)
    async with get_sessionmaker()() as db:
        dest = Destination(display_name="D", config={"type": "frameio", "account_id": "a1"})
        db.add(dest)
        await db.commit()
        dest_id = str(dest.id)
    resp = await client.post(
        "/api/upload-links",
        json={
            "destination_id": dest_id,
            "verify_email": True,
            "uploader_fields_required": {"name": False, "email": False, "message": False},
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert body["verify_email"] is True
    assert body["uploader_fields_required"]["email"] is True


# ── uploader delivery receipt (issue #86) ───────────────────────────────────────


def _capture_receipts(monkeypatch: Any) -> list[dict[str, Any]]:
    """Configure email for the completion endpoint and record receipts instead of sending them."""
    from portal.services.runtime_config import EmailConfig

    cfg = EmailConfig(
        smtp_host="smtp.test",
        smtp_port=587,
        smtp_username="",
        smtp_password="",
        smtp_from="portal@test",
        smtp_use_tls=False,
        smtp_starttls=True,
        notify_email="",
    )
    receipts: list[dict[str, Any]] = []

    async def fake_config(db: Any) -> EmailConfig:
        return cfg

    async def no_admin_mail(*args: Any, **kwargs: Any) -> None:
        return None

    async def fake_receipt(config: Any, **kwargs: Any) -> None:
        receipts.append(kwargs)

    monkeypatch.setattr(public_routes, "get_email_config", fake_config)
    monkeypatch.setattr(public_routes, "send_upload_completion", no_admin_mail)
    monkeypatch.setattr(public_routes, "send_upload_receipt", fake_receipt)
    return receipts


async def _session_with_a_file(client: AsyncClient, token: str, body: dict[str, Any]) -> str:
    started = await client.post(f"/api/public/links/{token}/sessions", json=body)
    assert started.status_code == 200
    sid = started.json()["session_id"]
    async with get_sessionmaker()() as db:
        session = await db.get(UploadSession, uuid.UUID(sid))
        assert session is not None
        session.frameio_asset_ids = [{"file_id": "f1", "name": "A-CAM/clip.mov", "size": 1000}]
        await db.commit()
    return sid


async def _verified_session(client: AsyncClient, monkeypatch: Any, token: str) -> str:
    sent = _enable_email(monkeypatch)
    await client.post(f"/api/public/links/{token}/request-code", json={"email": "c@x.com"})
    return await _session_with_a_file(client, token, {"email": "c@x.com", "code": sent["code"]})


async def test_verified_uploader_gets_a_receipt(client: AsyncClient, monkeypatch: Any) -> None:
    token = await _make_upload_link()
    receipts = _capture_receipts(monkeypatch)
    sid = await _verified_session(client, monkeypatch, token)

    done = await client.post(
        f"/api/public/links/{token}/sessions/{sid}/complete", json={"total_bytes": 1000}
    )

    assert done.json() == {"ok": True, "receipt_to": "c@x.com"}
    [receipt] = receipts
    assert receipt["to"] == "c@x.com"
    assert [(f.name, f.size) for f in receipt["files"]] == [("A-CAM/clip.mov", 1000)]
    assert receipt["total_bytes"] == 1000
    assert receipt["link_name"] == "Dest"

    # The browser retries a completion whose response it lost: no second receipt.
    again = await client.post(
        f"/api/public/links/{token}/sessions/{sid}/complete", json={"total_bytes": 1000}
    )
    assert again.json() == {"ok": True, "receipt_to": None}
    assert len(receipts) == 1


async def test_no_receipt_or_trust_without_this_browsers_verification(
    client: AsyncClient, monkeypatch: Any
) -> None:
    """The address must be proven on the completing browser. Without the trust cookie — say the
    session began before the link switched verification on — nothing is mailed, and completing
    mustn't mint trust for an address this browser never verified."""
    token = await _make_upload_link()
    receipts = _capture_receipts(monkeypatch)
    sid = await _verified_session(client, monkeypatch, token)
    client.cookies.clear()

    done = await client.post(
        f"/api/public/links/{token}/sessions/{sid}/complete", json={"total_bytes": 1000}
    )

    assert done.json() == {"ok": True, "receipt_to": None}
    assert receipts == []
    assert "portal_verified" not in done.headers.get("set-cookie", "")


async def test_no_receipt_on_a_link_without_verification(
    client: AsyncClient, monkeypatch: Any
) -> None:
    # A typed-in, unverified address is never mailed — or anyone could make Portal mail any inbox.
    token = await _make_upload_link(verify_email=False)
    receipts = _capture_receipts(monkeypatch)
    sid = await _session_with_a_file(client, token, {"email": "someone@else.com"})

    done = await client.post(
        f"/api/public/links/{token}/sessions/{sid}/complete", json={"total_bytes": 1000}
    )

    assert done.json() == {"ok": True, "receipt_to": None}
    assert receipts == []
