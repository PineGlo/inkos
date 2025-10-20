use r2d2_sqlite::rusqlite::Connection;
use uuid::Uuid;
use time::OffsetDateTime;

pub fn enqueue_job(conn: &Connection, kind: &str, payload: serde_json::Value) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO jobs (id, kind, state, payload, created_at, updated_at) VALUES (?1, ?2, 'queued', ?3, ?4, ?5)",
        (id.as_str(), kind, payload.to_string(), now, now),
    )?;
    Ok(id)
}
