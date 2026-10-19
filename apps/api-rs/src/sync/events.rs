
//! Turn Frame.io webhook events into sync jobs — port of `portal.sync.events`.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::frameio::webhooks::{parse_webhook_payload, SYNC_TRIGGER_EVENTS};

pub fn rule_matches(source_config: &serde_json::Value, payload: &crate::frameio::webhooks::WebhookPayload) -> bool {
    for (key, value) in [
        ("account_id", &payload.account_id),
        ("workspace_id", &payload.workspace_id),
        ("project_id", &payload.project_id),
    ] {
        let Some(v) = value else { return false };
        if source_config.get(key).and_then(|x| x.as_str()) != Some(v.as_str()) {
            return false;
        }
    }
    true
}

pub fn is_sync_trigger(payload: &crate::frameio::webhooks::WebhookPayload) -> bool {
    payload
        .r#type
        .as_deref()
        .map(|t| SYNC_TRIGGER_EVENTS.contains(&t))
        .unwrap_or(false)
        && payload.resource_type.as_deref() == Some("file")
}

/// The job to run now for a file that just arrived, as `(job_id, created)` — or None when the
/// file's existing job needs nothing (done, running, already queued, ...).
///
/// A new file gets a new pending job. An existing job parked in "waiting" is woken: the reconcile
/// cron often finds a large file on Frame.io while it is still uploading and leaves its job
/// re-checking readiness every `sync_source_poll_seconds`. Deduping on the existing row used to
/// ignore the arrival — the moment we know the file is finalized — so the sync trailed the upload
/// by up to a full poll (issue #85). Waking it is safe: the worker claims jobs atomically and drops
/// the superseded poll (see `sync::worker::run_sync_job`).
async fn job_to_run(db: &PgPool, rule_id: Uuid, file_id: &str) -> sqlx::Result<Option<(Uuid, bool)>> {
    let existing = sqlx::query("SELECT id, status FROM sync_jobs WHERE sync_rule_id = $1 AND frameio_file_id = $2")
        .bind(rule_id)
        .bind(file_id)
        .fetch_optional(db)
        .await?;
    match existing {
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO sync_jobs (sync_rule_id, frameio_file_id, status, retry_count) VALUES ($1,$2,'pending',0) RETURNING id",
            )
            .bind(rule_id)
            .bind(file_id)
            .fetch_one(db)
            .await?;
            Ok(Some((id, true)))
        }
        Some(row) if row.get::<String, _>("status") == "waiting" => Ok(Some((row.get("id"), false))),
        Some(_) => Ok(None),
    }
}

/// Jobs to run for a file-arrival event, as (created, woken) id lists.
async fn jobs_for_event(
    db: &PgPool,
    payload: &crate::frameio::webhooks::WebhookPayload,
    file_id: &str,
) -> sqlx::Result<(Vec<Uuid>, Vec<Uuid>)> {
    let rules = sqlx::query("SELECT id, source_config FROM sync_rules WHERE enabled")
        .fetch_all(db)
        .await?;
    let (mut created, mut woken) = (Vec::new(), Vec::new());
    for r in rules {
        let source: serde_json::Value = r.get("source_config");
        if !rule_matches(&source, payload) {
            continue;
        }
        if let Some((id, is_new)) = job_to_run(db, r.get("id"), file_id).await? {
            if is_new { created.push(id) } else { woken.push(id) }
        }
    }
    Ok((created, woken))
}

/// Process one persisted webhook event; returns the ids of the jobs to run — every created job,
/// plus an existing job parked in "waiting" (the arrival is the news it was waiting for).
/// Idempotent: already-processed events are skipped, and a (rule, file) pair that already has a
/// job is not duplicated, so webhook retries and the file.ready / file.upload.completed pair don't
/// create extra jobs.
pub async fn process_event(db: &PgPool, event_id: Uuid) -> Vec<Uuid> {
    let row = sqlx::query("SELECT raw, processed FROM webhook_events WHERE id = $1")
        .bind(event_id)
        .fetch_optional(db)
        .await
        .unwrap_or(None);
    let Some(row) = row else { return Vec::new() };
    let processed: bool = row.get("processed");
    if processed {
        return Vec::new();
    }
    let raw: serde_json::Value = row.get("raw");

    let (mut created, mut woken) = (Vec::new(), Vec::new());
    let mut error: Option<String> = None;
    let payload = parse_webhook_payload(&raw);
    if is_sync_trigger(&payload) && let Some(file_id) = &payload.resource_id {
        match jobs_for_event(db, &payload, file_id).await {
            Ok((c, w)) => (created, woken) = (c, w),
            // Record and stop; reconciliation is the safety net. Marked processed so a
            // malformed event doesn't loop forever.
            Err(e) => {
                tracing::warn!("webhook.process_failed event_id={event_id} error={e}");
                error = Some(e.to_string());
            }
        }
    }
    let _ = sqlx::query("UPDATE webhook_events SET processed=true, processed_at=now(), error=$1 WHERE id=$2")
        .bind(error.as_deref())
        .bind(event_id)
        .execute(db)
        .await;
    if !created.is_empty() {
        tracing::info!("webhook.jobs_created event_id={event_id} count={}", created.len());
    }
    if !woken.is_empty() {
        tracing::info!("webhook.jobs_woken event_id={event_id} count={}", woken.len());
    }
    created.extend(woken);
    created
}

/// Drive sync the moment a client upload through a Portal link completes: jobs for every enabled
/// rule whose source folder *is* the upload's destination folder. Files added directly in
/// Frame.io (outside Portal) aren't covered here — the reconcile cron catches those. A file that
/// already has a job parked in "waiting" gets that job woken now (see `job_to_run`).
///
/// Returns the ids to enqueue (created + woken); every row is committed by then, so the worker
/// never races a not-yet-visible job.
pub async fn trigger_sync_for_upload(
    db: &PgPool,
    account_id: &str,
    folder_id: &str,
    file_ids: &[String],
) -> Vec<Uuid> {
    if file_ids.is_empty() {
        return Vec::new();
    }
    let rules = sqlx::query("SELECT id, source_config FROM sync_rules WHERE enabled")
        .fetch_all(db)
        .await
        .unwrap_or_default();
    let (mut created, mut woken) = (Vec::new(), Vec::new());
    for r in rules {
        let source: serde_json::Value = r.get("source_config");
        if source.get("account_id").and_then(|v| v.as_str()) != Some(account_id)
            || source.get("folder_id").and_then(|v| v.as_str()) != Some(folder_id)
        {
            continue;
        }
        let rule_id: Uuid = r.get("id");
        for file_id in file_ids {
            match job_to_run(db, rule_id, file_id).await {
                Ok(Some((id, true))) => created.push(id),
                Ok(Some((id, false))) => woken.push(id),
                Ok(None) => {}
                Err(e) => tracing::warn!("sync.upload_trigger.failed rule_id={rule_id} file_id={file_id} error={e}"),
            }
        }
    }
    if !created.is_empty() {
        tracing::info!("sync.upload_trigger.created folder_id={folder_id} count={}", created.len());
    }
    if !woken.is_empty() {
        tracing::info!("sync.upload_trigger.woken folder_id={folder_id} count={}", woken.len());
    }
    created.extend(woken);
    created
}
