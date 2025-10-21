//! High level AI model manager that routes chat completions through the
//! configured providers with graceful fallbacks and structured logging.
//!
//! The manager hides the persistence and provider resolution concerns from
//! callers so that higher level modules (summariser, workers, IPC handlers)
//! can simply request a completion without caring which backend ultimately
//! fulfils it.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::task::spawn_blocking;

use crate::agents::config::{self, AiProviderInfo, AiRuntimeSelection};
use crate::agents::{AiChatInput, AiChatResponse, AiOrchestrator};
use crate::db::DbPool;
use crate::logging::log_event;

/// Wrapper that owns the orchestrator alongside access to provider metadata.
#[derive(Clone)]
pub struct ModelManager {
    pool: DbPool,
    orchestrator: Arc<AiOrchestrator>,
}

impl ModelManager {
    /// Construct a new manager backed by the given pool and orchestrator.
    pub fn new(pool: DbPool, orchestrator: Arc<AiOrchestrator>) -> Arc<Self> {
        Arc::new(Self { pool, orchestrator })
    }

    /// Return a clone of the underlying connection pool.
    pub fn pool(&self) -> DbPool {
        self.pool.clone()
    }

    /// Enumerate providers cached in SQLite.
    pub fn list_providers(&self) -> Result<Vec<AiProviderInfo>> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            config::list_providers(&conn)
        })
        .map_err(|err| anyhow!(err.to_string()))?
    }

    /// Resolve the runtime selection that should be used for a call.
    pub fn resolve_runtime(
        &self,
        provider_override: Option<String>,
        model_override: Option<String>,
        prefer_local: bool,
    ) -> Result<AiRuntimeSelection> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            resolve_with_fallback(&conn, provider_override, model_override, prefer_local)
        })
        .map_err(|err| anyhow!(err.to_string()))?
    }

    /// Execute a chat completion asynchronously, optionally overriding the
    /// provider/model. When `prefer_local` is true, local runtimes will be
    /// prioritised ahead of cloud providers during fallback selection.
    pub async fn chat(
        &self,
        input: AiChatInput,
        provider_override: Option<String>,
        model_override: Option<String>,
        prefer_local: bool,
    ) -> Result<AiChatResponse> {
        let mut attempts = Vec::new();
        attempts.push(self.resolve_runtime(
            provider_override.clone(),
            model_override.clone(),
            prefer_local,
        )?);

        // Gather any additional candidates up front so we only touch the
        // database once from the async context.
        let pool = self.pool.clone();
        let extra = spawn_blocking(move || {
            let conn = pool.get()?;
            collect_alternative_runtimes(&conn, provider_override, model_override, prefer_local)
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))??;
        attempts.extend(extra);

        let mut last_err: Option<anyhow::Error> = None;
        for selection in attempts {
            let provider_id = selection.provider.id.clone();
            let model_name = selection.model.clone();
            match self.orchestrator.chat(&selection, input.clone()).await {
                Ok(response) => {
                    log_invocation_success(&self.pool, &provider_id, &model_name, &response);
                    return Ok(response);
                }
                Err(err) => {
                    log_invocation_failure(&self.pool, &provider_id, &model_name, &err);
                    last_err = Some(err);
                    continue;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("no AI runtime available")))
    }

    /// Blocking helper that wraps [`chat`] for synchronous callers.
    pub fn chat_blocking(
        &self,
        input: AiChatInput,
        provider_override: Option<String>,
        model_override: Option<String>,
        prefer_local: bool,
    ) -> Result<AiChatResponse> {
        tauri::async_runtime::block_on(self.chat(
            input,
            provider_override,
            model_override,
            prefer_local,
        ))
    }
}

fn resolve_with_fallback(
    conn: &rusqlite::Connection,
    provider_override: Option<String>,
    model_override: Option<String>,
    prefer_local: bool,
) -> Result<AiRuntimeSelection> {
    if let Ok(selection) =
        config::resolve_runtime(conn, provider_override.clone(), model_override.clone())
    {
        return Ok(selection);
    }

    let candidates =
        collect_alternative_runtimes(conn, provider_override, model_override, prefer_local)?;
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No AI provider is configured"))
}

fn collect_alternative_runtimes(
    conn: &rusqlite::Connection,
    provider_override: Option<String>,
    model_override: Option<String>,
    prefer_local: bool,
) -> Result<Vec<AiRuntimeSelection>> {
    let providers = config::list_providers(conn)?;
    let mut attempts = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(pid) = &provider_override {
        seen.insert(pid.clone());
    }

    let mut ordered = providers;
    if prefer_local {
        ordered.sort_by_key(|p| if p.kind == "local" { 0 } else { 1 });
    }

    for provider in ordered {
        if provider.requires_api_key && !provider.has_credentials {
            continue;
        }
        if seen.contains(&provider.id) {
            continue;
        }
        if let Ok(selection) =
            config::resolve_runtime(conn, Some(provider.id.clone()), model_override.clone())
        {
            seen.insert(provider.id.clone());
            attempts.push(selection);
        }
    }

    Ok(attempts)
}

fn log_invocation_success(
    pool: &DbPool,
    provider_id: &str,
    model: &str,
    response: &AiChatResponse,
) {
    let preview = response.content.chars().take(200).collect::<String>();
    let pool = pool.clone();
    let provider = provider_id.to_string();
    let model = model.to_string();
    tokio::spawn(async move {
        if let Ok(conn) = pool.get() {
            let _ = log_event(
                &conn,
                "info",
                Some("AI-0200"),
                "ai.runtime",
                "AI chat invocation succeeded",
                Some("Model manager resolved a provider"),
                Some(serde_json::json!({
                    "provider": provider,
                    "model": model,
                    "preview": preview,
                })),
            );
        }
    });
}

fn log_invocation_failure(pool: &DbPool, provider_id: &str, model: &str, error: &anyhow::Error) {
    let pool = pool.clone();
    let provider = provider_id.to_string();
    let model = model.to_string();
    let message = error.to_string();
    tokio::spawn(async move {
        if let Ok(conn) = pool.get() {
            let _ = log_event(
                &conn,
                "warn",
                Some("AI-0201"),
                "ai.runtime",
                "AI provider invocation failed",
                Some("Attempting fallback"),
                Some(serde_json::json!({
                    "provider": provider,
                    "model": model,
                    "error": message,
                })),
            );
        }
    });
}

// Allow synchronous access to rusqlite without importing from the caller.
use r2d2_sqlite::rusqlite;
