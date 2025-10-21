//! Summarisation and conversation rollover engine.
//!
//! This module centralises all logic related to condensing content (notes,
//! conversations, daily logs) and managing context limits. It persists
//! summaries for reuse, records provenance in the event log, and coordinates
//! conversation rollover when token thresholds are exceeded.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use r2d2_sqlite::rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::agents::{AiChatInput, AiChatMessage};
use crate::db::DbPool;
use crate::logging::log_event;
use crate::model_manager::ModelManager;

const SUMMARISER_PROMPT: &str = "You are InkOS' summariser. Craft a concise, factual markdown summary highlighting key actions, decisions, and next steps. Keep the tone warm yet professional. Where appropriate, group related points together and avoid redundant phrasing.";

/// Cached configuration for the summariser thresholds and model selection.
#[derive(Clone, Debug, Serialize)]
pub struct SummarizerConfig {
    pub warn_ratio: f32,
    pub force_ratio: f32,
    pub summarizer_model: Option<String>,
}

/// Persisted summary metadata returned to callers.
#[derive(Clone, Debug, Serialize)]
pub struct SummaryRecord {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub version: i64,
    pub body: String,
    pub token_est: Option<i64>,
    pub model_id: Option<String>,
    pub created_at: i64,
    pub reused: bool,
}

/// Representation of a conversation row returned through the API.
#[derive(Clone, Debug, Serialize)]
pub struct ConversationRecord {
    pub id: String,
    pub title: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub ctx_warn: bool,
    pub ctx_force: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub quality_flags: Option<String>,
    pub total_tokens: i64,
}

/// Normalised chat message returned to the UI.
#[derive(Clone, Debug, Serialize)]
pub struct MessageRecord {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub body: String,
    pub token_est: Option<i64>,
    pub created_at: i64,
    pub quality_flags: Option<String>,
}

/// Outcome returned after appending a message and checking rollover.
#[derive(Clone, Debug, Serialize)]
pub struct AppendResult {
    pub message: MessageRecord,
    pub warn: bool,
    pub rolled: bool,
    pub new_conversation: Option<ConversationRecord>,
    pub summary: Option<SummaryRecord>,
    pub total_tokens: i64,
}

/// Details describing an explicit rollover request.
#[derive(Clone, Debug, Serialize)]
pub struct RolloverOutcome {
    pub rolled: bool,
    pub new_conversation: Option<ConversationRecord>,
    pub summary: Option<SummaryRecord>,
}

/// Primary entry point for the summarisation and rollover subsystem.
#[derive(Clone)]
pub struct Summarizer {
    pool: DbPool,
    models: Arc<ModelManager>,
}

impl Summarizer {
    /// Construct a new summariser bound to the SQLite pool and model manager.
    pub fn new(pool: DbPool, models: Arc<ModelManager>) -> Arc<Self> {
        Arc::new(Self { pool, models })
    }

    /// Provide synchronous access to the underlying connection pool.
    pub fn pool(&self) -> DbPool {
        self.pool.clone()
    }

    /// Read the persisted configuration from `app_settings`.
    pub fn load_config(&self) -> Result<SummarizerConfig> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        read_config(&conn)
    }

    /// Persist updated thresholds and optional summariser model override.
    pub fn update_config(
        &self,
        warn_ratio: f32,
        force_ratio: f32,
        summarizer_model: Option<String>,
    ) -> Result<SummarizerConfig> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        write_config(&conn, warn_ratio, force_ratio, summarizer_model)?;
        read_config(&conn)
    }

    /// Create a conversation seeded with the current active provider/model.
    pub fn create_conversation(
        &self,
        title: Option<String>,
        provider_override: Option<String>,
        model_override: Option<String>,
    ) -> Result<ConversationRecord> {
        let selection = self
            .models
            .resolve_runtime(provider_override, model_override, true)?;
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO conversations (id, title, provider_id, model_id, ctx_warn, ctx_force, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?5)",
            params![
                id,
                title,
                selection.provider.id,
                selection.model,
                now,
            ],
        )?;
        fetch_conversation(&conn, &id)?
            .ok_or_else(|| anyhow!("conversation missing after creation"))
    }

    /// Return conversations ordered by most recent activity.
    pub fn list_conversations(&self, limit: Option<usize>) -> Result<Vec<ConversationRecord>> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        list_conversations(&conn, limit)
    }

    /// Fetch messages for a conversation.
    pub fn list_messages(
        &self,
        conversation_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessageRecord>> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        list_messages(&conn, conversation_id, limit)
    }

    /// Fetch a single conversation by id.
    pub fn get_conversation(&self, conversation_id: &str) -> Result<Option<ConversationRecord>> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        fetch_conversation(&conn, conversation_id)
    }

    /// Override the provider/model used for a conversation.
    pub fn set_conversation_model(
        &self,
        conversation_id: &str,
        provider_override: Option<String>,
        model_override: Option<String>,
    ) -> Result<ConversationRecord> {
        let selection = self
            .models
            .resolve_runtime(provider_override, model_override, true)?;
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        conn.execute(
            "UPDATE conversations SET provider_id = ?2, model_id = ?3, updated_at = ?4 WHERE id = ?1",
            params![conversation_id, selection.provider.id, selection.model, now],
        )?;
        log_event(
            &conn,
            "info",
            Some("AI-SET-MODEL"),
            "ai.context",
            "Conversation model updated",
            Some("Provider/model override applied"),
            Some(json!({
                "conversation_id": conversation_id,
                "provider": selection.provider.id,
                "model": selection.model,
            })),
        )
        .ok();
        fetch_conversation(&conn, conversation_id)?.ok_or_else(|| anyhow!("conversation not found"))
    }

    /// Retrieve a previously cached summary by id.
    pub fn fetch_summary(&self, summary_id: &str) -> Result<Option<SummaryRecord>> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        load_summary(&conn, summary_id)
    }

    /// Generate or return a cached conversation summary without rolling over.
    pub fn summarise_conversation(&self, conversation_id: &str) -> Result<SummaryRecord> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        let conversation = fetch_conversation(&conn, conversation_id)?
            .ok_or_else(|| anyhow!("conversation not found"))?;
        let messages = list_messages(&conn, conversation_id, None)?;
        let mut excerpts = select_conversation_excerpts(&messages, None);
        let config = read_config(&conn)?;
        store_or_create_summary(
            &conn,
            self.models.as_ref(),
            "conversation",
            conversation_id,
            &mut excerpts,
            &config,
        )
    }

    /// Append a new message and evaluate rollover thresholds.
    pub fn append_and_maybe_rollover(
        &self,
        conversation_id: &str,
        role: &str,
        body: &str,
    ) -> Result<AppendResult> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        let config = read_config(&conn)?;
        let mut tx = conn.transaction()?;
        let conversation = fetch_conversation(&tx, conversation_id)?
            .ok_or_else(|| anyhow!("conversation not found"))?;
        if conversation.ctx_force {
            return Err(anyhow!("conversation already rolled"));
        }
        let message = insert_message(&tx, conversation_id, role, body)?;
        let total_tokens = sum_tokens(&tx, conversation_id)?;
        let context_limit =
            context_limit_from_tags(&tx, &conversation.provider_id, &conversation.model_id)?;
        let warn_threshold = (context_limit as f32 * config.warn_ratio) as i64;
        let force_threshold = (context_limit as f32 * config.force_ratio) as i64;
        let mut warn = conversation.ctx_warn;
        if total_tokens >= warn_threshold && !conversation.ctx_warn {
            mark_ctx_warn(&tx, conversation_id)?;
            warn = true;
            log_event(
                &tx,
                "warn",
                Some("AI-CTX-WARN"),
                "ai.context",
                "Conversation approaching context limit",
                Some("A warning banner should be shown in the UI."),
                Some(json!({
                    "conversation_id": conversation_id,
                    "total_tokens": total_tokens,
                    "threshold": warn_threshold,
                })),
            )
            .ok();
        }
        if total_tokens >= force_threshold {
            let outcome = perform_rollover(
                &mut tx,
                &conversation,
                self.models.as_ref(),
                &config,
                Some((role, body)),
            )?;
            tx.commit()?;
            return Ok(AppendResult {
                message,
                warn: true,
                rolled: true,
                new_conversation: outcome.new_conversation,
                summary: outcome.summary,
                total_tokens,
            });
        }
        tx.commit()?;
        Ok(AppendResult {
            message,
            warn,
            rolled: false,
            new_conversation: None,
            summary: None,
            total_tokens,
        })
    }

    /// Force a rollover for the supplied conversation.
    pub fn rollover(&self, conversation_id: &str) -> Result<RolloverOutcome> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        let config = read_config(&conn)?;
        let mut tx = conn.transaction()?;
        let conversation = fetch_conversation(&tx, conversation_id)?
            .ok_or_else(|| anyhow!("conversation not found"))?;
        let outcome =
            perform_rollover(&mut tx, &conversation, self.models.as_ref(), &config, None)?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Summarise arbitrary source material.
    pub fn summarise(
        &self,
        target_type: &str,
        target_id: &str,
        content: &str,
    ) -> Result<SummaryRecord> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        summarise_text(
            &conn,
            self.models.as_ref(),
            target_type,
            target_id,
            content,
            None,
        )
    }

    /// Summarise a daily digest payload with caching.
    pub fn summarise_daily_digest(
        &self,
        date_key: &str,
        facts: serde_json::Value,
        fallback: &str,
    ) -> Result<SummaryRecord> {
        let conn = self.pool.get().map_err(|err| anyhow!(err.to_string()))?;
        summarise_text(
            &conn,
            self.models.as_ref(),
            "day",
            date_key,
            fallback,
            Some(facts),
        )
    }
}

/// Estimate tokens using a simple character based heuristic.
pub fn approx_tokens(text: &str) -> usize {
    let chars = text.chars().count() as f32;
    let words = text.split_whitespace().count() as f32;
    let char_est = (chars / 4.0).ceil();
    let word_est = (words * 1.1).ceil();
    char_est.max(word_est).max(1.0) as usize
}

fn read_config(conn: &rusqlite::Connection) -> Result<SummarizerConfig> {
    let warn_ratio = read_setting(conn, "ai.rollover.warn_ratio")?.unwrap_or(0.75);
    let force_ratio = read_setting(conn, "ai.rollover.force_ratio")?.unwrap_or(0.9);
    let summarizer_model = read_string_setting(conn, "ai.summarizer_model")?;
    Ok(SummarizerConfig {
        warn_ratio,
        force_ratio,
        summarizer_model,
    })
}

fn write_config(
    conn: &rusqlite::Connection,
    warn_ratio: f32,
    force_ratio: f32,
    summarizer_model: Option<String>,
) -> Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    upsert_setting(conn, "ai.rollover.warn_ratio", warn_ratio.to_string(), now)?;
    upsert_setting(
        conn,
        "ai.rollover.force_ratio",
        force_ratio.to_string(),
        now,
    )?;
    let summarizer_value = serde_json::to_string(&summarizer_model)?;
    upsert_setting(conn, "ai.summarizer_model", summarizer_value, now)?;
    Ok(())
}

fn read_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<f32>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(value) = value {
        Ok(value.parse::<f32>().ok())
    } else {
        Ok(None)
    }
}

fn read_string_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
}

fn upsert_setting(conn: &rusqlite::Connection, key: &str, value: String, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now],
    )?;
    Ok(())
}

fn list_conversations(
    conn: &rusqlite::Connection,
    limit: Option<usize>,
) -> Result<Vec<ConversationRecord>> {
    let mut sql = "SELECT id, title, provider_id, model_id, ctx_warn, ctx_force, created_at, updated_at, closed_at, quality_flags FROM conversations ORDER BY updated_at DESC".to_string();
    if limit.is_some() {
        sql.push_str(" LIMIT ?1");
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(limit) = limit {
        stmt.query_map(params![limit as i64], |row| row_to_conversation(conn, row))?
    } else {
        stmt.query_map([], |row| row_to_conversation(conn, row))?
    };
    let mut conversations = Vec::new();
    for row in rows {
        conversations.push(row?);
    }
    Ok(conversations)
}

fn row_to_conversation(
    conn: &rusqlite::Connection,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConversationRecord> {
    let id: String = row.get(0)?;
    let total_tokens = sum_tokens(conn, &id).unwrap_or(0);
    Ok(ConversationRecord {
        id,
        title: row.get(1)?,
        provider_id: row.get(2)?,
        model_id: row.get(3)?,
        ctx_warn: row.get::<_, i64>(4)? != 0,
        ctx_force: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        closed_at: row.get(8)?,
        quality_flags: row.get(9)?,
        total_tokens,
    })
}

fn fetch_conversation(
    conn: &rusqlite::Connection,
    conversation_id: &str,
) -> Result<Option<ConversationRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, provider_id, model_id, ctx_warn, ctx_force, created_at, updated_at, closed_at, quality_flags FROM conversations WHERE id = ?1",
    )?;
    let row = stmt
        .query_row([conversation_id], |row| row_to_conversation(conn, row))
        .optional()?;
    Ok(row)
}

fn list_messages(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    limit: Option<usize>,
) -> Result<Vec<MessageRecord>> {
    let mut sql = "SELECT id, conversation_id, role, body, token_est, quality_flags, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC".to_string();
    if let Some(limit) = limit {
        sql.push_str(" LIMIT ");
        sql.push_str(&limit.to_string());
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([conversation_id], |row| {
        Ok(MessageRecord {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: row.get(2)?,
            body: row.get(3)?,
            token_est: row.get(4)?,
            quality_flags: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

fn insert_message(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    role: &str,
    body: &str,
) -> Result<MessageRecord> {
    let id = Uuid::new_v4().to_string();
    let tokens = approx_tokens(body) as i64;
    let created_at = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, body, token_est, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, conversation_id, role, body, tokens, created_at],
    )?;
    conn.execute(
        "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
        params![conversation_id, created_at],
    )?;
    Ok(MessageRecord {
        id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        body: body.to_string(),
        token_est: Some(tokens),
        created_at,
        quality_flags: None,
    })
}

fn sum_tokens(conn: &rusqlite::Connection, conversation_id: &str) -> Result<i64> {
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(token_est), 0) FROM messages WHERE conversation_id = ?1",
        params![conversation_id],
        |row| row.get(0),
    )?;
    Ok(total)
}

fn mark_ctx_warn(conn: &rusqlite::Connection, conversation_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE conversations SET ctx_warn = 1 WHERE id = ?1",
        params![conversation_id],
    )?;
    Ok(())
}

fn mark_ctx_force(conn: &rusqlite::Connection, conversation_id: &str) -> Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "UPDATE conversations SET ctx_force = 1, closed_at = ?2 WHERE id = ?1",
        params![conversation_id, now],
    )?;
    Ok(())
}

fn context_limit_from_tags(
    conn: &rusqlite::Connection,
    provider_id: &str,
    model_id: &str,
) -> Result<usize> {
    let providers = crate::agents::config::list_providers(conn)?;
    if let Some(provider) = providers.into_iter().find(|p| p.id == provider_id) {
        for tag in provider.capability_tags {
            if let Some(limit) = parse_context_tag(&tag) {
                return Ok(limit);
            }
        }
    }
    if model_id.to_lowercase().contains("32k") {
        return Ok(32_000);
    }
    Ok(4096)
}

fn parse_context_tag(tag: &str) -> Option<usize> {
    if let Some(rest) = tag.strip_prefix("ctx-") {
        if rest.ends_with('k') {
            let digits: String = rest[..rest.len() - 1]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if let Ok(value) = digits.parse::<usize>() {
                return Some(value * 1000);
            }
        } else if let Ok(value) = rest.parse::<usize>() {
            return Some(value);
        }
    }
    None
}

fn perform_rollover(
    conn: &mut rusqlite::Transaction<'_>,
    conversation: &ConversationRecord,
    models: &ModelManager,
    config: &SummarizerConfig,
    pending_message: Option<(&str, &str)>,
) -> Result<RolloverOutcome> {
    mark_ctx_force(conn, &conversation.id)?;
    let messages = list_messages(conn, &conversation.id, None)?;
    let mut excerpts = select_conversation_excerpts(&messages, pending_message);
    let summary = store_or_create_summary(
        conn,
        models,
        "conversation",
        &conversation.id,
        &mut excerpts,
        config,
    )?;

    let selection = models.resolve_runtime(
        Some(conversation.provider_id.clone()),
        Some(conversation.model_id.clone()),
        true,
    )?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let new_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO conversations (id, title, provider_id, model_id, ctx_warn, ctx_force, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?5)",
        params![
            new_id,
            conversation.title.clone(),
            selection.provider.id,
            selection.model,
            now,
        ],
    )?;

    let summary_body = summary.body.clone();
    insert_message(
        conn,
        &new_id,
        "system",
        &format!("Summary of previous thread:\n{}", summary_body),
    )?;

    insert_link(
        conn,
        &conversation.id,
        "conversation",
        &summary.id,
        "summary",
        "summarised_as",
    )?;
    insert_link(
        conn,
        &summary.id,
        "summary",
        &new_id,
        "conversation",
        "rollover_to",
    )?;

    log_event(
        conn,
        "info",
        Some("AI-CTX-ROLLOVER"),
        "ai.context",
        "Conversation rolled over",
        Some("A new thread was created to keep context within limits."),
        Some(json!({
            "previous_conversation": conversation.id,
            "new_conversation": new_id,
            "summary_id": summary.id,
        })),
    )
    .ok();

    let new_conversation = fetch_conversation(conn, &new_id)?;

    Ok(RolloverOutcome {
        rolled: true,
        new_conversation,
        summary: Some(summary),
    })
}

fn select_conversation_excerpts(
    messages: &[MessageRecord],
    pending_message: Option<(&str, &str)>,
) -> Vec<String> {
    let mut excerpts = Vec::new();
    let mut keywords: HashSet<String> = HashSet::new();
    if let Some((role, body)) = pending_message {
        excerpts.push(format!("{}: {}", role, body));
        keywords.extend(extract_keywords(body));
    }
    let total = messages.len();
    let tail_start = total.saturating_sub(12);
    for msg in messages.iter().skip(tail_start) {
        excerpts.push(format!("{}: {}", msg.role, msg.body));
    }
    if keywords.is_empty() {
        return excerpts;
    }
    for msg in messages.iter().take(tail_start) {
        if keywords.iter().any(|k| msg.body.to_lowercase().contains(k)) {
            excerpts.insert(0, format!("{}: {}", msg.role, msg.body));
        }
    }
    excerpts
}

fn extract_keywords(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .filter(|word| word.len() > 4)
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn summarise_text(
    conn: &rusqlite::Connection,
    models: &ModelManager,
    target_type: &str,
    target_id: &str,
    content: &str,
    context: Option<serde_json::Value>,
) -> Result<SummaryRecord> {
    let config = read_config(conn)?;
    let mut excerpts = vec![content.to_string()];
    if let Some(ctx) = context {
        excerpts.push(ctx.to_string());
    }
    store_or_create_summary(conn, models, target_type, target_id, &mut excerpts, &config)
}

fn store_or_create_summary(
    conn: &rusqlite::Connection,
    models: &ModelManager,
    target_type: &str,
    target_id: &str,
    excerpts: &mut Vec<String>,
    config: &SummarizerConfig,
) -> Result<SummaryRecord> {
    let hash = hash_strings(excerpts);
    if let Some(summary) = find_cached_summary(conn, target_type, target_id, &hash)? {
        return Ok(summary);
    }

    let prompt = excerpts.join("\n\n");
    let messages = vec![
        AiChatMessage {
            role: "system".into(),
            content: SUMMARISER_PROMPT.into(),
        },
        AiChatMessage {
            role: "user".into(),
            content: prompt.clone(),
        },
    ];
    let input = AiChatInput {
        messages,
        temperature: Some(0.2),
    };

    let response = models.chat_blocking(input, None, config.summarizer_model.clone(), true);

    let (body, model_id, explain) = match response {
        Ok(resp) => {
            let body = resp.content.trim().to_string();
            if body.is_empty() {
                (
                    prompt.clone(),
                    Some(resp.model),
                    "AI returned empty output".to_string(),
                )
            } else {
                (body, Some(resp.model), String::new())
            }
        }
        Err(err) => {
            let message = err.to_string();
            log_event(
                conn,
                "warn",
                Some("AI-SUMMARY-ERR"),
                "ai.summary",
                "AI summarisation failed",
                Some("Falling back to deterministic text"),
                Some(json!({
                    "target_type": target_type,
                    "target_id": target_id,
                    "error": message,
                })),
            )
            .ok();
            (prompt.clone(), None, message)
        }
    };

    let created = insert_summary(conn, target_type, target_id, &body, &hash, model_id.clone())?;
    if explain.is_empty() {
        log_event(
            conn,
            "info",
            Some("AI-SUMMARY"),
            "ai.summary",
            "Summary generated",
            Some("Cached for future reuse"),
            Some(json!({
                "target_type": target_type,
                "target_id": target_id,
                "model": model_id,
            })),
        )
        .ok();
    }
    Ok(created)
}

fn insert_summary(
    conn: &rusqlite::Connection,
    target_type: &str,
    target_id: &str,
    body: &str,
    source_hash: &str,
    model_id: Option<String>,
) -> Result<SummaryRecord> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM summaries WHERE target_type = ?1 AND target_id = ?2",
            params![target_type, target_id],
            |row| row.get(0),
        )?;
    let id = Uuid::new_v4().to_string();
    let token_est = approx_tokens(body) as i64;
    conn.execute(
        "INSERT INTO summaries (id, target_type, target_id, version, body, token_est, source_hash, model_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            target_type,
            target_id,
            version,
            body,
            token_est,
            source_hash,
            model_id,
            now,
        ],
    )?;
    Ok(SummaryRecord {
        id,
        target_type: target_type.into(),
        target_id: target_id.into(),
        version,
        body: body.into(),
        token_est: Some(token_est),
        model_id,
        created_at: now,
        reused: false,
    })
}

fn find_cached_summary(
    conn: &rusqlite::Connection,
    target_type: &str,
    target_id: &str,
    hash: &str,
) -> Result<Option<SummaryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, version, body, token_est, model_id, created_at FROM summaries WHERE target_type = ?1 AND target_id = ?2 AND source_hash = ?3 ORDER BY version DESC LIMIT 1",
    )?;
    let summary = stmt
        .query_row(params![target_type, target_id, hash], |row| {
            Ok(SummaryRecord {
                id: row.get(0)?,
                target_type: target_type.into(),
                target_id: target_id.into(),
                version: row.get(1)?,
                body: row.get(2)?,
                token_est: row.get(3)?,
                model_id: row.get(4)?,
                created_at: row.get(5)?,
                reused: true,
            })
        })
        .optional()?;
    Ok(summary)
}

fn load_summary(conn: &rusqlite::Connection, summary_id: &str) -> Result<Option<SummaryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT target_type, target_id, version, body, token_est, model_id, created_at FROM summaries WHERE id = ?1",
    )?;
    let summary = stmt
        .query_row([summary_id], |row| {
            Ok(SummaryRecord {
                id: summary_id.into(),
                target_type: row.get(0)?,
                target_id: row.get(1)?,
                version: row.get(2)?,
                body: row.get(3)?,
                token_est: row.get(4)?,
                model_id: row.get(5)?,
                created_at: row.get(6)?,
                reused: true,
            })
        })
        .optional()?;
    Ok(summary)
}

fn hash_strings(values: &[String]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn insert_link(
    conn: &rusqlite::Connection,
    src_id: &str,
    src_type: &str,
    dst_id: &str,
    dst_type: &str,
    rel: &str,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let created_at = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO links (id, src_id, src_type, dst_id, dst_type, rel, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, src_id, src_type, dst_id, dst_type, rel, created_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2_sqlite::rusqlite::Connection as SqliteConnection;

    #[test]
    fn approx_tokens_scales_with_length() {
        assert!(approx_tokens("short") > 0);
        assert!(approx_tokens(&"word".repeat(40)) > approx_tokens("hello"));
    }

    #[test]
    fn parse_context_tag_handles_suffixes() {
        assert_eq!(parse_context_tag("ctx-4096"), Some(4096));
        assert_eq!(parse_context_tag("ctx-8k"), Some(8000));
        assert_eq!(parse_context_tag("other"), None);
    }

    #[test]
    fn insert_summary_assigns_incrementing_versions() {
        let conn = SqliteConnection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE summaries (id TEXT PRIMARY KEY, target_type TEXT, target_id TEXT, version INTEGER, body TEXT, token_est INTEGER, source_hash TEXT, model_id TEXT, created_at INTEGER);",
        )
        .unwrap();
        let summary1 = insert_summary(
            &conn,
            "conversation",
            "a",
            "Body",
            "hash",
            Some("model".into()),
        )
        .unwrap();
        let summary2 = insert_summary(
            &conn,
            "conversation",
            "a",
            "Body",
            "hash",
            Some("model".into()),
        )
        .unwrap();
        assert_eq!(summary1.version + 1, summary2.version);
    }
}
