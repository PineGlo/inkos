PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'todo',
  due_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS links (
  id TEXT PRIMARY KEY,
  src_id TEXT NOT NULL,
  src_type TEXT NOT NULL,
  dst_id TEXT NOT NULL,
  dst_type TEXT NOT NULL,
  rel TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'queued',
  payload TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  run_at INTEGER
);

CREATE TABLE IF NOT EXISTS event_log (
  id TEXT PRIMARY KEY,
  ts INTEGER NOT NULL,
  level TEXT NOT NULL,
  code TEXT,
  module TEXT,
  message TEXT NOT NULL,
  explain TEXT,
  data TEXT
);

-- FTS for notes
CREATE VIRTUAL TABLE IF NOT EXISTS fts_notes USING fts5(
  title, body, content='notes', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
  INSERT INTO fts_notes(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
  INSERT INTO fts_notes(fts_notes, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
  INSERT INTO fts_notes(fts_notes, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
  INSERT INTO fts_notes(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
