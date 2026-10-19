
//! Redis-backed delayed job queue for the sync worker.
//!
//! Python used arq; both ends here are Rust, so this is a small sorted-set queue keyed by
//! ready-time. Tasks are `{"task": "...", "arg": "..."}` members scored by ready-at millis.

use redis::AsyncCommands;
use serde_json::json;

pub const PROCESS_WEBHOOK_EVENT: &str = "process_webhook_event";
pub const RUN_SYNC_JOB: &str = "run_sync_job";
pub const RECONCILE_RULE: &str = "reconcile_rule";

const QUEUE_KEY: &str = "portal:sync_jobs";

/// One queued task. `as_of` is set on the follow-ups a sync job schedules for itself (a readiness
/// re-check or a retry backoff): the job's `updated_at` when that attempt was recorded, so a
/// superseded follow-up can tell it is stale (see `sync::worker::run_sync_job`).
#[derive(Debug, PartialEq)]
pub struct Task {
    pub task: String,
    pub arg: String,
    pub as_of: Option<String>,
}

fn member(task: &str, arg: &str, as_of: Option<&str>) -> String {
    match as_of {
        None => json!({ "task": task, "arg": arg }).to_string(),
        Some(a) => json!({ "task": task, "arg": arg, "as_of": a }).to_string(),
    }
}

fn parse_member(m: &str) -> Option<Task> {
    let v = serde_json::from_str::<serde_json::Value>(m).ok()?;
    Some(Task {
        task: v.get("task")?.as_str()?.to_string(),
        arg: v.get("arg")?.as_str()?.to_string(),
        as_of: v.get("as_of").and_then(|x| x.as_str()).map(str::to_string),
    })
}

pub async fn enqueue(
    redis: &mut redis::aio::ConnectionManager,
    task: &str,
    arg: &str,
    delay_secs: f64,
) -> Result<(), redis::RedisError> {
    enqueue_as_of(redis, task, arg, None, delay_secs).await
}

pub async fn enqueue_as_of(
    redis: &mut redis::aio::ConnectionManager,
    task: &str,
    arg: &str,
    as_of: Option<&str>,
    delay_secs: f64,
) -> Result<(), redis::RedisError> {
    let ready_at_ms = chrono::Utc::now().timestamp_millis() as f64 + delay_secs * 1000.0;
    let _: () = redis.zadd(QUEUE_KEY, member(task, arg, as_of), ready_at_ms).await?;
    Ok(())
}

/// Pop every job due at or before now (up to `max`).
pub async fn dequeue_due(
    redis: &mut redis::aio::ConnectionManager,
    max: usize,
) -> Result<Vec<Task>, redis::RedisError> {
    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
    let members: Vec<String> = redis
        .zrangebyscore_limit(QUEUE_KEY, "-inf", now_ms, 0, max as isize)
        .await?;
    let mut out = Vec::new();
    for m in members {
        let removed: i64 = redis.zrem(QUEUE_KEY, &m).await?;
        if removed == 0 {
            continue; // another consumer took it
        }
        if let Some(t) = parse_member(&m) {
            out.push(t);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_round_trips_with_and_without_as_of() {
        let fresh = parse_member(&member(RUN_SYNC_JOB, "j1", None)).unwrap();
        assert_eq!(fresh, Task { task: RUN_SYNC_JOB.into(), arg: "j1".into(), as_of: None });
        let follow = parse_member(&member(RUN_SYNC_JOB, "j1", Some("2026-10-19T00:00:00.000001Z"))).unwrap();
        assert_eq!(follow.as_of.as_deref(), Some("2026-10-19T00:00:00.000001Z"));
        // A fresh request and a follow-up for the same job are distinct members, so queueing one
        // never re-scores (and so delays or advances) the other.
        assert_ne!(member(RUN_SYNC_JOB, "j1", None), member(RUN_SYNC_JOB, "j1", Some("x")));
    }

    #[test]
    fn members_from_before_as_of_still_parse() {
        let old = r#"{"task":"run_sync_job","arg":"j1"}"#;
        assert_eq!(parse_member(old).unwrap().as_of, None);
    }
}
