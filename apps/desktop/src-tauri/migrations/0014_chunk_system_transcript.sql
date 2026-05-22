-- [M13.1.5d] Dual-track chunks. Existing `transcript_json` хранит mic;
-- new `system_transcript_json` — system track (FaceTime/Zoom собеседник).
-- Phase 1 chunk_runner транскрибирует обе дорожки параллельно per chunk,
-- assembly в pipeline::chunk_assembly склеивает их обратно в две
-- DiarizedTranscript с timestamp offset'ами.
--
-- NULL допустим: chunks от M13.1.5c (когда chunk_runner был mic-only) или
-- chunks где system STT упал degraded-ok — assembly обрабатывает None как
-- «пустой system track для этого chunk'а».
ALTER TABLE call_chunks ADD COLUMN system_transcript_json TEXT;
