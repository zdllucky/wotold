-- [M14 foundation] Summary v2 schema: type-driven evidence-grounded recap.
-- См. docs/M14_SUMMARY_V2_PRD.md §4.1.
--
-- Adaptations from PRD:
-- - `summary_id` → `call_id` (нет отдельной таблицы summaries; recap.md
--   живёт на диске, metadata уезжает в `calls`).
-- - `language` skip — `calls.lang_detected` уже существует.
-- - `schema_version` → `summary_schema_version` (избегаем конфликта с
--   другими version полями в будущих миграциях).
-- - Non-destructive: ALTER ADD COLUMN, existing rows получают NULL.

-- ─── Calls: per-summary metadata ─────────────────────────────────────────
ALTER TABLE calls ADD COLUMN call_type TEXT;
ALTER TABLE calls ADD COLUMN call_type_confidence REAL;
ALTER TABLE calls ADD COLUMN summary_schema_version INTEGER DEFAULT 1;
ALTER TABLE calls ADD COLUMN summary_engine TEXT;
ALTER TABLE calls ADD COLUMN summary_pipeline_mode TEXT;
ALTER TABLE calls ADD COLUMN summary_generation_ms INTEGER;
ALTER TABLE calls ADD COLUMN summary_input_tokens INTEGER;
ALTER TABLE calls ADD COLUMN summary_output_tokens INTEGER;
-- type_specific_block — serialized JSON per call_type (pain_points для
-- sales_discovery, per_person для standup, и т.д.). NULL для legacy / other.
ALTER TABLE calls ADD COLUMN summary_type_specific_block TEXT;

-- ─── Action items: confidence + category + evidence anchor ──────────────
ALTER TABLE action_items ADD COLUMN owner_confidence REAL;
ALTER TABLE action_items ADD COLUMN due_confidence REAL;
-- category: 'commitment' (explicit accept) | 'proposal' (suggested) | 'idea' (raised, no action).
ALTER TABLE action_items ADD COLUMN category TEXT DEFAULT 'commitment';
ALTER TABLE action_items ADD COLUMN evidence_quote TEXT;
ALTER TABLE action_items ADD COLUMN evidence_speaker TEXT;
ALTER TABLE action_items ADD COLUMN evidence_start_ms INTEGER;

-- ─── Decisions: explicit choices made during the call ───────────────────
CREATE TABLE IF NOT EXISTS decisions (
  id TEXT PRIMARY KEY,
  call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  evidence_quote TEXT,
  evidence_speaker TEXT,
  evidence_start_ms INTEGER,
  evidence_end_ms INTEGER,
  confidence REAL,
  order_idx INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- ─── Open questions: unresolved items raised during the call ────────────
CREATE TABLE IF NOT EXISTS open_questions (
  id TEXT PRIMARY KEY,
  call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  raised_by TEXT,
  evidence_quote TEXT,
  evidence_speaker TEXT,
  evidence_start_ms INTEGER,
  order_idx INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- ─── Indices ────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_calls_call_type ON calls(call_type);
CREATE INDEX IF NOT EXISTS idx_calls_summary_engine ON calls(summary_engine);
CREATE INDEX IF NOT EXISTS idx_action_items_category ON action_items(category);
CREATE INDEX IF NOT EXISTS idx_decisions_call ON decisions(call_id);
CREATE INDEX IF NOT EXISTS idx_open_questions_call ON open_questions(call_id);
