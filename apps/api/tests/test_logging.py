import logging

from httpx import AsyncClient
from structlog.testing import capture_logs

from portal.lib.logging import HEALTH_PATH, QuietHealthyProbes


def _access_record(path: str, status: int) -> logging.LogRecord:
    # The shape uvicorn.access emits: '%s - "%s %s HTTP/%s" %d' with these args.
    return logging.LogRecord(
        "uvicorn.access",
        logging.INFO,
        __file__,
        0,
        '%s - "%s %s HTTP/%s" %d',
        ("127.0.0.1:5000", "GET", path, "1.1", status),
        None,
    )


def test_access_log_drops_only_healthy_probes() -> None:
    quiet = QuietHealthyProbes()
    assert quiet.filter(_access_record(HEALTH_PATH, 200)) is False
    assert quiet.filter(_access_record(HEALTH_PATH, 503)) is True  # unhealthy: keep
    assert quiet.filter(_access_record("/api/dashboard/transfers", 200)) is True
    plain = logging.LogRecord("uvicorn.access", logging.INFO, __file__, 0, "hello", None, None)
    assert quiet.filter(plain) is True


async def test_healthy_probe_is_not_request_logged(client: AsyncClient) -> None:
    with capture_logs() as logs:
        resp = await client.get(HEALTH_PATH)
        await client.get("/api/frameio/status")  # any other request is still logged
    assert resp.status_code == 200
    paths = [e.get("path") for e in logs if e.get("event") in ("request.start", "request.end")]
    assert HEALTH_PATH not in paths
    assert "/api/frameio/status" in paths
