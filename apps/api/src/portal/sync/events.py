"""Turn ingested Frame.io webhook events into sync jobs.

A verified webhook event is persisted by the ingestion endpoint (``routes.webhooks``); this is
the processor that interprets it. For a file-arrival event it finds the enabled sync rules whose
source matches and creates one pending ``sync_job`` per (rule, file). The job is the unit the
Step 11 worker downloads.

Folder-level matching: Frame.io webhooks are workspace-scoped, and the delivery body names the
file's account/workspace/project but not its parent folder. So we match on those three ids here
and leave the precise folder-membership check to the Step 11 worker, which fetches file metadata
(including parent_id) before downloading and skips files outside the rule's folder.
"""

from __future__ import annotations

import uuid
from collections.abc import Awaitable, Callable
from datetime import UTC, datetime

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from portal.db.models import SyncJob, SyncRule, WebhookEvent
from portal.db.session import get_sessionmaker
from portal.frameio.webhooks import SYNC_TRIGGER_EVENTS, WebhookPayload, parse_webhook_payload
from portal.lib.logging import get_logger
from portal.sync.queue import enqueue_sync_job

log = get_logger("sync.events")


def rule_matches(source_config: dict[str, object], payload: WebhookPayload) -> bool:
    """True if a sync rule's source is in the same account/workspace/project as the event.

    All three ids must be present and equal — a missing id never matches, so an event without a
    project can't accidentally fan out to every rule.
    """
    for key, value in (
        ("account_id", payload.account_id),
        ("workspace_id", payload.workspace_id),
        ("project_id", payload.project_id),
    ):
        if value is None or source_config.get(key) != value:
            return False
    return True


def is_sync_trigger(payload: WebhookPayload) -> bool:
    """True if the event is a file arrival worth syncing."""
    return payload.type in SYNC_TRIGGER_EVENTS and payload.resource_type == "file"


async def process_event(
    db: AsyncSession,
    event_id: uuid.UUID,
    on_job_created: Callable[[uuid.UUID], Awaitable[None]] | None = None,
) -> int:
    """Process one persisted webhook event; return the number of sync jobs created.

    For each job to run, ``on_job_created`` (if given) is awaited with its id — the worker uses it
    to enqueue the download. That is every created job, plus an existing job parked in "waiting":
    the arrival event is the news it was waiting for (see ``_job_to_run``). Idempotent:
    already-processed events are skipped, and a (rule, file) pair that already has a job is not
    duplicated, so webhook retries and the file.ready / file.upload.completed pair don't create
    extra jobs.
    """
    event = await db.get(WebhookEvent, event_id)
    if event is None or event.processed:
        return 0

    created_ids: list[uuid.UUID] = []
    woken_ids: list[uuid.UUID] = []
    try:
        payload = parse_webhook_payload(event.raw)
        if is_sync_trigger(payload) and payload.resource_id is not None:
            created_ids, woken_ids = await _jobs_for_event(db, payload, payload.resource_id)
        event.processed = True
        event.processed_at = datetime.now(UTC)
        event.error = None
    except Exception as exc:  # noqa: BLE001 - record and stop; reconciliation is the safety net
        # Mark processed so a malformed event doesn't loop forever; the reconciliation job catches
        # anything genuinely missed.
        event.processed = True
        event.processed_at = datetime.now(UTC)
        event.error = str(exc)
        log.warning("webhook.process_failed", event_id=str(event_id), error=str(exc))

    await db.commit()
    if created_ids:
        log.info("webhook.jobs_created", event_id=str(event_id), count=len(created_ids))
    if woken_ids:
        log.info("webhook.jobs_woken", event_id=str(event_id), count=len(woken_ids))
    if on_job_created is not None:
        for job_id in [*created_ids, *woken_ids]:
            await on_job_created(job_id)
    return len(created_ids)


async def trigger_sync_for_upload(
    account_id: str, folder_id: str, file_ids: list[str]
) -> int:
    """Drive sync the moment a client upload through a Portal link completes — no webhook needed.

    Portal owns the upload-completion event, so when files land via an upload link we know exactly
    which files arrived and which folder they're in. We create + enqueue sync jobs for any enabled
    rule whose source folder *is* the upload's destination folder. Files added directly in Frame.io
    (outside Portal) aren't covered here — the reconcile cron catches those.

    A file that already has a job parked in "waiting" gets that job woken now (see
    ``_job_to_run``).

    Runs as a background task in the API process: opens its own session, commits, then enqueues
    (so the worker never races a not-yet-committed job row).
    """
    if not file_ids:
        return 0

    created_ids: list[uuid.UUID] = []
    woken_ids: list[uuid.UUID] = []
    async with get_sessionmaker()() as db:
        result = await db.execute(select(SyncRule).where(SyncRule.enabled.is_(True)))
        rules = [
            r
            for r in result.scalars().all()
            if r.source_config.get("account_id") == account_id
            and r.source_config.get("folder_id") == folder_id
        ]
        for rule in rules:
            for file_id in file_ids:
                found = await _job_to_run(db, rule.id, file_id)
                if found is not None:
                    job_id, created = found
                    (created_ids if created else woken_ids).append(job_id)
        await db.commit()

    for job_id in [*created_ids, *woken_ids]:
        await enqueue_sync_job(job_id)
    if created_ids:
        log.info("sync.upload_trigger.created", folder_id=folder_id, count=len(created_ids))
    if woken_ids:
        log.info("sync.upload_trigger.woken", folder_id=folder_id, count=len(woken_ids))
    return len(created_ids)


async def _job_to_run(
    db: AsyncSession, rule_id: uuid.UUID, file_id: str
) -> tuple[uuid.UUID, bool] | None:
    """The job to run now for a file that just arrived, as ``(job_id, created)`` — or None when
    the file's existing job needs nothing (done, running, already queued, ...).

    A new file gets a new pending job. An existing job parked in "waiting" is woken: the reconcile
    cron often finds a large file on Frame.io while it is still uploading and leaves its job
    re-checking readiness every ``sync_source_poll_seconds``. Deduping on the existing row used to
    ignore the arrival — the moment we know the file is finalized — so the sync trailed the
    upload by up to a full poll (issue #85). Waking it is safe: the worker claims jobs atomically
    and drops the superseded poll (see portal.sync.worker.run_sync_job).
    """
    existing = await db.scalar(
        select(SyncJob).where(SyncJob.sync_rule_id == rule_id, SyncJob.frameio_file_id == file_id)
    )
    if existing is None:
        job = SyncJob(sync_rule_id=rule_id, frameio_file_id=file_id, status="pending")
        db.add(job)
        await db.flush()
        return job.id, True
    if existing.status == "waiting":
        return existing.id, False
    return None


async def _jobs_for_event(
    db: AsyncSession, payload: WebhookPayload, file_id: str
) -> tuple[list[uuid.UUID], list[uuid.UUID]]:
    """Jobs to run for a file-arrival event, as (created, woken) id lists."""
    result = await db.execute(select(SyncRule).where(SyncRule.enabled.is_(True)))
    created_ids: list[uuid.UUID] = []
    woken_ids: list[uuid.UUID] = []
    for rule in result.scalars().all():
        if not rule_matches(rule.source_config, payload):
            continue
        found = await _job_to_run(db, rule.id, file_id)
        if found is not None:
            job_id, created = found
            (created_ids if created else woken_ids).append(job_id)
    return created_ids, woken_ids
