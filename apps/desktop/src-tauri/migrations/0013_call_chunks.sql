-- [M13.1.4] Chunked pipelined transcription — per-chunk record для каждой
-- записи звонка. Один call → 1..N chunks (1 если запись короче 10 минут,
-- N = ceil(duration / 10min) ± 1 для silence-aware cut).
--
-- Per-chunk pipeline (M13.1.3) запускается асинхронно как только chunk
-- закрыт sidecar'ом rotate-командой. После stop() всех чанков — global
-- speaker re-clustering (M13.2.1) + concat + LLM recap.
--
-- Status FSM:
--   pending   → chunk WAV закрыт sidecar'ом, ждёт pipeline pickup
--   processing → STT/diarization/embedding в работе
--   done      → transcript_json заполнен, embeddings persisted
--   failed    → fatal error в pipeline, см. failure_kind в logs
CREATE TABLE call_chunks (
    call_id TEXT NOT NULL,
    chunk_idx INTEGER NOT NULL,

    -- Временные границы относительно начала записи (для timestamp offset
    -- merge во время global concat).
    start_ms INTEGER NOT NULL,
    end_ms INTEGER,  -- NULL пока chunk не закрыт (последний во время stop)

    -- Абсолютные пути WAV-файлов в `$APP_DATA/calls/<call_id>/chunks/N/`.
    mic_path TEXT NOT NULL,
    system_path TEXT NOT NULL,

    status TEXT NOT NULL DEFAULT 'pending',
    -- Сериализованный DiarizedTranscript для этого чанка (после pipeline
    -- pickup); NULL пока в processing/pending/failed.
    transcript_json TEXT,
    -- Сериализованный Vec<(local_speaker_id, embedding[256])> per segment,
    -- используется в global speaker re-clustering. NULL до pipeline done.
    embeddings_json TEXT,

    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (call_id, chunk_idx),
    FOREIGN KEY (call_id) REFERENCES calls(id) ON DELETE CASCADE
);

-- Pipeline workers polling pending chunks ORDER BY (call_id, chunk_idx).
CREATE INDEX idx_call_chunks_status ON call_chunks(status, call_id, chunk_idx);
