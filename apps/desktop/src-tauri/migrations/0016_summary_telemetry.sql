-- [M14 T-14] Summary generation telemetry log.
-- Local-only счётчик per-recap для будущей analytics UI (M14.5). R8 / R7 —
-- никаких сетевых отправок, всё в SQLite пользователя.
--
-- Columns:
-- - engine: 'cloud-managed' | 'local-qwen-1.5b' | 'local-qwen-3b' | ...
-- - schema_version: 1 (legacy v1 path) | 2 (v2 cloud_universal path)
-- - flag_state: 0 = пользователь выключил v2 в Settings, 1 = ON
-- - generation_ms: end-to-end LLM call duration (включая retry/backoff)
-- - created_at: ISO-8601 UTC timestamp (consistent с calls.created_at)
--
-- CASCADE: при delete_call_and_samples лог тоже удаляется (privacy hygiene).

CREATE TABLE IF NOT EXISTS summary_generation_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  engine TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  flag_state INTEGER NOT NULL,
  generation_ms INTEGER NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_summary_log_call ON summary_generation_log(call_id);
CREATE INDEX IF NOT EXISTS idx_summary_log_created ON summary_generation_log(created_at DESC);
