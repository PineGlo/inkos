use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD as B64_ENGINE;
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;

use super::providers::PROVIDER_SEEDS;
use crate::logging::log_event;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderInfo {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub description: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    #[serde(default)]
    pub capability_tags: Vec<String>,
    pub requires_api_key: bool,
    pub has_credentials: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiSettingsSnapshot {
    pub active_provider_id: Option<String>,
    pub active_model: Option<String>,
    pub provider: Option<AiProviderInfo>,
}

#[derive(Debug, Clone)]
pub struct AiRuntimeSelection {
    pub provider: AiProviderInfo,
    pub model: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AiSettingsUpdate {
    pub provider_id: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

pub fn seed_defaults(conn: &rusqlite::Connection) -> Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    for seed in PROVIDER_SEEDS {
        let models_json = serde_json::to_string(seed.models)?;
        let caps_json = serde_json::to_string(seed.tags)?;
        conn.execute(
            "INSERT INTO ai_providers (id, kind, display_name, description, base_url, default_model, models_json, capabilities_json, requires_api_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 kind = excluded.kind,
                 display_name = excluded.display_name,
                 description = excluded.description,
                 base_url = excluded.base_url,
                 default_model = excluded.default_model,
                 models_json = excluded.models_json,
                 capabilities_json = excluded.capabilities_json,
                 requires_api_key = excluded.requires_api_key,
                 updated_at = excluded.updated_at",
            params![
                seed.id,
                seed.kind,
                seed.display,
                seed.description,
                seed.base_url,
                seed.default_model,
                models_json,
                caps_json,
                seed.requires_api_key as i32,
                now,
            ],
        )?;
    }

    let has_active: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'ai.active'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if has_active.is_none() {
        let default_provider = PROVIDER_SEEDS
            .first()
            .ok_or_else(|| anyhow!("no providers seeded"))?;
        set_active_setting(
            conn,
            default_provider.id,
            Some(default_provider.default_model),
        )?;
    }

    Ok(())
}

pub fn list_providers(conn: &rusqlite::Connection) -> Result<Vec<AiProviderInfo>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.kind, p.display_name, p.description, p.base_url, p.default_model, p.models_json, p.capabilities_json, p.requires_api_key, \
                (SELECT COUNT(1) FROM ai_credentials c WHERE c.provider_id = p.id) as has_secret
         FROM ai_providers p
         ORDER BY p.display_name",
    )?;

    let rows = stmt.query_map([], |row| {
        let models_json: String = row.get(6)?;
        let caps_json: String = row.get(7)?;
        let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
        let caps: Vec<String> = serde_json::from_str(&caps_json).unwrap_or_default();
        Ok(AiProviderInfo {
            id: row.get(0)?,
            kind: row.get(1)?,
            display_name: row.get(2)?,
            description: row.get(3)?,
            base_url: row.get(4)?,
            default_model: row.get(5)?,
            models,
            capability_tags: caps,
            requires_api_key: row.get::<_, i64>(8)? != 0,
            has_credentials: row.get::<_, i64>(9)? > 0,
        })
    })?;

    let mut providers = Vec::new();
    for row in rows {
        providers.push(row?);
    }
    Ok(providers)
}

pub fn get_settings(conn: &rusqlite::Connection) -> Result<AiSettingsSnapshot> {
    let (provider_id, model) = read_active_setting(conn)?;
    let provider = if let Some(ref pid) = provider_id {
        Some(get_provider(conn, pid)?)
    } else {
        None
    };
    Ok(AiSettingsSnapshot {
        active_provider_id: provider_id,
        active_model: model,
        provider,
    })
}

pub fn update_settings(
    conn: &rusqlite::Connection,
    update: AiSettingsUpdate,
) -> Result<AiSettingsSnapshot> {
    let provider = get_provider(conn, &update.provider_id)?;

    if let Some(base_url) = update.base_url {
        conn.execute(
            "UPDATE ai_providers SET base_url = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                base_url,
                OffsetDateTime::now_utc().unix_timestamp(),
                update.provider_id
            ],
        )?;
    }

    if let Some(api_key) = update.api_key {
        let trimmed = api_key.trim().to_string();
        if trimmed.is_empty() {
            conn.execute(
                "DELETE FROM ai_credentials WHERE provider_id = ?1",
                params![update.provider_id],
            )?;
        } else {
            let encoded = B64_ENGINE.encode(trimmed.as_bytes());
            let now = OffsetDateTime::now_utc().unix_timestamp();
            conn.execute(
                "INSERT INTO ai_credentials (provider_id, secret, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(provider_id) DO UPDATE SET secret = excluded.secret, updated_at = excluded.updated_at",
                params![update.provider_id, encoded, now],
            )?;
        }
    }

    let model = update.model.or_else(|| provider.default_model.clone());
    set_active_setting(conn, &update.provider_id, model.as_deref())?;
    get_settings(conn)
}

pub fn resolve_runtime(
    conn: &rusqlite::Connection,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<AiRuntimeSelection> {
    let (active_provider_id, active_model) = read_active_setting(conn)?;

    let provider_id = provider_override
        .or(active_provider_id)
        .ok_or_else(|| anyhow!("No AI provider configured"))?;

    let provider = get_provider(conn, &provider_id)?;
    let mut model = model_override
        .or_else(|| {
            if provider.id == provider_id {
                active_model.clone()
            } else {
                None
            }
        })
        .or_else(|| provider.default_model.clone())
        .or_else(|| provider.models.first().cloned())
        .ok_or_else(|| anyhow!("No model configured for provider"))?;

    // allow override to reference canonical names stored in models list
    if !provider.models.is_empty() && !provider.models.contains(&model) {
        model = provider.models.first().cloned().unwrap_or(model);
    }

    let secret = load_secret(conn, &provider.id)?;

    Ok(AiRuntimeSelection {
        provider,
        model,
        secret,
    })
}

fn read_active_setting(conn: &rusqlite::Connection) -> Result<(Option<String>, Option<String>)> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'ai.active'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(v) = value {
        let data: serde_json::Value = serde_json::from_str(&v)?;
        let provider_id = data
            .get("provider_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let model = data
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok((provider_id, model))
    } else {
        Ok((None, None))
    }
}

fn set_active_setting(
    conn: &rusqlite::Connection,
    provider_id: &str,
    model: Option<&str>,
) -> Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let payload = json!({
        "provider_id": provider_id,
        "model": model,
    })
    .to_string();
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ('ai.active', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![payload, now],
    )?;
    Ok(())
}

fn get_provider(conn: &rusqlite::Connection, provider_id: &str) -> Result<AiProviderInfo> {
    conn.query_row(
        "SELECT p.id, p.kind, p.display_name, p.description, p.base_url, p.default_model, p.models_json, p.capabilities_json, p.requires_api_key,
                (SELECT COUNT(1) FROM ai_credentials c WHERE c.provider_id = p.id) as has_secret
         FROM ai_providers p WHERE p.id = ?1",
        params![provider_id],
        |row| {
            let models_json: String = row.get(6)?;
            let caps_json: String = row.get(7)?;
            let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
            let caps: Vec<String> = serde_json::from_str(&caps_json).unwrap_or_default();
            Ok(AiProviderInfo {
                id: row.get(0)?,
                kind: row.get(1)?,
                display_name: row.get(2)?,
                description: row.get(3)?,
                base_url: row.get(4)?,
                default_model: row.get(5)?,
                models,
                capability_tags: caps,
                requires_api_key: row.get::<_, i64>(8)? != 0,
                has_credentials: row.get::<_, i64>(9)? > 0,
            })
        },
    )
    .map_err(|_| anyhow!("Unknown AI provider: {provider_id}"))
}

fn load_secret(conn: &rusqlite::Connection, provider_id: &str) -> Result<Option<String>> {
    let secret: Option<String> = conn
        .query_row(
            "SELECT secret FROM ai_credentials WHERE provider_id = ?1",
            params![provider_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(s) = secret {
        let decoded = B64_ENGINE
            .decode(s.as_bytes())
            .map_err(|_| anyhow!("Failed to decode stored credential"))?;
        let value = String::from_utf8(decoded)
            .map_err(|_| anyhow!("Stored credential was not valid UTF-8"))?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

pub fn audit_settings_change(conn: &rusqlite::Connection, message: &str) {
    let _ = log_event(
        conn,
        "info",
        Some("AI-0001"),
        "ai.settings",
        message,
        Some("AI configuration updated"),
        None,
    );
}
