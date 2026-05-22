-- [M12.4.4-bis] Storage management — last_used_at per model.
--
-- PRD v0.2 §M12.5.2 table shows: name · size · last_used_at · ✓активна
-- × delete. last_used_at нужен чтобы UI отсортировал «давно не использовалось».
-- Колонка пишется pipeline'ом при success completion call'а с этим preset'ом
-- (M12.6 phase 3 wire-up — пока пишется только при явных `model_use` вызовах).
--
-- Минимальная таблица: id PK (matches MODEL_CATALOG.id), last_used_at RFC3339.
-- При delete модели — row остаётся (история). При reset через `wipe_all_data` —
-- очищается каскадом.

CREATE TABLE IF NOT EXISTS local_engine_model_usage (
    model_id TEXT PRIMARY KEY,
    last_used_at TEXT NOT NULL
);
