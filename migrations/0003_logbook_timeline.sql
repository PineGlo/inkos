PRAGMA foreign_keys = ON;

ALTER TABLE jobs ADD COLUMN result TEXT;

CREATE TABLE IF NOT EXISTS logbook_entries (
  id TEXT PRIMARY KEY,
  entry_date TEXT NOT NULL UNIQUE,
  summary TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS timeline_events (
  id TEXT PRIMARY KEY,
  entry_date TEXT NOT NULL,
  event_time INTEGER NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  detail TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_timeline_events_entry_date ON timeline_events(entry_date);
