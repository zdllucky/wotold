-- [B3.1] Cluster embedding per call_speaker — извлекается pipeline после STT
-- через Embedder (M3.1, см. embeddings.rs). Используется для:
--   1) matching против voice_samples других contacts → suggestion
--   2) при confirm c consent_voice='true' → копируется в voice_samples
--      как новый «образец» для будущего matching этого контакта
--
-- BLOB = little-endian f32 vector, EMBEDDING_DIM=256 (см. embedding_to_bytes).
-- NULL до момента когда pipeline извлёк cluster (legacy звонки + processing).

ALTER TABLE call_speakers ADD COLUMN cluster_embedding BLOB NULL;
