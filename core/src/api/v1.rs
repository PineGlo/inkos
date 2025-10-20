use std::sync::Arc;

use crate::agents::config::{self, AiSettingsUpdate};
use crate::agents::{AiChatInput, AiChatMessage, AiChatResponse, AiOrchestrator};
use crate::db::DbPool;
use crate::logging::log_event;
use crate::workers::{self, JobRunResult};
use r2d2_sqlite::rusqlite::{Connection as SqliteConnection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{async_runtime::spawn_blocking, State};
use time::macros::format_description;
use time::Date;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub db: DbPool,
    pub ai: Arc<AiOrchestrator>,
}

#[tauri::command]
pub fn ping() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "ts": OffsetDateTime::now_utc().unix_timestamp(),
    })
}

#[tauri::command]
pub fn db_status(state: State<ApiState>) -> Result<serde_json::Value, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok(row.get::<_, String>(0)?))
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for r in rows {
        names.push(r.map_err(|e| e.to_string())?);
    }
    Ok(serde_json::json!({ "ok": true, "tables": names }))
}

#[derive(Deserialize)]
pub struct CreateNoteInput {
    pub title: String,
    pub body: Option<String>,
}
#[derive(Serialize)]
pub struct CreateNoteOutput {
    pub id: String,
}

#[tauri::command]
pub fn create_note(
    state: State<ApiState>,
    input: CreateNoteInput,
) -> Result<CreateNoteOutput, String> {
    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let body = input.body.unwrap_or_default();
    let conn = state.db.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO notes (id, title, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        (id.as_str(), input.title.as_str(), body.as_str(), now, now),
    )
    .map_err(|e| e.to_string())?;
    log_event(
        &conn,
        "info",
        Some("NTE-0000"),
        "notes",
        "note created",
        Some("created via IPC"),
        Some(serde_json::json!({ "id": id })),
    )
    .map_err(|e| e.to_string())?;
    Ok(CreateNoteOutput { id })
}

#[derive(Deserialize)]
pub struct ListNotesInput {
    pub q: Option<String>,
}

#[tauri::command]
pub fn list_notes(
    state: State<ApiState>,
    input: Option<ListNotesInput>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    if let Some(i) = input {
        if let Some(q) = i.q {
            let mut stmt = conn.prepare("SELECT id, title, created_at FROM notes WHERE rowid IN (SELECT rowid FROM fts_notes WHERE fts_notes MATCH ?1) ORDER BY created_at DESC").map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([q], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "created_at": row.get::<_, i64>(2)?
                    }))
                })
                .map_err(|e| e.to_string())?;
            for r in rows {
                results.push(r.map_err(|e| e.to_string())?);
            }
            return Ok(results);
        }
    }
    let mut stmt = conn
        .prepare("SELECT id, title, created_at FROM notes ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "created_at": row.get::<_, i64>(2)?
            }))
        })
        .map_err(|e| e.to_string())?;
    for r in rows {
        results.push(r.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[derive(Serialize)]
pub struct LogbookEntry {
    pub id: String,
    pub entry_date: String,
    pub summary: String,
    pub created_at: i64,
}

#[tauri::command]
pub fn list_logbook_entries(
    state: State<ApiState>,
    limit: Option<usize>,
) -> Result<Vec<LogbookEntry>, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    ensure_today_digest(&conn)?;

    let mut entries = Vec::new();
    if let Some(limit) = limit {
        let mut stmt = conn
            .prepare(
                "SELECT id, entry_date, summary, created_at FROM logbook_entries ORDER BY entry_date DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |row| {
                Ok(LogbookEntry {
                    id: row.get(0)?,
                    entry_date: row.get(1)?,
                    summary: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            entries.push(row.map_err(|e| e.to_string())?);
        }
        return Ok(entries);
    }

    let mut stmt = conn
        .prepare("SELECT id, entry_date, summary, created_at FROM logbook_entries ORDER BY entry_date DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LogbookEntry {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                summary: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        entries.push(row.map_err(|e| e.to_string())?);
    }
    Ok(entries)
}

#[derive(Serialize)]
pub struct TimelineEvent {
    pub id: String,
    pub entry_date: String,
    pub event_time: i64,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    pub created_at: i64,
}

#[tauri::command]
pub fn list_timeline_events(
    state: State<ApiState>,
    date: Option<String>,
) -> Result<Vec<TimelineEvent>, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    ensure_today_digest(&conn)?;

    let resolved_date = if let Some(value) = date {
        Date::parse(&value, &format_description!("[year]-[month]-[day]"))
            .map_err(|e| e.to_string())?
    } else {
        OffsetDateTime::now_utc().date()
    };
    let date_key = resolved_date.to_string();

    let mut stmt = conn
        .prepare("SELECT id, entry_date, event_time, kind, title, detail, created_at FROM timeline_events WHERE entry_date = ?1 ORDER BY event_time ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([date_key.as_str()], |row| {
            let detail: Option<String> = row.get(5)?;
            Ok(TimelineEvent {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                event_time: row.get(2)?,
                kind: row.get(3)?,
                title: row.get(4)?,
                detail,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| e.to_string())?);
    }
    Ok(events)
}

#[derive(Serialize)]
pub struct AiRuntimeEvent {
    pub id: String,
    pub ts: i64,
    pub level: String,
    pub code: Option<String>,
    pub message: String,
    pub explain: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[tauri::command]
pub fn list_ai_events(
    state: State<ApiState>,
    limit: Option<usize>,
) -> Result<Vec<AiRuntimeEvent>, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;

    let mut events = Vec::new();
    if let Some(limit) = limit {
        let mut stmt = conn
            .prepare(
                "SELECT id, ts, level, code, message, explain, data FROM event_log WHERE module = 'ai.runtime' ORDER BY ts DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], map_ai_event)
            .map_err(|e| e.to_string())?;
        for row in rows {
            events.push(row.map_err(|e| e.to_string())?);
        }
        return Ok(events);
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, ts, level, code, message, explain, data FROM event_log WHERE module = 'ai.runtime' ORDER BY ts DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], map_ai_event)
        .map_err(|e| e.to_string())?;
    for row in rows {
        events.push(row.map_err(|e| e.to_string())?);
    }
    Ok(events)
}

fn map_ai_event(row: &r2d2_sqlite::rusqlite::Row) -> r2d2_sqlite::rusqlite::Result<AiRuntimeEvent> {
    let data_str: Option<String> = row.get(6)?;
    let data = data_str.and_then(|raw| serde_json::from_str(&raw).ok());
    Ok(AiRuntimeEvent {
        id: row.get(0)?,
        ts: row.get(1)?,
        level: row.get(2)?,
        code: row.get(3)?,
        message: row.get(4)?,
        explain: row.get(5)?,
        data,
    })
}

#[tauri::command]
pub fn run_daily_digest(
    state: State<ApiState>,
    date: Option<String>,
) -> Result<JobRunResult, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    let payload = if let Some(value) = date {
        json!({ "date": value })
    } else {
        json!({})
    };
    workers::enqueue_job(&conn, "workspace.daily_digest", payload).map_err(|e| e.to_string())
}

fn ensure_today_digest(conn: &SqliteConnection) -> Result<(), String> {
    let today = OffsetDateTime::now_utc().date().to_string();
    let mut stmt = conn
        .prepare("SELECT id FROM logbook_entries WHERE entry_date = ?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    let existing: Option<String> = stmt
        .query_row([today.as_str()], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if existing.is_none() {
        let _ = workers::enqueue_job(conn, "workspace.daily_digest", json!({}))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn ai_list_providers(
    state: State<'_, ApiState>,
) -> Result<Vec<config::AiProviderInfo>, String> {
    let pool = state.db.clone();
    spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        config::list_providers(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ai_get_settings(
    state: State<'_, ApiState>,
) -> Result<config::AiSettingsSnapshot, String> {
    let pool = state.db.clone();
    spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        config::get_settings(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
pub struct AiUpdateSettingsInput {
    pub provider_id: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[tauri::command]
pub async fn ai_update_settings(
    state: State<'_, ApiState>,
    input: AiUpdateSettingsInput,
) -> Result<config::AiSettingsSnapshot, String> {
    let pool = state.db.clone();
    spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let snapshot = config::update_settings(
            &conn,
            AiSettingsUpdate {
                provider_id: input.provider_id,
                model: input.model,
                api_key: input.api_key,
                base_url: input.base_url,
            },
        )
        .map_err(|e| e.to_string())?;
        config::audit_settings_change(&conn, "AI settings updated");
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
pub struct AiChatMessageInput {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct AiChatCommandInput {
    pub messages: Vec<AiChatMessageInput>,
    pub temperature: Option<f32>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, ApiState>,
    input: AiChatCommandInput,
) -> Result<AiChatResponse, String> {
    let pool = state.db.clone();
    let provider_override = input.provider_id.clone();
    let model_override = input.model.clone();

    let selection = spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        config::resolve_runtime(&conn, provider_override, model_override).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let ai_input = AiChatInput {
        messages: input
            .messages
            .iter()
            .map(|m| AiChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect(),
        temperature: input.temperature,
    };

    let response = state
        .ai
        .chat(&selection, ai_input)
        .await
        .map_err(|e| e.to_string())?;

    let log_pool = state.db.clone();
    let provider_id = selection.provider.id.clone();
    let model_name = selection.model.clone();
    let preview = response.content.chars().take(160).collect::<String>();

    let _ = spawn_blocking(move || {
        if let Ok(conn) = log_pool.get() {
            let _ = log_event(
                &conn,
                "info",
                Some("AI-0100"),
                "ai.runtime",
                "AI chat invocation",
                Some("Request completed"),
                Some(serde_json::json!({
                    "provider": provider_id,
                    "model": model_name,
                    "preview": preview,
                })),
            );
        }
        Ok::<(), ()>(())
    })
    .await;

    Ok(response)
}
