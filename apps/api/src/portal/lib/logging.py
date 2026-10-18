import logging
import uuid
from collections.abc import Awaitable, Callable
from typing import Any

import structlog
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response

# The container healthcheck probes this every 15s. Logged in full, healthy probes were about a third
# of all log lines and pushed real activity out of Docker's size-capped log in ~1.5 days. Only an
# unhealthy answer is worth a line.
HEALTH_PATH = "/api/health"


class QuietHealthyProbes(logging.Filter):
    """Drop uvicorn's access-log line for a health-check probe that came back healthy."""

    def filter(self, record: logging.LogRecord) -> bool:
        args = record.args
        # uvicorn's access args: (client, method, path, http_version, status)
        if isinstance(args, tuple) and len(args) == 5:
            path, status = args[2], args[4]
            return not (path == HEALTH_PATH and isinstance(status, int) and status < 400)
        return True


_QUIET_HEALTHY_PROBES = QuietHealthyProbes()


def configure_logging(level: str = "INFO") -> None:
    logging.basicConfig(format="%(message)s", level=getattr(logging, level.upper(), logging.INFO))
    logging.getLogger("uvicorn.access").addFilter(_QUIET_HEALTHY_PROBES)  # no-op if already added
    structlog.configure(
        processors=[
            structlog.contextvars.merge_contextvars,
            structlog.processors.add_log_level,
            structlog.processors.TimeStamper(fmt="iso"),
            structlog.processors.StackInfoRenderer(),
            structlog.processors.format_exc_info,
            structlog.processors.JSONRenderer(),
        ],
        wrapper_class=structlog.make_filtering_bound_logger(
            getattr(logging, level.upper(), logging.INFO)
        ),
        cache_logger_on_first_use=True,
    )


def get_logger(name: str | None = None) -> Any:
    return structlog.get_logger(name)


class RequestIdMiddleware(BaseHTTPMiddleware):
    """Bind a request_id to the structlog context for the lifetime of each request."""

    async def dispatch(
        self, request: Request, call_next: Callable[[Request], Awaitable[Response]]
    ) -> Response:
        request_id = request.headers.get("x-request-id") or uuid.uuid4().hex
        structlog.contextvars.clear_contextvars()
        structlog.contextvars.bind_contextvars(request_id=request_id)
        log = get_logger("http")
        probe = request.url.path == HEALTH_PATH  # logged only if unhealthy (see HEALTH_PATH)
        if not probe:
            log.info("request.start", method=request.method, path=request.url.path)
        try:
            response = await call_next(request)
        except Exception:
            log.exception("request.error", method=request.method, path=request.url.path)
            raise
        response.headers["x-request-id"] = request_id
        if not probe or response.status_code >= 400:
            log.info("request.end", status=response.status_code, path=request.url.path)
        return response
