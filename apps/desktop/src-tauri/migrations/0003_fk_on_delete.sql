-- 0003 [B16 audit P0]: добавляем недостающие ON DELETE-правила и FK
-- констрэйнты, чтобы каскадные удаления контактов / звонков не оставляли
-- висящих ссылок и блокирующих NO ACTION FK errors.
--
-- Меняем:
--   call_speakers.contact_id      → ON DELETE SET NULL
--   call_speakers.suggestion_contact_id → FK + ON DELETE SET NULL (был просто TEXT)
--   action_items.owner_contact_id → ON DELETE SET NULL
--   voice_samples.source_call     → FK на calls(id) + ON DELETE SET NULL
--
-- SQLite не поддерживает ALTER TABLE для FK — стандартная процедура
-- create-new + copy + drop-old + rename. Foreign keys временно ВЫКЛЮЧЕНЫ
-- внутри migration чтобы не блокировать data copy.

PRAGMA foreign_keys = OFF;

-- ============================================================
-- call_speakers
-- ============================================================

CREATE TABLE call_speakers_new (
  id                    TEXT PRIMARY KEY,
  call_id               TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  speaker_tag           TEXT NOT NULL,
  contact_id            TEXT REFERENCES contacts(id) ON DELETE SET NULL,
  suggestion_contact_id TEXT REFERENCES contacts(id) ON DELETE SET NULL,
  suggestion_score      REAL,
  suggestion_source     TEXT,
  confirmed             INTEGER NOT NULL DEFAULT 0,
  embedding             BLOB
);

INSERT INTO call_speakers_new SELECT * FROM call_speakers;
DROP TABLE call_speakers;
ALTER TABLE call_speakers_new RENAME TO call_speakers;
CREATE INDEX IF NOT EXISTS call_speakers_call_idx ON call_speakers(call_id);

-- ============================================================
-- action_items
-- ============================================================

CREATE TABLE action_items_new (
  id                TEXT PRIMARY KEY,
  call_id           TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  text              TEXT NOT NULL,
  owner_contact_id  TEXT REFERENCES contacts(id) ON DELETE SET NULL,
  due               TEXT,
  done              INTEGER NOT NULL DEFAULT 0
);

INSERT INTO action_items_new SELECT * FROM action_items;
DROP TABLE action_items;
ALTER TABLE action_items_new RENAME TO action_items;
CREATE INDEX IF NOT EXISTS action_items_call_idx ON action_items(call_id);

-- ============================================================
-- voice_samples — добавляем FK на calls.id и index на source_call
-- ============================================================

CREATE TABLE voice_samples_new (
  id          TEXT PRIMARY KEY,
  contact_id  TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
  embedding   BLOB NOT NULL,
  source_call TEXT REFERENCES calls(id) ON DELETE SET NULL,
  quality     REAL,
  created_at  TEXT NOT NULL
);

INSERT INTO voice_samples_new SELECT * FROM voice_samples;
DROP TABLE voice_samples;
ALTER TABLE voice_samples_new RENAME TO voice_samples;
CREATE INDEX IF NOT EXISTS voice_samples_contact_idx ON voice_samples(contact_id);
-- Новый index для delete_call_and_samples (был full table scan).
CREATE INDEX IF NOT EXISTS voice_samples_source_call_idx ON voice_samples(source_call);

PRAGMA foreign_keys = ON;
