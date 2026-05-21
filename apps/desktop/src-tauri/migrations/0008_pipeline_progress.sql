-- [V6.2] Pipeline progress fields для async-states UI (CallStateTag,
-- ProgressRail, PipelineStrip).
--
-- DB is source of truth — фронт пере-загружает state на reload и не теряет
-- прогресс если окно было закрыто. Tauri-событие call:progress эмитится из
-- pipeline'а параллельно UPDATE — UI получает live tick без polling'а.
--
-- step (1..5)         — текущий шаг pipeline'а:
--                        1=upload, 2=transcribe, 3=recognize_speakers,
--                        4=merge_artifacts, 5=recap.
-- pct (0..100)        — completion внутри шага (или overall — UI решает).
-- eta_sec             — heuristic remaining seconds (опц., NULL когда неизвестно).
-- upload_bytes        — байтов аплоада уже отправлено (для step=1).
--
-- Все nullable — старые rows и шаги без прогресса остаются с NULL.

ALTER TABLE calls ADD COLUMN pipeline_step       INTEGER;
ALTER TABLE calls ADD COLUMN pipeline_pct        INTEGER;
ALTER TABLE calls ADD COLUMN pipeline_eta_sec    INTEGER;
ALTER TABLE calls ADD COLUMN upload_bytes        INTEGER;
