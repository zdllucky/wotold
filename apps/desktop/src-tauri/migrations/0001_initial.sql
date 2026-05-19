-- Раздел 6.2 паспорта: исходная схема SQLite.
-- TEXT для timestamp'ов (RFC 3339), UUID-строки в качестве PK для sync-friendly будущего.

CREATE TABLE IF NOT EXISTS contacts (
  id            TEXT PRIMARY KEY,
  display_name  TEXT NOT NULL,
  is_owner      INTEGER NOT NULL DEFAULT 0,
  org           TEXT,
  role          TEXT,
  attributes    TEXT,
  notes         TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contact_identifiers (
  id          TEXT PRIMARY KEY,
  contact_id  TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL,
  value       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS contact_identifiers_contact_idx
  ON contact_identifiers(contact_id);

-- M6.1 + M3.6: несколько голосовых семплов на контакт, N последних качественных.
CREATE TABLE IF NOT EXISTS voice_samples (
  id          TEXT PRIMARY KEY,
  contact_id  TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  embedding   BLOB NOT NULL,
  source_call TEXT,
  quality     REAL,
  created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS voice_samples_contact_idx
  ON voice_samples(contact_id);

CREATE TABLE IF NOT EXISTS calls (
  id            TEXT PRIMARY KEY,
  title         TEXT,
  started_at    TEXT NOT NULL,
  ended_at      TEXT,
  duration_sec  INTEGER,
  status        TEXT NOT NULL,           -- recording|processing|ready|failed
  provider      TEXT,                    -- soniox|gladia|...
  path_label    TEXT NOT NULL,           -- managed|byo (M2.3)
  lang_detected TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS calls_started_at_idx
  ON calls(started_at DESC);

-- M3.4: подсказка системы + источник сигнала; финальная привязка только пользователем.
CREATE TABLE IF NOT EXISTS call_speakers (
  id                    TEXT PRIMARY KEY,
  call_id               TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  speaker_tag           TEXT NOT NULL,
  contact_id            TEXT REFERENCES contacts(id),
  suggestion_contact_id TEXT,
  suggestion_score      REAL,
  suggestion_source     TEXT,            -- embedding|llm|both
  confirmed             INTEGER NOT NULL DEFAULT 0,
  embedding             BLOB
);
CREATE INDEX IF NOT EXISTS call_speakers_call_idx
  ON call_speakers(call_id);

CREATE TABLE IF NOT EXISTS action_items (
  id                TEXT PRIMARY KEY,
  call_id           TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  text              TEXT NOT NULL,
  owner_contact_id  TEXT REFERENCES contacts(id),
  due               TEXT,
  done              INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS action_items_call_idx
  ON action_items(call_id);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT
);

-- M5.2: FTS5 по контенту звонков.
CREATE VIRTUAL TABLE IF NOT EXISTS call_fts USING fts5(
  call_id,
  title,
  transcript,
  recap,
  tokenize = 'unicode61'
);
