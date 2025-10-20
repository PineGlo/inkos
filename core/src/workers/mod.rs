use anyhow::{anyhow, Context, Result};
use r2d2_sqlite::rusqlite::Connection;
use r2d2_sqlite::rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime, Time};
use uuid::Uuid;

use crate::logging::log_event;

#[derive(Debug, Serialize)]
pub struct JobRunResult {
    pub job_id: String,
    pub kind: String,
    pub state: String,
    pub result: Value,
}

const DAILY_DIGEST_JOB: &str = "workspace.daily_digest";

pub fn enqueue_job(conn: &Connection, kind: &str, payload: Value) -> Result<JobRunResult> {
    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO jobs (id, kind, state, payload, created_at, updated_at) VALUES (?1, ?2, 'queued', ?3, ?4, ?5)",
        (id.as_str(), kind, payload.to_string(), now, now),
    )
    .with_context(|| format!("failed to enqueue job {kind}"))?;

    run_job(conn, &id, kind, payload)
}

fn run_job(conn: &Connection, id: &str, kind: &str, payload: Value) -> Result<JobRunResult> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "UPDATE jobs SET state='running', updated_at=?2 WHERE id=?1",
        params![id, now],
    )
    .with_context(|| format!("failed to update job {kind} to running"))?;

    let result = match kind {
        DAILY_DIGEST_JOB => perform_daily_digest(conn, &payload),
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

fn perform_daily_digest(conn: &Connection, payload: &Value) -> Result<Value> {
    let date = resolve_entry_date(payload)?;
    let date_key = date.to_string();
    let start_ts = date
        .with_time(Time::MIDNIGHT)
        .context("failed to derive midnight for date")?
        .assume_utc()
        .unix_timestamp();
    let end_ts = (OffsetDateTime::from_unix_timestamp(start_ts)? + Duration::DAY).unix_timestamp();

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

    if let Some((title, ts)) = latest_note {
        let when = OffsetDateTime::from_unix_timestamp(ts)?
            .format(&format_description!("[hour]:[minute] UTC"))?;
        summary_parts.push(format!("Latest note \"{}\" captured at {}.", title, when));
    }

    let summary = summary_parts.join(" ");

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
        "entry_date": date_key,
        "logbook": logbook_entry,
        "timeline": timeline,
    }))
}

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

fn resolve_entry_date(payload: &Value) -> Result<Date> {
    if let Some(date_str) = payload.get("date").and_then(Value::as_str) {
        Date::parse(date_str, &format_description!("[year]-[month]-[day]"))
            .context("invalid date supplied to daily digest job")
    } else {
        Ok(OffsetDateTime::now_utc().date())
    }
}

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
