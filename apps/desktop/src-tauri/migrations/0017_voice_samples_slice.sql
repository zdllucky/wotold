-- [P4] Voice sample slice metadata — start/end seconds + track kind для
-- playback короткого аудио-фрагмента вместо full source_call mic.wav.
--
-- Сейчас voice_samples хранят только embedding vector (256-dim f32) +
-- source_call FK. Невозможно reconstruct WAV slice момента где sample
-- captured → P3 playback (commit ea6b1e2) линкует full mic.wav (21+ мин),
-- даёт silence если sample был на system track.
--
-- Legacy rows: все 3 NULL → UI выключает play button c hint.
-- Future enforcement (post-backfill tool): NOT NULL CHECK через follow-up.
--
-- track_kind whitelist: 'mic' | 'system'. CHECK constraint defensive.

ALTER TABLE voice_samples ADD COLUMN start_sec REAL;
ALTER TABLE voice_samples ADD COLUMN end_sec REAL;
ALTER TABLE voice_samples ADD COLUMN track_kind TEXT
  CHECK (track_kind IS NULL OR track_kind IN ('mic', 'system'));
