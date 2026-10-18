import uuid
from datetime import UTC, datetime, timedelta
from typing import Any

import httpx
import pytest_asyncio
from sqlalchemy import delete

from portal.db.models import SyncJob, SyncRule
from portal.db.session import get_sessionmaker
from portal.lib.config import get_settings
from portal.storage.base import DestinationConfig, RemoteFile
from portal.sync import worker
from portal.sync.worker import _unwrap_error


def test_unwrap_plain_exception() -> None:
    assert _unwrap_error(ValueError("boom")) == "ValueError: boom"


def test_unwrap_exception_group() -> None:
    # A TaskGroup wraps the real cause in an ExceptionGroup; the recorded error should name it.
    err = httpx.HTTPStatusError(
        "403", request=httpx.Request("GET", "https://x"), response=httpx.Response(403)
    )
    group = BaseExceptionGroup("unhandled errors in a TaskGroup", [err])
    out = _unwrap_error(group)
    assert "HTTPStatusError" in out
    assert "403" in out


def test_unwrap_nested_group() -> None:
    inner = BaseExceptionGroup("inner", [OSError("disk full")])
    outer = BaseExceptionGroup("outer", [inner])
    assert "OSError: disk full" in _unwrap_error(outer)


# --- the source-readiness window ------------------------------------------------------------------


class _NotReadyBackend:
    """A Frame.io file that never finishes uploading — it stays in status "created" forever."""

    async def get_file(self, dest: DestinationConfig, file_id: str) -> RemoteFile:
        return RemoteFile(
            id=file_id,
            name="clip.mov",
            size=10,
            parent_id="fld-1",
            relative_dir="",
            status="created",
        )


class _StubRedis:
    def __init__(self) -> None:
        self.enqueued: list[str] = []
        self.calls: list[tuple[str, tuple[Any, ...], dict[str, Any]]] = []

    async def enqueue_job(self, fn: str, *args: Any, **kw: Any) -> None:
        self.enqueued.append(fn)
        self.calls.append((fn, args, kw))


@pytest_asyncio.fixture(autouse=True)
async def _clear() -> None:
    async with get_sessionmaker()() as db:
        await db.execute(delete(SyncJob))
        await db.execute(delete(SyncRule))
        await db.commit()


async def _make_job(tmp_dest: str, **over: Any) -> uuid.UUID:
    async with get_sessionmaker()() as db:
        rule = SyncRule(
            name="r",
            source_config={
                "type": "frameio",
                "account_id": "a1",
                "workspace_id": "w1",
                "project_id": "p1",
                "folder_id": "fld-1",
                "recursive": True,
            },
            destination_path=tmp_dest,
            enabled=True,
        )
        db.add(rule)
        await db.flush()
        over.setdefault("status", "pending")
        job = SyncJob(sync_rule_id=rule.id, frameio_file_id="file-1", **over)
        db.add(job)
        await db.commit()
        await db.refresh(job)
        return job.id


async def test_first_wait_starts_the_readiness_clock(tmp_path: Any, monkeypatch: Any) -> None:
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    job_id = await _make_job(str(tmp_path))

    result = await worker.run_sync_job({"http": None, "redis": _StubRedis()}, str(job_id))

    assert result == "waiting"
    async with get_sessionmaker()() as db:
        job = await db.get(SyncJob, job_id)
        assert job.status == "waiting"
        assert job.source_wait_started_at is not None  # the window starts now, not at created_at


async def test_expired_window_skips_rather_than_dead_letters(
    tmp_path: Any, monkeypatch: Any
) -> None:
    """An upload abandoned on Frame.io isn't a failure needing attention — it's a terminal skip.

    Dead-lettering it kept the dashboard alert lit forever and had reconcile re-pick it on every
    pass, which is what issue #53 was actually watching happen every 15 minutes.
    """
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    long_ago = datetime.now(UTC) - timedelta(days=7)
    job_id = await _make_job(str(tmp_path), source_wait_started_at=long_ago)

    result = await worker.run_sync_job({"http": None, "redis": _StubRedis()}, str(job_id))

    assert result == "skipped"
    async with get_sessionmaker()() as db:
        job = await db.get(SyncJob, job_id)
        assert job.status == "skipped"
        assert job.completed_at is not None
        assert "never became ready" in (job.error or "")


async def test_a_retried_job_gets_a_full_window(tmp_path: Any, monkeypatch: Any) -> None:
    """The regression behind "manual sync does nothing": a job re-picked long after it was created
    used to measure its wait from created_at and expire on its first tick."""
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    # Created a week ago, but its clock was cleared by a retry/reconcile reset.
    job_id = await _make_job(str(tmp_path), source_wait_started_at=None)
    async with get_sessionmaker()() as db:
        job = await db.get(SyncJob, job_id)
        job.created_at = datetime.now(UTC) - timedelta(days=7)
        await db.commit()

    result = await worker.run_sync_job({"http": None, "redis": _StubRedis()}, str(job_id))

    assert result == "waiting"  # not "skipped" — the ancient created_at is irrelevant now


async def test_running_job_is_not_claimed_twice(tmp_path: Any, monkeypatch: Any) -> None:
    # A job can have two tasks queued (a readiness poll and an upload-complete nudge). The second
    # must not start a parallel download of the same file.
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    job_id = await _make_job(str(tmp_path), status="running")
    redis = _StubRedis()

    assert await worker.run_sync_job({"http": None, "redis": redis}, str(job_id)) == "skip"
    assert redis.enqueued == []
    async with get_sessionmaker()() as db:
        assert (await db.get(SyncJob, job_id)).status == "running"


async def test_waiting_job_is_claimable(tmp_path: Any, monkeypatch: Any) -> None:
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    job_id = await _make_job(
        str(tmp_path), status="waiting", source_wait_started_at=datetime.now(UTC)
    )
    result = await worker.run_sync_job({"http": None, "redis": _StubRedis()}, str(job_id))
    assert result == "waiting"  # it ran (and the source is still not ready)


async def _updated_at(job_id: uuid.UUID) -> datetime:
    async with get_sessionmaker()() as db:
        job = await db.get(SyncJob, job_id)
        assert job is not None
        return job.updated_at


async def test_follow_up_carries_the_jobs_version(tmp_path: Any, monkeypatch: Any) -> None:
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    job_id = await _make_job(str(tmp_path))
    redis = _StubRedis()

    assert await worker.run_sync_job({"http": None, "redis": redis}, str(job_id)) == "waiting"

    [(fn, args, kw)] = redis.calls
    assert fn == "run_sync_job"
    assert args == (str(job_id), (await _updated_at(job_id)).isoformat())
    assert kw == {"_defer_by": get_settings().sync_source_poll_seconds}


async def test_a_woken_job_supersedes_the_pending_recheck(tmp_path: Any, monkeypatch: Any) -> None:
    """Waking a waiting job (issue #85) queues a second task for it. The re-check the earlier
    attempt scheduled must then bow out, or two re-check chains would run for the rest of the
    wait."""
    monkeypatch.setattr(worker, "FrameioStorageBackend", _NotReadyBackend)
    job_id = await _make_job(str(tmp_path))

    first = _StubRedis()
    assert await worker.run_sync_job({"http": None, "redis": first}, str(job_id)) == "waiting"
    [(_, (_, stale_as_of), _)] = first.calls

    woken = _StubRedis()  # the wake-up: a fresh task, no as_of
    assert await worker.run_sync_job({"http": None, "redis": woken}, str(job_id)) == "waiting"
    [(_, (_, live_as_of), _)] = woken.calls
    assert live_as_of != stale_as_of

    # The superseded re-check fires: dropped, scheduling nothing.
    late = _StubRedis()
    assert (
        await worker.run_sync_job({"http": None, "redis": late}, str(job_id), stale_as_of) == "skip"
    )
    assert late.calls == []

    # The wake-up's own re-check still runs.
    assert (
        await worker.run_sync_job({"http": None, "redis": _StubRedis()}, str(job_id), live_as_of)
        == "waiting"
    )


async def test_retry_follow_up_carries_the_jobs_version(tmp_path: Any, monkeypatch: Any) -> None:
    class _Boom:
        async def get_file(self, dest: DestinationConfig, file_id: str) -> RemoteFile:
            raise httpx.ConnectError("network blip")

    monkeypatch.setattr(worker, "FrameioStorageBackend", _Boom)
    job_id = await _make_job(str(tmp_path))
    redis = _StubRedis()

    assert await worker.run_sync_job({"http": None, "redis": redis}, str(job_id)) == "retry"
    [(_, args, kw)] = redis.calls
    assert args == (str(job_id), (await _updated_at(job_id)).isoformat())
    assert kw == {"_defer_by": get_settings().sync_retry_base_seconds}
