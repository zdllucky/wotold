-- [M15.2] Ассистент: чаты, сообщения, поисковый индекс (FTS5).
--
-- Закрывает follow-up #30 (FTS5, дропнутый в 0006): полнотекст возвращается
-- как external-content таблица assistant_fts над assistant_passages,
-- синхронизация — ТОЛЬКО триггерами (каскадный DELETE от calls активирует
-- delete-триггер → FTS чистится сам, рассинхронизация невозможна).
--
-- ВАЖНО: запись в assistant_passages — строго через db/assistant.rs
-- (см. doc-блок репозитория). PRD: docs/M15_ASSISTANT_PRD.md §5.1.

-- Чаты. call_id IS NULL — глобальный чат раздела «Ассистент»;
-- call_id NOT NULL — единственный персистентный тред звонка.
CREATE TABLE IF NOT EXISTS assistant_chats (
  id         TEXT PRIMARY KEY,
  call_id    TEXT REFERENCES calls(id) ON DELETE CASCADE,
  title      TEXT NOT NULL,               -- первый вопрос, усечённый ~42 симв (в Rust)
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS assistant_chats_call_uniq
  ON assistant_chats(call_id) WHERE call_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS assistant_chats_updated_idx
  ON assistant_chats(updated_at DESC);

-- Сообщения. answer_json — сериализованный AssistantAnswer (contract S2)
-- для role='assistant'; text дублирует ans.text для копирования без парсинга.
CREATE TABLE IF NOT EXISTS assistant_messages (
  id          TEXT PRIMARY KEY,
  chat_id     TEXT NOT NULL REFERENCES assistant_chats(id) ON DELETE CASCADE,
  role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
  text        TEXT NOT NULL,
  answer_json TEXT,
  order_idx   INTEGER NOT NULL,
  created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS assistant_messages_chat_idx
  ON assistant_messages(chat_id, order_idx ASC);

-- Пассажи индекса. INTEGER PRIMARY KEY = rowid — external content id для FTS.
-- kind: transcript | recap | decision | action_item | open_question.
CREATE TABLE IF NOT EXISTS assistant_passages (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  call_id   TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL CHECK (kind IN
              ('transcript', 'recap', 'decision', 'action_item', 'open_question')),
  speaker   TEXT,
  start_ms  INTEGER,
  end_ms    INTEGER,
  text      TEXT NOT NULL,
  token_est INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS assistant_passages_call_idx
  ON assistant_passages(call_id);

-- Полнотекст: external content над assistant_passages.
-- unicode61 без стемминга; русская морфология компенсируется
-- префикс-экспансией в retrieval (M15.5) и гибридом с эмбеддером (Ph2).
CREATE VIRTUAL TABLE IF NOT EXISTS assistant_fts USING fts5(
  text,
  content = 'assistant_passages',
  content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS assistant_passages_ai AFTER INSERT ON assistant_passages BEGIN
  INSERT INTO assistant_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS assistant_passages_ad AFTER DELETE ON assistant_passages BEGIN
  INSERT INTO assistant_fts(assistant_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER IF NOT EXISTS assistant_passages_au AFTER UPDATE ON assistant_passages BEGIN
  INSERT INTO assistant_fts(assistant_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO assistant_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Состояние индексации per звонок (для backfill-sweep и чипа статистики).
CREATE TABLE IF NOT EXISTS assistant_index_state (
  call_id       TEXT PRIMARY KEY REFERENCES calls(id) ON DELETE CASCADE,
  indexed_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  passage_count INTEGER NOT NULL DEFAULT 0,
  token_total   INTEGER NOT NULL DEFAULT 0
);
