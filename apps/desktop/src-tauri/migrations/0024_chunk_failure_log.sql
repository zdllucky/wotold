-- 0024: журнал падений чанков.
--
-- `call_chunks.status = 'failed'` — состояние, а не история: ретрай перетирает
-- его на 'pending', и после успешной второй попытки от падения не остаётся
-- следа. Поэтому «часто ли вообще падают чанки и на чём» по базе не
-- восстанавливалось никак — только гриппингом логов, которые ротируются.
--
-- Строка пишется на каждый переход `* → failed`, включая повторные: retry_idx
-- считает, какая это по счёту неудача для той же пары (call_id, chunk_idx),
-- поэтому «упало один раз и починилось ретраем» отличимо от «падает всегда».
--
-- preset фиксируется на момент падения (light/balanced/quality или 'unknown'):
-- разбивка по пресету — главный вопрос к этим данным, а настройку пользователь
-- меняет, и задним числом её уже не восстановить.
--
-- Local-only, как и summary_generation_log: никаких сетевых отправок.
-- CASCADE по call_id — удаление звонка уносит и его записи.

CREATE TABLE IF NOT EXISTS chunk_failure_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  chunk_idx INTEGER NOT NULL,
  reason TEXT NOT NULL,
  retry_idx INTEGER NOT NULL,
  preset TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_chunk_failure_call ON chunk_failure_log(call_id);
CREATE INDEX IF NOT EXISTS idx_chunk_failure_created ON chunk_failure_log(created_at DESC);
