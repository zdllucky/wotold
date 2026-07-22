-- [M16.6] Новый вид пассажа `call_meta` — синтетическая «карточка звонка»
-- (титул + дата + участники): якорь для вопросов «в каком звонке / кто был /
-- о чём» (живые фейлы M16). CHECK-констрейнт 0019 не расширяется ALTER'ом →
-- пересоздание таблицы + FTS + триггеров.
--
-- Данные ПРОИЗВОДНЫЕ (transcript.md/recap.md/structured rows): вместо
-- миграции контента чистим всё — startup-backfill (`indexer::backfill`,
-- lib.rs setup) переиндексирует ready-звонки штатным механизмом, а
-- embed-backfill пересчитает вектора (каскад/очистка ниже).

DELETE FROM assistant_embeddings;
DELETE FROM assistant_index_state;

DROP TABLE IF EXISTS assistant_fts;
-- Триггеры ai/ad/au умирают вместе с таблицей.
DROP TABLE IF EXISTS assistant_passages;

CREATE TABLE assistant_passages (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  call_id   TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL CHECK (kind IN
              ('transcript', 'recap', 'decision', 'action_item',
               'open_question', 'call_meta')),
  speaker   TEXT,
  start_ms  INTEGER,
  end_ms    INTEGER,
  text      TEXT NOT NULL,
  token_est INTEGER NOT NULL
);

CREATE INDEX assistant_passages_call_idx ON assistant_passages(call_id);

CREATE VIRTUAL TABLE assistant_fts USING fts5(
  text,
  content = 'assistant_passages',
  content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER assistant_passages_ai AFTER INSERT ON assistant_passages BEGIN
  INSERT INTO assistant_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER assistant_passages_ad AFTER DELETE ON assistant_passages BEGIN
  INSERT INTO assistant_fts(assistant_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER assistant_passages_au AFTER UPDATE ON assistant_passages BEGIN
  INSERT INTO assistant_fts(assistant_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO assistant_fts(rowid, text) VALUES (new.id, new.text);
END;
