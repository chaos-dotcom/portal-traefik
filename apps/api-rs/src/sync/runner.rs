
//! Sync job execution + reconciliation — port of `portal.sync.runner`.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::config::Settings;
use crate::frameio::FrameioError;
use crate::storage::frameio_backend::FrameioStorageBackend;
use crate::storage::{DestinationConfig, RemoteFile};
use crate::sync::destination::{arm_destination, check_destination};
use crate::sync::download::download_to_file;
use crate::sync::paths::{resolve_conflict, resolve_destination, PathContext};

pub const DOWNLOADABLE_STATUSES: [&str; 2] = ["uploaded", "transcoded"];

/// Error recorded on a job skipped because its file isn't inside the rule's folder. Not final:
/// files and folders get moved, so reconcile re-runs such a job once the file shows up in the
/// rule's own listing.
pub const OUT_OF_SCOPE: &str = "outside the rule's source folder";

#[derive(Debug)]
pub enum JobError {
    SourceNotReady(String),
    DestinationUnavailable(String),
    Frameio(FrameioError),
    Other(String),
}

/// The text recorded on a failed job, like Python's `_unwrap_error` (`"Type: message"`), so the
/// job list shows the real cause rather than an internal enum dump.
impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::SourceNotReady(m) => write!(f, "SourceNotReady: {m}"),
            JobError::DestinationUnavailable(m) => write!(f, "DestinationUnavailable: {m}"),
            JobError::Frameio(e) => write!(f, "FrameioError: {e}"),
            JobError::Other(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Clone)]
pub struct SyncJobRow {
    pub id: Uuid,
    pub sync_rule_id: Uuid,
    pub frameio_file_id: String,
    pub status: String,
    pub bytes: Option<i64>,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub retry_count: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub source_wait_started_at: Option<DateTime<Utc>>,
}

pub struct SyncRuleRow {
    pub id: Uuid,
    pub destination_path: String,
    pub conflict_policy: String,
    pub path_template: Option<String>,
    pub source_config: serde_json::Value,
    pub frameio_webhook_id: Option<String>,
    pub enabled: bool,
}

pub async fn load_rule(db: &PgPool, id: Uuid) -> sqlx::Result<Option<SyncRuleRow>> {
    let row = sqlx::query(
        "SELECT id, destination_path, conflict_policy, path_template, source_config,
                frameio_webhook_id, enabled FROM sync_rules WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| SyncRuleRow {
        id: r.get("id"),
        destination_path: r.get("destination_path"),
        conflict_policy: r.get("conflict_policy"),
        path_template: r.get("path_template"),
        source_config: r.get("source_config"),
        frameio_webhook_id: r.get("frameio_webhook_id"),
        enabled: r.get("enabled"),
    }))
}

pub async fn load_job(db: &PgPool, id: Uuid) -> sqlx::Result<Option<SyncJobRow>> {
    let row = sqlx::query(
        "SELECT id, sync_rule_id, frameio_file_id, status, bytes, sha256, error, retry_count,
                started_at, completed_at, updated_at, source_wait_started_at
           FROM sync_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| SyncJobRow {
        id: r.get("id"),
        sync_rule_id: r.get("sync_rule_id"),
        frameio_file_id: r.get("frameio_file_id"),
        status: r.get("status"),
        bytes: r.get("bytes"),
        sha256: r.get("sha256"),
        error: r.get("error"),
        retry_count: r.get("retry_count"),
        started_at: r.get("started_at"),
        completed_at: r.get("completed_at"),
        updated_at: r.get("updated_at"),
        source_wait_started_at: r.get("source_wait_started_at"),
    }))
}

pub async fn save_job(db: &PgPool, job: &SyncJobRow) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE sync_jobs SET status=$1, bytes=$2, sha256=$3, error=$4, retry_count=$5,
            started_at=$6, completed_at=$7, source_wait_started_at=$8, updated_at=now()
         WHERE id=$9",
    )
    .bind(&job.status)
    .bind(job.bytes)
    .bind(&job.sha256)
    .bind(&job.error)
    .bind(job.retry_count)
    .bind(job.started_at)
    .bind(job.completed_at)
    .bind(job.source_wait_started_at)
    .bind(job.id)
    .execute(db)
    .await?;
    Ok(())
}

pub fn destination(rule: &SyncRuleRow) -> DestinationConfig {
    let t = rule.source_config.get("type").and_then(|v| v.as_str()).unwrap_or("frameio");
    DestinationConfig::new(t, rule.source_config.clone())
}

pub fn is_orphaned(job: &SyncJobRow, now: DateTime<Utc>, stale_after_seconds: i64) -> bool {
    if job.status != "running" {
        return false;
    }
    match job.started_at {
        Some(started) => (now - started).num_seconds() > stale_after_seconds,
        None => false,
    }
}

/// A "pending" job waits on a queued task — its first run, or the retry its last failure
/// scheduled. Untouched for longer than any such task takes to come due, that task was lost (the
/// worker restarted before picking it up, or the queued task expired), and nothing else will ever
/// run the job: the file strands forever. Measured from `updated_at` (bumped whenever the row is
/// touched) so a job re-enqueued moments ago isn't immediately re-queued again.
pub fn is_stranded_pending(job: &SyncJobRow, now: DateTime<Utc>, stale_after_seconds: i64) -> bool {
    if job.status != "pending" {
        return false;
    }
    (now - job.updated_at).num_seconds() > stale_after_seconds
}

/// A "waiting" job is re-checked every `sync_source_poll_seconds` and each check touches the row,
/// so one untouched for far longer has lost its scheduled re-check (the queued task was dropped,
/// or the run that superseded it died) and would otherwise wait forever.
pub fn is_stranded_waiting(job: &SyncJobRow, now: DateTime<Utc>, stale_after_seconds: i64) -> bool {
    if job.status != "waiting" {
        return false;
    }
    (now - job.updated_at).num_seconds() > stale_after_seconds
}

/// A non-recursive rule only takes files directly in its folder; a recursive one takes files
/// anywhere beneath it. Webhooks and upload triggers match a whole project, so this is the check
/// that keeps a sibling folder (e.g. OUT next to a rule on IN) from syncing into the destination:
/// `relative_dir` is None when the file's ancestry never reaches the rule's folder.
fn in_scope(source_config: &serde_json::Value, remote: &RemoteFile) -> bool {
    if remote.relative_dir.is_none() {
        return false;
    }
    let recursive = source_config.get("recursive").and_then(|v| v.as_bool()).unwrap_or(true);
    if recursive {
        return true;
    }
    remote.parent_id.as_deref() == source_config.get("folder_id").and_then(|v| v.as_str())
}

/// The reconcile thresholds for (pending, waiting) jobs that lost their queued task: comfortably
/// past any live task's due time (the longest retry backoff, the re-check cadence).
pub fn stranded_thresholds(settings: &Settings) -> (i64, i64) {
    let stale_floor = settings.sync_pending_stale_seconds;
    let longest_backoff = settings.sync_retry_base_seconds * 2i64.pow((settings.sync_max_retries - 2).max(0) as u32);
    let pending_stale_after = stale_floor.max(3 * longest_backoff);
    let waiting_stale_after = stale_floor.max(3 * settings.sync_source_poll_seconds);
    (pending_stale_after, waiting_stale_after)
}

/// Run one sync job to completion, setting its terminal fields.
pub async fn execute_job(
    db: &PgPool,
    job: &mut SyncJobRow,
    backend: &FrameioStorageBackend,
    http: &reqwest::Client,
    settings: &Settings,
) -> Result<(), JobError> {
    let Some(rule) = load_rule(db, job.sync_rule_id).await.map_err(|e| JobError::Other(e.to_string()))? else {
        return Err(JobError::Other(format!("sync rule {} is gone", job.sync_rule_id)));
    };

    let armed: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sync_jobs WHERE sync_rule_id = $1 AND status = 'done' LIMIT 1",
    )
    .bind(rule.id)
    .fetch_optional(db)
    .await
    .unwrap_or(None);
    if let Err(e) = check_destination(&rule.destination_path, armed.is_some()) {
        return Err(JobError::DestinationUnavailable(e.to_string()));
    }

    let dest = destination(&rule);
    let remote = match backend.get_file(&dest, &job.frameio_file_id).await {
        Ok(f) => f,
        Err(FrameioError::NotFound) => {
            job.status = "skipped".to_string();
            job.completed_at = Some(Utc::now());
            job.error = Some("source file no longer exists on Frame.io".to_string());
            return Ok(());
        }
        Err(e) => return Err(JobError::Frameio(e)),
    };

    // Name the job after its file (jobs made from a webhook only know the file's id).
    let _ = sqlx::query("UPDATE sync_jobs SET file_name = $1 WHERE id = $2 AND file_name IS DISTINCT FROM $1")
        .bind(display_name(&remote))
        .bind(job.id)
        .execute(db)
        .await;

    // Scope first: a file outside the rule's folder is never synced, so don't wait on it.
    if !in_scope(&rule.source_config, &remote) {
        job.status = "skipped".to_string();
        job.completed_at = Some(Utc::now());
        job.error = Some(OUT_OF_SCOPE.to_string());
        tracing::info!("sync.job.skipped_out_of_scope job_id={} file_id={}", job.id, job.frameio_file_id);
        return Ok(());
    }

    // Wait for the original to be downloadable. Frame.io reports a status; if it's not yet a
    // downloadable one, defer (the worker re-queues patiently without burning the retry budget).
    if let Some(status) = &remote.status
        && !DOWNLOADABLE_STATUSES.contains(&status.as_str()) {
            return Err(JobError::SourceNotReady(format!("file status is '{status}', not yet downloadable")));
        }

    let ctx = PathContext {
        filename: remote.name.clone(),
        project: rule.source_config.get("project_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        folder: rule.source_config.get("folder_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        subfolder: remote.relative_dir.clone().unwrap_or_default(),
        uploader_name: String::new(),
        when: Some(chrono::Local::now()),
    };
    let target = resolve_destination(&rule.destination_path, rule.path_template.as_deref(), &ctx);
    let Some(final_path) = resolve_conflict(&target, &rule.conflict_policy) else {
        job.status = "skipped".to_string();
        job.completed_at = Some(Utc::now());
        return Ok(());
    };

    let download = match backend.get_download_url(&dest, &job.frameio_file_id).await {
        Ok(d) => d,
        Err(FrameioError::Forbidden) => {
            return Err(JobError::SourceNotReady("original not downloadable yet (403)".into()))
        }
        Err(e) => return Err(JobError::Frameio(e)),
    };

    let mut tmp = final_path.clone();
    let name = final_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    tmp.set_file_name(format!("{name}.part"));
    let result = download_to_file(http, &download.url, remote.size, &tmp, settings.sync_download_concurrency).await;
    let result = match result {
        Ok(r) => {
            std::fs::rename(&tmp, &final_path).map_err(|e| JobError::Other(e.to_string()))?;
            r
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(JobError::Other(e.to_string()));
        }
    };

    job.status = "done".to_string();
    job.bytes = Some(result.bytes_written);
    job.sha256 = Some(result.sha256.clone());
    job.completed_at = Some(Utc::now());
    job.error = None;
    arm_destination(&rule.destination_path);
    Ok(())
}

/// The name a job is shown by: the file's path below the rule's folder, e.g. "Dailies/A001.mov".
fn display_name(f: &RemoteFile) -> String {
    match f.relative_dir.as_deref() {
        Some(dir) if !dir.is_empty() => format!("{dir}/{}", f.name),
        _ => f.name.clone(),
    }
}

/// Re-pick every job left `running` (worker died mid-download) and re-enqueue it.
pub async fn reclaim_orphaned_running(db: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    let rows = sqlx::query("SELECT id FROM sync_jobs WHERE status = 'running'")
        .fetch_all(db)
        .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.get("id")).collect();
    for id in &ids {
        sqlx::query(
            "UPDATE sync_jobs SET status='pending', started_at=NULL, completed_at=NULL, error=NULL, updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .execute(db)
        .await?;
    }
    Ok(ids)
}

/// Create jobs for files in a rule's folder that lack one (and reset ones that need another go).
/// Returns the ids to enqueue.
pub async fn reconcile_rule(
    db: &PgPool,
    rule_id: Uuid,
    backend: &FrameioStorageBackend,
    settings: &Settings,
) -> Result<Vec<Uuid>, JobError> {
    let Some(rule) = load_rule(db, rule_id).await.map_err(|e| JobError::Other(e.to_string()))? else {
        return Ok(Vec::new());
    };
    if !rule.enabled {
        return Ok(Vec::new());
    }
    let dest = destination(&rule);
    let recursive = rule.source_config.get("recursive").and_then(|v| v.as_bool()).unwrap_or(true);
    let files = if recursive {
        backend.list_files_recursive(&dest).await.map_err(JobError::Frameio)?
    } else {
        backend.list_files_in_destination(&dest).await.map_err(JobError::Frameio)?
    };

    let now = Utc::now();
    let stale_after = settings.sync_job_timeout_seconds;
    let (pending_stale_after, waiting_stale_after) = stranded_thresholds(settings);
    // Returned to the caller, which enqueues them: every write above has committed by then, so a
    // worker never picks up a task for a row it can't see yet.
    let mut created: Vec<Uuid> = Vec::new();

    for f in files {
        let existing = sqlx::query(
            "SELECT id, status, error, started_at, updated_at FROM sync_jobs WHERE sync_rule_id = $1 AND frameio_file_id = $2",
        )
        .bind(rule.id)
        .bind(&f.id)
        .fetch_optional(db)
        .await
        .map_err(|e| JobError::Other(e.to_string()))?;

        match existing {
            None => {
                let id: Uuid = sqlx::query_scalar(
                    "INSERT INTO sync_jobs (sync_rule_id, frameio_file_id, file_name, status, retry_count) VALUES ($1,$2,$3,'pending',0) RETURNING id",
                )
                .bind(rule.id)
                .bind(&f.id)
                .bind(display_name(&f))
                .fetch_one(db)
                .await
                .map_err(|e| JobError::Other(e.to_string()))?;
                created.push(id);
            }
            Some(row) => {
                let job = SyncJobRow {
                    id: row.get("id"),
                    sync_rule_id: rule.id,
                    frameio_file_id: f.id.clone(),
                    status: row.get("status"),
                    bytes: None,
                    sha256: None,
                    error: row.get("error"),
                    retry_count: 0,
                    started_at: row.get("started_at"),
                    completed_at: None,
                    updated_at: row.get("updated_at"),
                    source_wait_started_at: None,
                };
                // Self-heal: a transient failure (a network blip), a job left "running" by a
                // killed worker, one whose queued task (or re-check) was lost, or a file moved
                // into the folder shouldn't strand a file. Reset + re-attempt.
                if job.status == "dead_letter"
                    || is_orphaned(&job, now, stale_after)
                    || is_stranded_pending(&job, now, pending_stale_after)
                    || is_stranded_waiting(&job, now, waiting_stale_after)
                    // Skipped as outside the rule's folder, yet it's in the rule's listing: it
                    // has been moved in since, so it's due a sync after all.
                    || (job.status == "skipped" && job.error.as_deref() == Some(OUT_OF_SCOPE))
                {
                    sqlx::query(
                        "UPDATE sync_jobs SET status='pending', retry_count=0, error=NULL, started_at=NULL, completed_at=NULL, source_wait_started_at=NULL, updated_at=now() WHERE id=$1",
                    )
                    .bind(job.id)
                    .execute(db)
                    .await
                    .map_err(|e| JobError::Other(e.to_string()))?;
                    created.push(job.id);
                }
            }
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn job(status: &str, updated_ago_s: i64) -> SyncJobRow {
        let now = Utc::now();
        SyncJobRow {
            id: Uuid::nil(),
            sync_rule_id: Uuid::nil(),
            frameio_file_id: "f".into(),
            status: status.into(),
            bytes: None,
            sha256: None,
            error: None,
            retry_count: 0,
            started_at: Some(now),
            completed_at: None,
            updated_at: now - Duration::seconds(updated_ago_s),
            source_wait_started_at: None,
        }
    }

    fn remote(parent: &str, relative_dir: Option<&str>) -> RemoteFile {
        RemoteFile { parent_id: Some(parent.into()), relative_dir: relative_dir.map(str::to_string), ..Default::default() }
    }

    #[test]
    fn scope_excludes_files_outside_the_rule_folder() {
        let recursive = json!({ "folder_id": "IN" });
        let flat = json!({ "folder_id": "IN", "recursive": false });
        // A sibling folder (OUT next to IN): the ancestry walk never reached IN.
        assert!(!in_scope(&recursive, &remote("OUT", None)));
        assert!(!in_scope(&flat, &remote("OUT", None)));
        // Directly in the rule folder, and nested below it.
        assert!(in_scope(&recursive, &remote("IN", Some(""))));
        assert!(in_scope(&recursive, &remote("SUB", Some("Dailies/Monday"))));
        assert!(in_scope(&flat, &remote("IN", Some(""))));
        assert!(!in_scope(&flat, &remote("SUB", Some("Dailies")))); // non-recursive: root only
    }

    #[test]
    fn default_remote_file_sits_in_the_root() {
        assert_eq!(RemoteFile::default().relative_dir.as_deref(), Some(""));
    }

    #[test]
    fn stranded_checks_look_at_status_and_staleness() {
        let now = Utc::now();
        // A pending job counts as stranded once stale, even after an earlier attempt started it.
        assert!(is_stranded_pending(&job("pending", 4000), now, 3600));
        assert!(!is_stranded_pending(&job("pending", 10), now, 3600));
        assert!(!is_stranded_pending(&job("waiting", 4000), now, 3600));
        assert!(is_stranded_waiting(&job("waiting", 4000), now, 3600));
        assert!(!is_stranded_waiting(&job("waiting", 10), now, 3600));
        assert!(!is_stranded_waiting(&job("pending", 4000), now, 3600));
    }

    #[test]
    fn stranded_thresholds_clear_the_longest_live_wait() {
        let mut s = crate::config::Settings::from_env();
        s.sync_pending_stale_seconds = 1800;
        s.sync_retry_base_seconds = 30;
        s.sync_max_retries = 5;
        s.sync_source_poll_seconds = 120;
        // Longest backoff 30 * 2^3 = 240s; floors win at these defaults.
        assert_eq!(stranded_thresholds(&s), (1800, 1800));
        s.sync_retry_base_seconds = 600; // 600 * 8 = 4800 → 3x = 14400
        s.sync_source_poll_seconds = 900; // 3x = 2700
        assert_eq!(stranded_thresholds(&s), (14400, 2700));
    }

    #[test]
    fn job_errors_read_like_python() {
        assert_eq!(JobError::Frameio(FrameioError::NotFound).to_string(), "FrameioError: Frame.io entity not found");
        assert_eq!(JobError::Other("disk full".into()).to_string(), "disk full");
    }
}
