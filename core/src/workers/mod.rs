//! Background job execution and helpers for deriving daily workspace digests.
//!
//! Jobs are persisted to the SQLite `jobs` table so that they can be retried
//! and inspected by diagnostic tooling. A lightweight async scheduler polls the
//! queue, executes due jobs on blocking threads, and records structured output
//! for the UI.

use std::sync::Arc;
use std::time::Duration as StdDuration;
//! Synchronous worker implementations invoked from IPC commands.
//!
//! These helpers run inside the same process but are isolated from the UI
//! thread. They return JSON payloads so the frontend can render rich status.

use anyhow::{anyhow, Context, Result};
use log::error;
use r2d2_sqlite::rusqlite::Connection;
use r2d2_sqlite::rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::async_runtime;
use time::macros::format_description;
use time::{Date, Duration as TimeDuration, OffsetDateTime, Time};
use tokio::sync::Notify;
use tokio::task::spawn_blocking;
use tokio::time::interval;
use uuid::Uuid;

use crate::agents::config;
use crate::agents::{AiChatInput, AiChatMessage, AiOrchestrator};
use crate::db::DbPool;
use crate::logging::log_event;

const DAILY_DIGEST_JOB: &str = "workspace.daily_digest";

/// Result payload returned when a worker completes a job.
#[derive(Debug, Clone, Serialize)]
/// Result payload returned when a worker completes a job.
#[derive(Debug, Serialize)]
pub struct JobRunResult {
    pub job_id: String,
    pub kind: String,
    pub state: String,
    pub result: Value,
}

struct PendingJob {
    id: String,
    kind: String,
    payload: Value,
}

/// Cooperative scheduler that executes queued jobs on background threads.
pub struct JobScheduler {
    pool: DbPool,
    ai: Arc<AiOrchestrator>,
    notifier: Arc<Notify>,
}

impl JobScheduler {
    /// Construct a scheduler backed by the provided database pool and AI runtime.
    pub fn new(pool: DbPool, ai: Arc<AiOrchestrator>) -> Arc<Self> {
        let scheduler = Arc::new(Self {
            pool,
            ai,
            notifier: Arc::new(Notify::new()),
        });
        scheduler.spawn_worker();
        scheduler
    }

    fn spawn_worker(self: &Arc<Self>) {
        let runner = Arc::clone(self);
        async_runtime::spawn(async move {
            let mut tick = interval(StdDuration::from_secs(60));
            loop {
                tokio::select! {
                    _ = runner.notifier.notified() => {
                        if let Err(err) = runner.dispatch_due_jobs().await {
                            error!("failed to dispatch queued jobs: {err:?}");
                        }
                    }
                    _ = tick.tick() => {
                        if let Err(err) = runner.dispatch_due_jobs().await {
                            error!("failed to dispatch queued jobs: {err:?}");
                        }
                        if let Err(err) = runner.ensure_nightly_digest_schedule().await {
                            error!("failed to ensure nightly digest schedule: {err:?}");
                        }
                    }
                }
            }
        });
    }

    fn wake(&self) {
        self.notifier.notify_one();
    }

    /// Persist a job and execute it immediately on a worker thread.
    pub async fn run_now(&self, kind: &str, payload: Value) -> Result<JobRunResult> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let job_id = self.persist_job(kind, &payload, Some(now)).await?;
        let result = self
            .run_existing_job(PendingJob {
                id: job_id.clone(),
                kind: kind.to_string(),
                payload,
            })
            .await?;
        let _ = self.ensure_nightly_digest_schedule().await;
        Ok(result)
    }

    /// Blocking helper used by synchronous IPC handlers.
    pub fn run_now_blocking(&self, kind: &str, payload: Value) -> Result<JobRunResult> {
        async_runtime::block_on(self.run_now(kind, payload))
    }

    /// Queue a job for execution at a specific unix timestamp.
    pub async fn enqueue_at(&self, kind: &str, payload: Value, run_at: i64) -> Result<String> {
        let id = self.persist_job(kind, &payload, Some(run_at)).await?;
        self.wake();
        Ok(id)
    }

    /// Blocking variant of [`enqueue_at`].
    pub fn enqueue_at_blocking(&self, kind: &str, payload: Value, run_at: i64) -> Result<String> {
        async_runtime::block_on(self.enqueue_at(kind, payload, run_at))
    }

    /// Ensure a nightly digest job exists for the upcoming 02:00 UTC run.
    pub async fn ensure_nightly_digest_schedule(&self) -> Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            schedule_next_digest(&conn)
        })
        .await??;
        Ok(())
    }

    /// Blocking convenience wrapper for [`ensure_nightly_digest_schedule`].
    pub fn ensure_nightly_digest_schedule_blocking(&self) -> Result<()> {
        async_runtime::block_on(self.ensure_nightly_digest_schedule())
    }

    async fn dispatch_due_jobs(self: &Arc<Self>) -> Result<()> {
        let jobs = self.fetch_due_jobs().await?;
        for job in jobs {
            if let Err(err) = self.run_existing_job(job).await {
                error!("job execution failed: {err:?}");
            }
        }
        Ok(())
    }

    async fn fetch_due_jobs(&self) -> Result<Vec<PendingJob>> {
        let pool = self.pool.clone();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, kind, payload FROM jobs WHERE state='queued' AND (run_at IS NULL OR run_at <= ?1) ORDER BY run_at IS NULL DESC, run_at ASC, created_at ASC",
            )?;
            let rows = stmt.query_map([now], |row| {
                let payload_json: String = row.get(2)?;
                let payload = serde_json::from_str(&payload_json).unwrap_or_else(|_| json!({}));
                Ok(PendingJob {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    payload,
                })
            })?;
            let mut pending = Vec::new();
            for row in rows {
                pending.push(row?);
            }
            Ok(pending)
        })
        .await??
    }

    async fn run_existing_job(&self, job: PendingJob) -> Result<JobRunResult> {
        let pool = self.pool.clone();
        let ai = Arc::clone(&self.ai);
        spawn_blocking(move || {
            let conn = pool.get()?;
            run_job(&conn, ai.as_ref(), &job.id, &job.kind, job.payload)
        })
        .await??
    }

    async fn persist_job(
        &self,
        kind: &str,
        payload: &Value,
        run_at: Option<i64>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let kind = kind.to_string();
        let payload = payload.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            persist_job_with_conn(&conn, &kind, &payload, run_at)
        })
        .await??
    }
}

fn persist_job_with_conn(
    conn: &Connection,
    kind: &str,
    payload: &Value,
    run_at: Option<i64>,
) -> Result<String> {
/// Persist a job row and immediately execute it.
pub fn enqueue_job(conn: &Connection, kind: &str, payload: Value) -> Result<JobRunResult> {
    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO jobs (id, kind, state, payload, created_at, updated_at, run_at) VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6)",
        params![id.as_str(), kind, payload.to_string(), now, now, run_at],
    )
    .with_context(|| format!("failed to enqueue job {kind}"))?;
    Ok(id)
}

/// Run a job and update its persisted state transitions.
fn run_job(
    conn: &Connection,
    ai: &AiOrchestrator,
    id: &str,
    kind: &str,
    payload: Value,
) -> Result<JobRunResult> {
fn run_job(conn: &Connection, id: &str, kind: &str, payload: Value) -> Result<JobRunResult> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "UPDATE jobs SET state='running', updated_at=?2 WHERE id=?1",
        params![id, now],
    )
    .with_context(|| format!("failed to update job {kind} to running"))?;

    let result = match kind {
        DAILY_DIGEST_JOB => perform_daily_digest(conn, ai, &payload),
        other => Err(anyhow!("unknown job kind: {other}")),
    };

    match result {
        Ok(value) => {
            let finished = OffsetDateTime::now_utc().unix_timestamp();
            conn.execute(
                "UPDATE jobs SET state='succeeded', result=?2, updated_at=?3 WHERE id=?1",
                params![id, value.to_string(), finished],
            )
            .with_context(|| format!("failed to mark job {kind} as succeeded"))?;
            Ok(JobRunResult {
                job_id: id.to_string(),
                kind: kind.to_string(),
                state: "succeeded".into(),
                result: value,
            })
        }
        Err(error) => {
            let finished = OffsetDateTime::now_utc().unix_timestamp();
            let message = error.to_string();
            conn.execute(
                "UPDATE jobs SET state='failed', result=?2, updated_at=?3 WHERE id=?1",
                params![id, message.as_str(), finished],
            )
            .with_context(|| format!("failed to mark job {kind} as failed"))?;
            Err(error)
        }
    }
}

/// Generate the logbook summary and timeline entries for a given day.
fn perform_daily_digest(conn: &Connection, ai: &AiOrchestrator, payload: &Value) -> Result<Value> {
fn perform_daily_digest(conn: &Connection, payload: &Value) -> Result<Value> {
    let date = resolve_entry_date(payload)?;
    let date_key = date.to_string();
    let start_ts = date
        .with_time(Time::MIDNIGHT)
        .context("failed to derive midnight for date")?
        .assume_utc()
        .unix_timestamp();
    let end_ts =
        (OffsetDateTime::from_unix_timestamp(start_ts)? + TimeDuration::DAY).unix_timestamp();

    let notes_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM notes WHERE created_at >= ?1 AND created_at < ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )
        .context("failed to count notes for logbook digest")?;

    let latest_note: Option<(String, i64)> = conn
        .prepare(
            "SELECT title, created_at FROM notes WHERE created_at >= ?1 AND created_at < ?2 ORDER BY created_at DESC LIMIT 1",
        )?
        .query_row(params![start_ts, end_ts], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?;

    let ai_calls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM event_log WHERE module = 'ai.runtime' AND ts >= ?1 AND ts < ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )
        .context("failed to count AI interactions")?;

    let ai_failures: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM event_log WHERE module = 'ai.runtime' AND level IN ('error', 'warn') AND ts >= ?1 AND ts < ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )
        .context("failed to count AI failures")?;

    let job_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE created_at >= ?1 AND created_at < ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )
        .context("failed to count job executions")?;

    let note_excerpts = collect_note_excerpts(conn, start_ts, end_ts)?;

    let mut summary_parts = Vec::new();
    summary_parts.push(format!(
        "Captured {notes_count} note{} today.",
        plural(notes_count)
    ));
    summary_parts.push(format!(
        "Dispatched {ai_calls} AI run{} with {ai_failures} incident{}.",
        plural(ai_calls),
        ai_failures,
        plural(ai_failures)
    ));
    summary_parts.push(format!(
        "Processed {job_count} background job{}.",
        plural(job_count)
    ));

    if let Some((title, ts)) = &latest_note {
        let when = OffsetDateTime::from_unix_timestamp(*ts)?
            .format(&format_description!("[hour]:[minute] UTC"))?;
        summary_parts.push(format!("Latest note \"{}\" captured at {}.", title, when));
    }

    let fallback_summary = summary_parts.join(" ");

    let facts = DailyDigestFacts {
        date_key: date_key.clone(),
        notes_count,
        ai_calls,
        ai_failures,
        job_count,
        latest_note: latest_note.clone(),
        note_excerpts,
    };

    let ai_summary = generate_ai_summary(conn, ai, &facts)?;
    let summary = ai_summary.unwrap_or(fallback_summary);

    let logbook_entry = upsert_logbook_entry(conn, &date_key, &summary)?;
    let timeline = rebuild_timeline(
        conn,
        &date_key,
        &summary,
        notes_count,
        ai_calls,
        ai_failures,
    )?;

    log_event(
        conn,
        "info",
        Some("SYS-LOG-100"),
        "jobs.daily",
        "Daily digest job completed",
        Some("Created or refreshed logbook and timeline entries."),
        Some(json!({ "entry_date": date_key })),
    )
    .context("failed to log daily digest completion")?;

    Ok(json!({
        "entry_date": date.to_string(),
        "logbook": logbook_entry,
        "timeline": timeline,
    }))
}

struct DailyDigestFacts {
    date_key: String,
    notes_count: i64,
    ai_calls: i64,
    ai_failures: i64,
    job_count: i64,
    latest_note: Option<(String, i64)>,
    note_excerpts: Vec<NoteExcerpt>,
}

struct NoteExcerpt {
    title: String,
    preview: String,
}

fn collect_note_excerpts(
    conn: &Connection,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<NoteExcerpt>> {
    let mut stmt = conn.prepare(
        "SELECT title, body FROM notes WHERE created_at >= ?1 AND created_at < ?2 ORDER BY created_at DESC LIMIT 5",
    )?;
    let rows = stmt.query_map(params![start_ts, end_ts], |row| {
        let body: String = row.get(1)?;
        let preview: String = body.chars().take(240).collect();
        Ok(NoteExcerpt {
            title: row.get(0)?,
            preview: preview.replace('\n', " "),
        })
    })?;
    let mut excerpts = Vec::new();
    for row in rows {
        excerpts.push(row?);
    }
    Ok(excerpts)
}

fn generate_ai_summary(
    conn: &Connection,
    ai: &AiOrchestrator,
    facts: &DailyDigestFacts,
) -> Result<Option<String>> {
    let selection = match config::resolve_runtime(conn, None, None) {
        Ok(selection) => selection,
        Err(err) => {
            let _ = log_event(
                conn,
                "warn",
                Some("AI-DIGEST-001"),
                "jobs.daily",
                "AI digest unavailable",
                Some("No active AI provider is configured for summaries."),
                Some(json!({ "error": err.to_string() })),
            );
            return Ok(None);
        }
    };

    let mut lines = Vec::new();
    lines.push(format!("Date: {}", facts.date_key));
    lines.push(format!("Notes captured: {}", facts.notes_count));
    lines.push(format!(
        "AI runs: {} ({} alerts)",
        facts.ai_calls, facts.ai_failures
    ));
    lines.push(format!("Jobs processed: {}", facts.job_count));
    if let Some((title, ts)) = &facts.latest_note {
        if let Ok(when) = OffsetDateTime::from_unix_timestamp(*ts) {
            if let Ok(formatted) = when.format(&format_description!("[hour]:[minute] UTC")) {
                lines.push(format!("Latest note: \"{}\" at {}", title, formatted));
            }
        }
    }
    if !facts.note_excerpts.is_empty() {
        lines.push(String::from("Recent note highlights:"));
        for note in &facts.note_excerpts {
            lines.push(format!("- {} — {}", note.title, note.preview));
        }
    }

    let system_prompt = "You are InkOS' nightly curator. Craft a concise (3-4 sentence) operational digest highlighting the most meaningful activity for the day. Blend factual counts with qualitative insight. Avoid markdown lists and keep the tone warm and professional.";

    let input = AiChatInput {
        messages: vec![
            AiChatMessage {
                role: "system".into(),
                content: system_prompt.into(),
            },
            AiChatMessage {
                role: "user".into(),
                content: lines.join("\n"),
            },
        ],
        temperature: Some(0.25),
    };

    match async_runtime::block_on(ai.chat(&selection, input)) {
        Ok(response) => {
            let summary = response.content.trim().to_string();
            if summary.is_empty() {
                let _ = log_event(
                    conn,
                    "warn",
                    Some("AI-DIGEST-002"),
                    "jobs.daily",
                    "AI digest returned empty content",
                    Some("Falling back to deterministic summary."),
                    Some(json!({ "provider": selection.provider.id })),
                );
                Ok(None)
            } else {
                let _ = log_event(
                    conn,
                    "info",
                    Some("AI-DIGEST-200"),
                    "jobs.daily",
                    "AI digest generated",
                    Some("Nightly summary authored by configured AI runtime."),
                    Some(json!({
                        "provider": selection.provider.id,
                        "model": selection.model,
                    })),
                );
                Ok(Some(summary))
            }
        }
        Err(err) => {
            let _ = log_event(
                conn,
                "warn",
                Some("AI-DIGEST-100"),
                "jobs.daily",
                "AI digest request failed",
                Some("Falling back to deterministic summary."),
                Some(json!({
                    "provider": selection.provider.id,
                    "model": selection.model,
                    "error": err.to_string(),
                })),
            );
            Ok(None)
        }
    }
}

fn schedule_next_digest(conn: &Connection) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    let target_time =
        Time::from_hms(2, 0, 0).context("failed to construct digest schedule time")?;
    let mut next_run = now
        .date()
        .with_time(target_time)
        .context("failed to derive next digest timestamp")?
        .assume_utc();
    if now >= next_run {
        next_run += TimeDuration::DAY;
    }
    let digest_date = (next_run - TimeDuration::DAY).date().to_string();
    let run_at_ts = next_run.unix_timestamp();

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM jobs WHERE kind = ?1 AND state = 'queued' AND run_at = ?2 LIMIT 1",
            params![DAILY_DIGEST_JOB, run_at_ts],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(());
    }

    let payload = json!({ "date": digest_date });
    let id = persist_job_with_conn(conn, DAILY_DIGEST_JOB, &payload, Some(run_at_ts))?;
    let _ = log_event(
        conn,
        "info",
        Some("JOB-200"),
        "jobs.scheduler",
        "Scheduled nightly digest job",
        Some("Will summarise the previous day at 02:00 UTC."),
        Some(json!({
            "job_id": id,
            "run_at": run_at_ts,
            "payload": payload,
        })),
    );
    Ok(())
}

/// Insert or update the daily logbook entry for `entry_date`.
fn upsert_logbook_entry(conn: &Connection, entry_date: &str, summary: &str) -> Result<Value> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let existing: Option<(String, i64)> = conn
        .prepare("SELECT id, created_at FROM logbook_entries WHERE entry_date = ?1")?
        .query_row(params![entry_date], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;

    let entry_id = if let Some((id, _)) = existing {
        conn.execute(
            "UPDATE logbook_entries SET summary = ?2 WHERE id = ?1",
            params![id, summary],
        )
        .context("failed to update existing logbook entry")?;
        id
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO logbook_entries (id, entry_date, summary, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, entry_date, summary, now],
        )
        .context("failed to insert logbook entry")?;
        id
    };

    let (created_at,): (i64,) = conn
        .query_row(
            "SELECT created_at FROM logbook_entries WHERE id = ?1",
            params![entry_id],
            |row| Ok((row.get(0)?,)),
        )
        .context("failed to reload logbook entry")?;

    Ok(json!({
        "id": entry_id,
        "entry_date": entry_date,
        "summary": summary,
        "created_at": created_at,
    }))
}

/// Recreate the derived timeline events backing the daily digest view.
fn rebuild_timeline(
    conn: &Connection,
    entry_date: &str,
    summary: &str,
    notes_count: i64,
    ai_calls: i64,
    ai_failures: i64,
) -> Result<Value> {
    conn.execute(
        "DELETE FROM timeline_events WHERE entry_date = ?1",
        params![entry_date],
    )
    .context("failed to clear previous timeline events")?;

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut events = Vec::new();

    events.push(create_timeline_event(
        conn,
        entry_date,
        now,
        "logbook",
        format!("Daily log captured ({entry_date})"),
        summary.to_string(),
    )?);

    if notes_count > 0 {
        events.push(create_timeline_event(
            conn,
            entry_date,
            now,
            "notes",
            format!("{} new note{}", notes_count, plural(notes_count)),
            "Review the Notes tab to explore today's captures.".to_string(),
        )?);
    }

    if ai_calls > 0 {
        events.push(create_timeline_event(
            conn,
            entry_date,
            now,
            "ai",
            format!("{} AI interaction{}", ai_calls, plural(ai_calls)),
            "Inspect the AI Debugger console for transcripts and usage.".to_string(),
        )?);
    }

    if ai_failures > 0 {
        events.push(create_timeline_event(
            conn,
            entry_date,
            now,
            "alerts",
            format!("{} AI alert{}", ai_failures, plural(ai_failures)),
            "Errors were detected in today's AI runs. Investigate via the debugger.".to_string(),
        )?);
    }

    Ok(Value::Array(events))
}

/// Persist a single timeline event and return its serialised form.
fn create_timeline_event(
    conn: &Connection,
    entry_date: &str,
    event_time: i64,
    kind: &str,
    title: String,
    detail: String,
) -> Result<Value> {
    let id = Uuid::new_v4().to_string();
    let created_at = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO timeline_events (id, entry_date, event_time, kind, title, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, entry_date, event_time, kind, title, detail, created_at],
    )
    .context("failed to insert timeline event")?;

    Ok(json!({
        "id": id,
        "entry_date": entry_date,
        "event_time": event_time,
        "kind": kind,
        "title": title,
        "detail": detail,
        "created_at": created_at,
    }))
}

/// Resolve the target date for a digest run, defaulting to today.
fn resolve_entry_date(payload: &Value) -> Result<Date> {
    if let Some(date_str) = payload.get("date").and_then(Value::as_str) {
        Date::parse(date_str, &format_description!("[year]-[month]-[day]"))
            .context("invalid date supplied to daily digest job")
    } else {
        Ok(OffsetDateTime::now_utc().date())
    }
}

/// Helper for English pluralisation of counts.
fn plural(count: i64) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2_sqlite::rusqlite::Connection as SqliteConnection;

    #[test]
    fn pluralises_correctly() {
        assert_eq!(plural(0), "s");
        assert_eq!(plural(1), "");
        assert_eq!(plural(2), "s");
    }

    #[test]
    fn resolve_entry_date_defaults_to_today() {
        let today = OffsetDateTime::now_utc().date();
        let resolved = resolve_entry_date(&json!({})).unwrap();
        assert_eq!(resolved, today);
    }

    #[test]
    fn resolve_entry_date_parses_explicit_string() {
        let resolved = resolve_entry_date(&json!({ "date": "2024-01-05" })).unwrap();
        assert_eq!(resolved.to_string(), "2024-01-05");
    }

    #[test]
    fn rebuild_timeline_generates_entries() {
        let conn = SqliteConnection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE timeline_events (id TEXT PRIMARY KEY, entry_date TEXT, event_time INTEGER, kind TEXT, title TEXT, detail TEXT, created_at INTEGER);",
        )
        .unwrap();

        let events = rebuild_timeline(&conn, "2024-01-05", "summary", 2, 1, 0).unwrap();
        let array = events.as_array().unwrap();
        assert!(array.len() >= 2);
    }
}
