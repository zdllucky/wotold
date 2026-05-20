-- [B16 audit P2]: call_fts virtual table создана в 0001_initial.sql но
-- никогда не populated (FTS5 implementation отложен до #30/M5.2).
-- Drop'аем чтобы не путать схему и не нагружать SQLite журналом по пустой
-- таблице. Если FTS5 понадобится — recreate в новой миграции с corrected
-- schema (например, tokenize=porter unicode61) + populate-trigger.

DROP TABLE IF EXISTS call_fts;
