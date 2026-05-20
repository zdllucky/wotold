-- [B16 audit P2 / #25 follow-up]: embedding_dim в voice_samples.
--
-- Зачем: matching pipeline (cosine_similarity) требует чтобы embedding ВСЕХ
-- семплов имел один и тот же размер. Сейчас в БД лежит только BLOB без
-- метаданных — если поменяется ONNX модель и dim изменится с 256 → 512,
-- старые семплы беспроверочно склеятся с новыми, дав NaN/garbage similarity.
--
-- Решение: добавить INTEGER колонку embedding_dim. Insert-time валидация в
-- Rust-коде должна проверять что новый sample совпадает по dim с тем что
-- уже в БД (для same contact). При mismatch — отдельный код-pathway (drop +
-- re-extract или skip), не silent corruption.
--
-- Backfill: для старых записей до этой миграции — derive из length(embedding)
-- (f32 = 4 байта), а если file пуст — NULL.

ALTER TABLE voice_samples ADD COLUMN embedding_dim INTEGER;

-- Backfill: размер blob в байтах / 4 = количество f32-чисел = dim.
UPDATE voice_samples
   SET embedding_dim = length(embedding) / 4
 WHERE embedding_dim IS NULL
   AND embedding IS NOT NULL
   AND length(embedding) > 0;
