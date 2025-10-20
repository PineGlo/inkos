PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS ai_providers (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  description TEXT,
  base_url TEXT,
  default_model TEXT,
  models_json TEXT NOT NULL,
  capabilities_json TEXT NOT NULL,
  requires_api_key INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_credentials (
  provider_id TEXT PRIMARY KEY,
  secret TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (provider_id) REFERENCES ai_providers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
