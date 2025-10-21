//! Lightweight helpers for writing structured diagnostics into the
//! `event_log` table. The log stream powers the AI debugger UI and the
//! daily digest worker, so keeping the API small and predictable is useful.

use r2d2_sqlite::rusqlite::{params, Connection};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

/// Insert a structured event into the `event_log` table.
///
/// The function accepts optional metadata so that callers can provide
/// machine-readable error codes alongside human-readable explanations.
/// `data` is stored as raw JSON to keep the schema flexible while still
/// allowing downstream analysis.
pub fn log_event(
    conn: &Connection,
    level: &str,
    code: Option<&str>,
    module: &str,
    message: &str,
    explain: Option<&str>,
    data: Option<Value>,
) -> rusqlite::Result<()> {
    let id = Uuid::new_v4().to_string();
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let data_str = data.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO event_log (id, ts, level, code, module, message, explain, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, ts, level, code, module, message, explain, data_str],
    )?;
    Ok(())
}
