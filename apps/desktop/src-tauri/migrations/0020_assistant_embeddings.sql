-- [M15.10] Ph2 ассистента: вектора пассажей для семантического канала
-- retrieval (гибрид RRF, PRD §5.2/§6.3).
--
-- Отдельная таблица, НЕ колонка assistant_passages: backfill эмбеддингов
-- асинхронный и не трогает FTS-триггеры 0019. Каскад: DELETE звонка →
-- каскад assistant_passages → каскад сюда; полная переиндексация
-- (replace_call_passages = DELETE+INSERT) автоматически сбрасывает вектора.
--
-- vec — little-endian f32 (embeddings.rs::embedding_to_bytes). dim per-row:
-- текстовый e5-small = 384 и НЕ равен голосовому EMBEDDING_DIM = 256;
-- per-row dim позволяет сменить модель эмбеддера без миграции схемы.
CREATE TABLE IF NOT EXISTS assistant_embeddings (
  passage_id INTEGER PRIMARY KEY REFERENCES assistant_passages(id) ON DELETE CASCADE,
  dim        INTEGER NOT NULL,
  vec        BLOB NOT NULL
);
