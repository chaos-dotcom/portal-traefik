import pytest
from httpx import AsyncClient

from portal.notify.email import (
    EmailError,
    UploadedFile,
    _format_bytes,
    _format_duration,
    render_upload_completion,
    render_upload_receipt,
    send_email,
)

CREDS = {"email": "admin@example.com", "password": "test-password"}


def test_format_bytes() -> None:
    assert _format_bytes(512) == "512 B"
    assert _format_bytes(2048) == "2.0 KB"
    assert _format_bytes(5 * 1024**3) == "5.0 GB"


def test_format_duration() -> None:
    assert _format_duration(45) == "45s"
    assert _format_duration(90) == "1m 30s"
    assert _format_duration(3700) == "1h 1m"


def test_render_upload_completion() -> None:
    subject, body, html = render_upload_completion(
        link_name="DIT Drop",
        uploader_name="Berlin DIT",
        uploader_email="dit@example.com",
        uploader_message="200GB of dailies",
        files=[UploadedFile("A-CAM/clip001.mov", 1024), UploadedFile("audio.wav", None)],
        total_bytes=1024,
        duration_seconds=125,
    )
    assert "DIT Drop" in subject and "2 files" in subject
    assert "Berlin DIT" in body
    assert "dit@example.com" in body
    assert "200GB of dailies" in body
    assert "A-CAM/clip001.mov" in body and "1.0 KB" in body
    assert "audio.wav" in body and "—" in body  # unknown size
    assert "2m 5s" in body
    # HTML alternative carries the same content, wrapped in the branded shell.
    assert "<!DOCTYPE html>" in html
    assert "A-CAM/clip001.mov" in html and "Berlin DIT" in html
    assert "Upload complete" in html


def test_render_upload_receipt() -> None:
    subject, body, html = render_upload_receipt(
        link_name="DIT Drop",
        files=[UploadedFile("A-CAM/clip001.mov", 1024), UploadedFile("<b>x</b>.wav", None)],
        total_bytes=1024,
    )
    assert subject == "Delivered to DIT Drop: 2 files"
    assert "A-CAM/clip001.mov  (1.0 KB)" in body
    assert "<b>x</b>.wav  (—)" in body  # plain-text part is not HTML
    assert "Total: 2 files, 1.0 KB" in body
    # The uploader typed those names: the HTML part must escape them.
    assert "&lt;b&gt;x&lt;/b&gt;.wav" in html and "<b>x</b>" not in html
    assert "Upload complete" in html


async def test_send_email_refuses_more_than_one_recipient() -> None:
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
    with pytest.raises(EmailError):  # refused before any SMTP connection is attempted
        await send_email(cfg, "postmaster,me@evil.com", "s", "t")


# ── settings endpoints (email not configured in the test env) ─────────────────


async def _login(client: AsyncClient) -> None:
    assert (await client.post("/api/auth/login", json=CREDS)).status_code == 200


async def _clear_email(client: AsyncClient) -> None:
    # app_settings persists across the test session, so explicitly clear SMTP config first.
    await client.post("/api/settings/email", json={"clear_password": True})


async def test_email_status_not_configured(client: AsyncClient) -> None:
    await _login(client)
    await _clear_email(client)
    resp = await client.get("/api/settings/email")
    assert resp.status_code == 200
    assert resp.json()["configured"] is False


async def test_email_test_requires_config(client: AsyncClient) -> None:
    await _login(client)
    await _clear_email(client)
    resp = await client.post("/api/settings/email/test")
    assert resp.status_code == 400


async def test_settings_require_auth(client: AsyncClient) -> None:
    assert (await client.get("/api/settings/email")).status_code == 401
    assert (await client.post("/api/settings/email/test")).status_code == 401
