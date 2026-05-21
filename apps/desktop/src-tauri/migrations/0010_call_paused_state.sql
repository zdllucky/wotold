-- [W2] Поддержка pause/resume для recording sessions.
-- paused_at TEXT (RFC3339) — non-NULL только когда status='recording' AND on pause.
-- paused_total_ms INTEGER — накопленная пауза в миллисекундах, используется
-- pipeline'ом для корректного elapsed/duration вычисления.

ALTER TABLE calls ADD COLUMN paused_at TEXT;
ALTER TABLE calls ADD COLUMN paused_total_ms INTEGER NOT NULL DEFAULT 0;
