//! Version 1 of the Tauri IPC API.
//!
//! Commands are intentionally thin wrappers that validate input, execute work
//! on background threads where needed, and return JSON-friendly payloads to
//! the UI.

use std::sync::Arc;

use crate::agents::config::{self, AiSettingsUpdate};
use crate::agents::{AiChatInput, AiChatMessage, AiChatResponse};
use crate::db::DbPool;
use crate::logging::log_event;
use crate::model_manager::ModelManager;
use crate::summarizer::{
    AppendResult, ConversationRecord, MessageRecord, RolloverOutcome, Summarizer, SummaryRecord,
};
use crate::workers::{JobRunResult, JobScheduler};
use r2d2_sqlite::rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{async_runtime::spawn_blocking, State};
use time::macros::format_description;
use time::Date;
use time::OffsetDateTime;
use uuid::Uuid;

/// Shared state injected into each Tauri command handler.
#[derive(Clone)]
pub struct ApiState {
    pub db: DbPool,
    pub model_manager: Arc<ModelManager>,
    pub summarizer: Arc<Summarizer>,
    pub scheduler: Arc<JobScheduler>,
}

/// Simple health-check endpoint for UI components.
#[tauri::command]
pub fn ping() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "ts": OffsetDateTime::now_utc().unix_timestamp(),
    })
}

/// Inspect the SQLite catalog to confirm the database is reachable.
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

#[derive(Serialize)]
pub struct AiSettingsView {
    #[serde(flatten)]
    pub snapshot: config::AiSettingsSnapshot,
    pub warn_ratio: f32,
    pub force_ratio: f32,
    pub summarizer_model: Option<String>,
}

/// Persist a note and log the action for the activity feed.
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

/// Return notes optionally filtered by a full-text query.
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

/// Summarised view of each logbook record.
#[derive(Serialize)]
pub struct LogbookEntry {
    pub id: String,
    pub entry_date: String,
    pub summary: String,
    pub created_at: i64,
}

/// List daily logbook entries, ensuring today's digest is queued if missing.
#[tauri::command]
pub fn list_logbook_entries(
    state: State<ApiState>,
    limit: Option<usize>,
) -> Result<Vec<LogbookEntry>, String> {
    ensure_today_digest(&state)?;
    let conn = state.db.get().map_err(|e| e.to_string())?;

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

/// Timeline event DTO surfaced to the frontend.
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

/// Fetch timeline events for a specific day.
#[tauri::command]
pub fn list_timeline_events(
    state: State<ApiState>,
    date: Option<String>,
) -> Result<Vec<TimelineEvent>, String> {
    ensure_today_digest(&state)?;
    let conn = state.db.get().map_err(|e| e.to_string())?;

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

/// Structured AI runtime event surfaced in the debugger UI.
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

/// Return recent AI runtime events for diagnostics.
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

/// Trigger the daily digest worker immediately.
#[tauri::command]
pub async fn run_daily_digest(
    state: State<'_, ApiState>,
    date: Option<String>,
) -> Result<JobRunResult, String> {
    let payload = if let Some(value) = date {
        json!({ "date": value })
    } else {
        json!({})
    };
    state
        .scheduler
        .run_now("workspace.daily_digest", payload)
        .await
        .map_err(|e| e.to_string())
}

/// Ensure the daily digest job has been scheduled for the current day.
fn ensure_today_digest(state: &State<ApiState>) -> Result<(), String> {
    let today = OffsetDateTime::now_utc().date().to_string();
    let missing = {
        let conn = state.db.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id FROM logbook_entries WHERE entry_date = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let existing: Option<String> = stmt
            .query_row([today.as_str()], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        existing.is_none()
    };
    if missing {
        let _ = state
            .scheduler
            .run_now_blocking("workspace.daily_digest", json!({ "date": today }))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// List available AI providers via a blocking thread pool.
#[tauri::command]
pub async fn ai_list_providers(
    state: State<'_, ApiState>,
) -> Result<Vec<config::AiProviderInfo>, String> {
    state
        .model_manager
        .list_providers()
        .map_err(|e| e.to_string())
}

/// Fetch the current AI settings snapshot via a blocking thread pool.
#[tauri::command]
pub async fn ai_get_settings(state: State<'_, ApiState>) -> Result<AiSettingsView, String> {
    let pool = state.db.clone();
    let snapshot = spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        config::get_settings(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let summarizer_config = state.summarizer.load_config().map_err(|e| e.to_string())?;

    Ok(AiSettingsView {
        snapshot,
        warn_ratio: summarizer_config.warn_ratio,
        force_ratio: summarizer_config.force_ratio,
        summarizer_model: summarizer_config.summarizer_model,
    })
}

#[derive(Deserialize)]
pub struct AiUpdateSettingsInput {
    pub provider_id: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub warn_ratio: Option<f32>,
    pub force_ratio: Option<f32>,
    pub summarizer_model: Option<String>,
}

/// Update AI provider settings from the UI.
#[tauri::command]
pub async fn ai_update_settings(
    state: State<'_, ApiState>,
    input: AiUpdateSettingsInput,
) -> Result<AiSettingsView, String> {
    let pool = state.db.clone();
    let summarizer_config = state.summarizer.load_config().map_err(|e| e.to_string())?;
    let warn_ratio = input.warn_ratio.unwrap_or(summarizer_config.warn_ratio);
    let force_ratio = input.force_ratio.unwrap_or(summarizer_config.force_ratio);
    let summarizer_model = input
        .summarizer_model
        .clone()
        .or(summarizer_config.summarizer_model.clone());

    let provider_id = input.provider_id.clone();
    let model = input.model.clone();
    let api_key = input.api_key.clone();
    let base_url = input.base_url.clone();

    let snapshot = spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let snapshot = config::update_settings(
            &conn,
            AiSettingsUpdate {
                provider_id,
                model,
                api_key,
                base_url,
            },
        )
        .map_err(|e| e.to_string())?;
        config::audit_settings_change(&conn, "AI settings updated");
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())??;

    let summarizer_state = state
        .summarizer
        .update_config(warn_ratio, force_ratio, summarizer_model)
        .map_err(|e| e.to_string())?;

    Ok(AiSettingsView {
        snapshot,
        warn_ratio: summarizer_state.warn_ratio,
        force_ratio: summarizer_state.force_ratio,
        summarizer_model: summarizer_state.summarizer_model,
    })
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

#[derive(Deserialize)]
pub struct ChatCreateConversationInput {
    pub title: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ChatMessagesInput {
    pub conversation_id: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct ChatAppendInput {
    pub conversation_id: String,
    pub content: String,
    pub role: Option<String>,
}

#[derive(Deserialize)]
pub struct AiRolloverInput {
    pub conversation_id: String,
}

#[derive(Deserialize)]
pub struct AiSetModelInput {
    pub conversation_id: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AiSummarizeInput {
    pub target_type: String,
    pub target_id: String,
}

#[derive(Deserialize)]
pub struct AiSummaryLookupInput {
    pub summary_id: String,
}

/// Execute a chat completion via the orchestrator and record the result.
#[tauri::command]
pub async fn ai_chat(
    state: State<'_, ApiState>,
    input: AiChatCommandInput,
) -> Result<AiChatResponse, String> {
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

    state
        .model_manager
        .chat(
            ai_input,
            input.provider_id.clone(),
            input.model.clone(),
            false,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_list_models(
    state: State<'_, ApiState>,
) -> Result<Vec<config::AiProviderInfo>, String> {
    ai_list_providers(state).await
}

#[tauri::command]
pub async fn chat_create_conversation(
    state: State<'_, ApiState>,
    input: ChatCreateConversationInput,
) -> Result<ConversationRecord, String> {
    state
        .summarizer
        .create_conversation(input.title, input.provider_id, input.model_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_list_conversations(
    state: State<'_, ApiState>,
    limit: Option<usize>,
) -> Result<Vec<ConversationRecord>, String> {
    state
        .summarizer
        .list_conversations(limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_get_messages(
    state: State<'_, ApiState>,
    input: ChatMessagesInput,
) -> Result<Vec<MessageRecord>, String> {
    state
        .summarizer
        .list_messages(&input.conversation_id, input.limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_append_and_maybe_rollover(
    state: State<'_, ApiState>,
    input: ChatAppendInput,
) -> Result<AppendResult, String> {
    let role = input.role.unwrap_or_else(|| "user".to_string());
    state
        .summarizer
        .append_and_maybe_rollover(&input.conversation_id, &role, &input.content)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_rollover_chat(
    state: State<'_, ApiState>,
    input: AiRolloverInput,
) -> Result<RolloverOutcome, String> {
    state
        .summarizer
        .rollover(&input.conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_set_model(
    state: State<'_, ApiState>,
    input: AiSetModelInput,
) -> Result<ConversationRecord, String> {
    state
        .summarizer
        .set_conversation_model(&input.conversation_id, input.provider_id, input.model_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_summarize(
    state: State<'_, ApiState>,
    input: AiSummarizeInput,
) -> Result<SummaryRecord, String> {
    match input.target_type.as_str() {
        "note" => {
            let pool = state.db.clone();
            let target_id = input.target_id.clone();
            let (title, body): (String, String) = spawn_blocking(move || {
                let conn = pool.get().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare("SELECT title, body FROM notes WHERE id = ?1")
                    .map_err(|e| e.to_string())?;
                let result = stmt
                    .query_row([target_id.as_str()], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(|e| e.to_string())?;
                Ok(result)
            })
            .await
            .map_err(|e| e.to_string())??;
            let content = format!("# {title}\n\n{body}");
            state
                .summarizer
                .summarise("note", &input.target_id, &content)
                .map_err(|e| e.to_string())
        }
        "conversation" => state
            .summarizer
            .summarise_conversation(&input.target_id)
            .map_err(|e| e.to_string()),
        "day" => {
            let pool = state.db.clone();
            let target_id = input.target_id.clone();
            let summary_text: String = spawn_blocking(move || {
                let conn = pool.get().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare("SELECT summary FROM logbook_entries WHERE entry_date = ?1")
                    .map_err(|e| e.to_string())?;
                let result = stmt
                    .query_row([target_id.as_str()], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                Ok(result)
            })
            .await
            .map_err(|e| e.to_string())??;
            state
                .summarizer
                .summarise("day", &input.target_id, &summary_text)
                .map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported summary target: {other}")),
    }
}

#[tauri::command]
pub async fn ai_get_summary(
    state: State<'_, ApiState>,
    input: AiSummaryLookupInput,
) -> Result<Option<SummaryRecord>, String> {
    state
        .summarizer
        .fetch_summary(&input.summary_id)
        .map_err(|e| e.to_string())
}
