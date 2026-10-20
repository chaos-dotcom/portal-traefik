
//! Sync worker — port of `portal.sync.worker`. Consumes the Redis queue and runs the crons.

use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::state::AppState;
use crate::storage::frameio_backend::FrameioStorageBackend;
use crate::sync::events::process_event;
use crate::sync::queue::{self, Task, PROCESS_WEBHOOK_EVENT, RECONCILE_RULE, RUN_SYNC_JOB};
use crate::sync::runner::{self, execute_job, JobError};

/// Run one task from the queue.
async fn run_task(state: AppState, task: Task) {
    let Task { task, arg, as_of } = task;
    match task.as_str() {
        PROCESS_WEBHOOK_EVENT => {
            if let Ok(event_id) = Uuid::parse_str(&arg) {
                let to_run = process_event(&state.db, event_id).await;
                let mut redis = state.redis.clone();
                for job_id in to_run {
                    let _ = queue::enqueue(&mut redis, RUN_SYNC_JOB, &job_id.to_string(), 0.0).await;
                }
            }
        }
        RECONCILE_RULE => {
            if let Ok(rule_id) = Uuid::parse_str(&arg) {
                let backend = FrameioStorageBackend::new(state.frameio.clone());
                match runner::reconcile_rule(&state.db, rule_id, &backend, &state.settings).await {
                    Ok(created) => {
                        let mut redis = state.redis.clone();
                        for job_id in created {
                            let _ = queue::enqueue(&mut redis, RUN_SYNC_JOB, &job_id.to_string(), 0.0).await;
                        }
                    }
                    Err(e) => tracing::warn!("sync.reconcile.failed {e}"),
                }
            }
        }
        RUN_SYNC_JOB => {
            if let Ok(job_id) = Uuid::parse_str(&arg) {
                // Like arq's job_timeout: a full file pull may take long, but not forever. A job
                // cut off here stays "running" until reclaim/reconcile re-picks it.
                let limit = Duration::from_secs(state.settings.sync_job_timeout_seconds.max(1) as u64);
                if tokio::time::timeout(limit, run_sync_job(&state, job_id, as_of.as_deref())).await.is_err() {
                    tracing::warn!("sync.job.timed_out job_id={job_id}");
                }
            }
        }
        other => tracing::warn!("worker.unknown_task {other}"),
    }
}

/// Schedule the job's next attempt (a readiness re-check or a retry backoff).
///
/// Called after the write that recorded this attempt's outcome. The task carries the job's
/// `updated_at` as of that write — read back from the database, so worker/DB clock skew can't
/// matter — and only runs if nothing has touched the job since (see `run_sync_job`).
async fn follow_up(state: &AppState, job_id: Uuid, defer_s: i64) {
    let as_of: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT updated_at FROM sync_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let Some(as_of) = as_of else { return }; // deleted meanwhile: nothing left to follow up
    let as_of = as_of.to_rfc3339_opts(SecondsFormat::Micros, true);
    let mut redis = state.redis.clone();
    let _ = queue::enqueue_as_of(&mut redis, RUN_SYNC_JOB, &job_id.to_string(), Some(&as_of), defer_s as f64).await;
}

/// Download one file. On failure, retry with exponential backoff; dead-letter after the cap.
///
/// `as_of` is set on the follow-ups this task schedules for itself (see `follow_up`); a fresh
/// request to run the job (a new file, a wake-up, a reset, a manual retry) has none.
async fn run_sync_job(state: &AppState, job_id: Uuid, as_of: Option<&str>) -> &'static str {
    // Claim the job atomically. A job can have more than one task queued for it (a readiness
    // re-check plus an arrival wake-up, or a reconcile re-pick); only one may run it, or two
    // downloads would race on the same .part file. Terminal and already-running jobs are left
    // alone — orphaned "running" jobs are re-picked by reclaim/reconcile resetting them first.
    // A follow-up (`as_of`) runs only if the job is still as its own attempt left it: anything
    // that touched it since (a wake-up, a reconcile reset, a manual retry) ran the job itself and
    // scheduled its own follow-up, so this one is superseded.
    let as_of: Option<DateTime<Utc>> = match as_of {
        None => None,
        Some(raw) => match DateTime::parse_from_rfc3339(raw) {
            Ok(t) => Some(t.with_timezone(&Utc)),
            Err(_) => {
                tracing::warn!("sync.job.bad_as_of job_id={job_id} as_of={raw}");
                return "skip";
            }
        },
    };
    let claimed: Option<Uuid> = sqlx::query_scalar(
        "UPDATE sync_jobs SET status='running', started_at=now(), updated_at=now()
          WHERE id = $1 AND status IN ('pending', 'waiting')
            AND ($2::timestamptz IS NULL OR updated_at <= $2)
      RETURNING id",
    )
    .bind(job_id)
    .bind(as_of)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    if claimed.is_none() {
        return "skip";
    }
    let Ok(Some(mut job)) = runner::load_job(&state.db, job_id).await else { return "skip" };

    let backend = FrameioStorageBackend::new(state.frameio.clone());
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(300))
        .build()
        .expect("http client");

    match execute_job(&state.db, &mut job, &backend, &http, &state.settings).await {
        Ok(()) => {
            let _ = runner::save_job(&state.db, &job).await;
            "done"
        }
        Err(JobError::SourceNotReady(msg)) => {
            // The original isn't downloadable yet (still finalizing). Wait patiently — re-check on
            // a fixed cadence WITHOUT spending the retry budget — and give up only after the
            // readiness timeout. The window runs from this attempt's first wait.
            let now = Utc::now();
            let waiting_since = *job.source_wait_started_at.get_or_insert(now);
            let waited = (now - waiting_since).num_seconds();
            if waited > state.settings.sync_source_ready_timeout_seconds {
                // Abandoned on Frame.io: a terminal skip, not a failure for an admin to fix.
                job.status = "skipped".to_string();
                job.error = Some(format!("source never became ready: {msg}"));
                job.completed_at = Some(now);
                let _ = runner::save_job(&state.db, &job).await;
                tracing::warn!("sync.job.source_timeout job_id={job_id} waited_s={waited}");
                return "skipped";
            }
            job.status = "waiting".to_string();
            job.error = Some(msg);
            let _ = runner::save_job(&state.db, &job).await;
            let poll = state.settings.sync_source_poll_seconds;
            follow_up(state, job_id, poll).await;
            tracing::info!("sync.job.waiting_source job_id={job_id} poll_s={poll}");
            "waiting"
        }
        Err(JobError::DestinationUnavailable(msg)) => {
            // The destination share is not mounted: wait for it without spending the retry
            // budget, and with no timeout (an outage lasting days should resume cleanly).
            job.status = "waiting".to_string();
            job.error = Some(msg);
            let _ = runner::save_job(&state.db, &job).await;
            let poll = state.settings.sync_source_poll_seconds;
            follow_up(state, job_id, poll).await;
            tracing::warn!("sync.job.destination_unavailable job_id={job_id} poll_s={poll}");
            "waiting"
        }
        Err(e) => {
            let error = e.to_string();
            job.retry_count += 1;
            job.error = Some(error.clone());
            if job.retry_count >= state.settings.sync_max_retries {
                job.status = "dead_letter".to_string();
                let _ = runner::save_job(&state.db, &job).await;
                tracing::warn!("sync.job.dead_letter job_id={job_id} error={error}");
                return "dead_letter";
            }
            job.status = "pending".to_string();
            let _ = runner::save_job(&state.db, &job).await;
            let defer = state.settings.sync_retry_base_seconds * 2i64.pow((job.retry_count - 1).max(0) as u32);
            follow_up(state, job_id, defer).await;
            tracing::info!("sync.job.retry job_id={job_id} attempt={} defer_s={defer}", job.retry_count);
            "retry"
        }
    }
}

/// Hourly/interval reconciliation of every enabled rule.
async fn reconcile_all(state: &AppState) -> usize {
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM sync_rules WHERE enabled")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let mut total = 0;
    for id in ids {
        let backend = FrameioStorageBackend::new(state.frameio.clone());
        if let Ok(created) = runner::reconcile_rule(&state.db, id, &backend, &state.settings).await {
            let mut redis = state.redis.clone();
            for job_id in created {
                let _ = queue::enqueue(&mut redis, RUN_SYNC_JOB, &job_id.to_string(), 0.0).await;
            }
            total += 1;
        }
    }
    total
}

/// Worker entrypoint: reclaim orphans on boot, then drain the queue and run the crons.
pub async fn run(state: AppState) -> anyhow::Result<()> {
    tracing::info!("worker.startup");
    // Re-pick jobs stranded "running" by a previous worker.
    if let Ok(orphans) = runner::reclaim_orphaned_running(&state.db).await {
        let mut redis = state.redis.clone();
        for id in orphans {
            let _ = queue::enqueue(&mut redis, RUN_SYNC_JOB, &id.to_string(), 0.0).await;
        }
    }

    let reconcile_every = Duration::from_secs((state.settings.sync_reconcile_interval_minutes.max(1) * 60) as u64);
    let mut last_reconcile = Instant::now() - reconcile_every; // reconcile once at startup
    let mut last_abandon = Instant::now();
    let mut last_retention = Instant::now();
    let mut last_reminders = Instant::now() - Duration::from_secs(10 * 60); // check once at startup
    let mut inflight: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    loop {
        inflight.retain(|h| !h.is_finished());
        let capacity = state.settings.sync_max_concurrent_jobs.saturating_sub(inflight.len());
        if capacity > 0 {
            let mut redis = state.redis.clone();
            if let Ok(jobs) = queue::dequeue_due(&mut redis, capacity).await {
                for task in jobs {
                    let st = state.clone();
                    inflight.push(tokio::spawn(run_task(st, task)));
                }
            }
        }

        if last_reconcile.elapsed() >= reconcile_every {
            last_reconcile = Instant::now();
            reconcile_all(&state).await;
        }
        if last_abandon.elapsed() >= Duration::from_secs(15 * 60) {
            last_abandon = Instant::now();
            let _ = crate::services::upload_sessions::abandon_stale_upload_sessions(&state.db, &state.settings).await;
        }
        if last_reminders.elapsed() >= Duration::from_secs(10 * 60) {
            last_reminders = Instant::now();
            crate::services::reminders::send_due(&state).await;
        }
        if last_retention.elapsed() >= Duration::from_secs(24 * 60 * 60) {
            last_retention = Instant::now();
            let _ = crate::services::retention::purge_expired(&state.db, &state.settings).await;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
