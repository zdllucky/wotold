-- [V7] Audit trail для авто-привязанных speaker'ов. R2 паспорта говорит
-- «никакой автопривязки без подтверждения»; этот столбец — для
-- opt-in auto-bind фичи (Settings → Транскрипция → «Автоматически
-- привязывать собеседника»). NULL = ручное подтверждение, не-NULL =
-- авто-привязка (RFC3339 timestamp + UI рендерит «↩ отменить» баннер).
--
-- Не меняет existing semantics confirmed=1: ручной и авто-confirmed оба
-- одинаково confirmed для pipeline'а и UI; auto_bound_at просто
-- отличает provenance для undo/audit.

ALTER TABLE call_speakers ADD COLUMN auto_bound_at TEXT;
