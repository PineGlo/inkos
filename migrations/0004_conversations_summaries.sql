PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY,
  title TEXT,
  provider_id TEXT,
  model_id TEXT,
  ctx_warn INTEGER NOT NULL DEFAULT 0,
  ctx_force INTEGER NOT NULL DEFAULT 0,
  quality_flags TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER
);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL,
  role TEXT NOT NULL,
  body TEXT NOT NULL,
  token_est INTEGER,
  quality_flags TEXT,
  created_at INTEGER NOT NULL,
  FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);

CREATE TABLE IF NOT EXISTS summaries (
  id TEXT PRIMARY KEY,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  body TEXT NOT NULL,
  token_est INTEGER,
  source_hash TEXT,
  model_id TEXT,
  created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_summaries_target_version ON summaries(target_type, target_id, version);
CREATE INDEX IF NOT EXISTS idx_summaries_target_created ON summaries(target_type, target_id, created_at DESC);

INSERT INTO app_settings (key, value, updated_at)
SELECT 'ai.rollover.warn_ratio', '0.75', strftime('%s','now')
WHERE NOT EXISTS (SELECT 1 FROM app_settings WHERE key = 'ai.rollover.warn_ratio');

INSERT INTO app_settings (key, value, updated_at)
SELECT 'ai.rollover.force_ratio', '0.9', strftime('%s','now')
WHERE NOT EXISTS (SELECT 1 FROM app_settings WHERE key = 'ai.rollover.force_ratio');

INSERT INTO app_settings (key, value, updated_at)
SELECT 'ai.summarizer_model', '""', strftime('%s','now')
WHERE NOT EXISTS (SELECT 1 FROM app_settings WHERE key = 'ai.summarizer_model');
