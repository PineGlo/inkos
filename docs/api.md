# IPC API v1

The InkOS frontend talks to the Rust core through Tauri's `invoke` bridge. The commands below are registered in `core/src/api/v1.rs`.

## Health & Database

### `ping`
Returns `{ ok: true, ts: <unix timestamp> }` as a basic liveness check.

### `db_status`
Validates the SQLite schema and returns the list of tables.

## Notes Sandbox

### `create_note`
Create a note with `{ title: string, body?: string }` and returns `{ id }`.

### `list_notes`
List note summaries. Accepts an optional `{ q: string }` for FTS searches.

## AI Runtime Management

Phase 0 now exposes a hybrid AI layer that supports premium cloud models (OpenAI, Anthropic, Google) and local engines (Ollama, LM Studio).

### `ai_list_providers`
Returns an array of provider descriptors. Each object contains:

```json
{
  "id": "openai",
  "kind": "cloud" | "local",
  "display_name": "OpenAI GPT-4o",
  "description": "...",
  "base_url": "https://api.openai.com",
  "default_model": "gpt-4o-mini",
  "models": ["gpt-4o", "gpt-4o-mini"],
  "capability_tags": ["chat", "multimodal"],
  "requires_api_key": true,
  "has_credentials": false
}
```

### `ai_get_settings`
Returns the active provider snapshot:

```json
{
  "active_provider_id": "openai",
  "active_model": "gpt-4o-mini",
  "provider": { ...same structure as above... }
}
```

### `ai_update_settings`
Persists provider selection, credentials, and local endpoint overrides.

Payload:

```json
{
  "provider_id": "openai",
  "model": "gpt-4o",
  "api_key": "sk-...", // omit to keep existing, empty string to clear
  "base_url": "https://api.openai.com"
}
```

Returns an updated `ai_get_settings` snapshot. All secrets are stored base64 encoded in the workspace database.

### `ai_chat`
Invokes the orchestrator with chat-style prompts.

Payload:

```json
{
  "provider_id": "openai", // optional, falls back to the active provider
  "model": "gpt-4o",
  "temperature": 0.2,
  "messages": [
    { "role": "system", "content": "You are InkOS." },
    { "role": "user", "content": "Hello!" }
  ]
}
```

Response:

```json
{
  "provider_id": "openai",
  "model": "gpt-4o",
  "content": "Hi there!",
  "usage": { "prompt_tokens": 12, "completion_tokens": 10, "total_tokens": 22 },
  "raw": { /* provider-specific payload */ }
}
```

Errors are bubbled back as strings and also logged into the `event_log` table with code `AI-0100`.
