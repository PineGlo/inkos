use tauri::State;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use time::OffsetDateTime;
use crate::db::DbPool;
use crate::logging::log_event;

#[derive(Clone)]
pub struct ApiState { pub db: DbPool }

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
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(row.get::<_, String>(0)?)).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for r in rows { names.push(r.map_err(|e| e.to_string())?); }
    Ok(serde_json::json!({ "ok": true, "tables": names }))
}

#[derive(Deserialize)] pub struct CreateNoteInput { pub title: String, pub body: Option<String> }
#[derive(Serialize)] pub struct CreateNoteOutput { pub id: String }

#[tauri::command]
pub fn create_note(state: State<ApiState>, input: CreateNoteInput) -> Result<CreateNoteOutput, String> {
    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let body = input.body.unwrap_or_default();
    let conn = state.db.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO notes (id, title, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        (id.as_str(), input.title.as_str(), body.as_str(), now, now),
    ).map_err(|e| e.to_string())?;
    log_event(&conn, "info", Some("NTE-0000"), "notes", "note created", Some("created via IPC"), Some(serde_json::json!({ "id": id }))).map_err(|e| e.to_string())?;
    Ok(CreateNoteOutput { id })
}

#[derive(Deserialize)] pub struct ListNotesInput { pub q: Option<String> }

#[tauri::command]
pub fn list_notes(state: State<ApiState>, input: Option<ListNotesInput>) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.get().map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    if let Some(i) = input {
        if let Some(q) = i.q {
            let mut stmt = conn.prepare("SELECT id, title, created_at FROM notes WHERE rowid IN (SELECT rowid FROM fts_notes WHERE fts_notes MATCH ?1) ORDER BY created_at DESC").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([q], |row| Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "created_at": row.get::<_, i64>(2)?
            }))).map_err(|e| e.to_string())?;
            for r in rows { results.push(r.map_err(|e| e.to_string())?); }
            return Ok(results);
        }
    }
    let mut stmt = conn.prepare("SELECT id, title, created_at FROM notes ORDER BY created_at DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(serde_json::json!({
        "id": row.get::<_, String>(0)?,
        "title": row.get::<_, String>(1)?,
        "created_at": row.get::<_, i64>(2)?
    }))).map_err(|e| e.to_string())?;
    for r in rows { results.push(r.map_err(|e| e.to_string())?); }
    Ok(results)
}
