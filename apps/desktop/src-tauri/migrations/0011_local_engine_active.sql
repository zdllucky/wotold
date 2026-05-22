-- [M12.6] Local engine selector — backfill `local_engine.active` setting.
--
-- Существующие пользователи: маппим `provider_path` → новый ключ
-- (managed → cloud_managed, byo → cloud_byo). Свежие установки (никакой
-- provider_path не выставлен) получают `local` по дефолту — PRD §M12.6.1.
--
-- Идемпотентно: вставка только если ключ ещё не существует. Перезапуск
-- миграции (теоретически) не изменит выбор пользователя.
--
-- ВАЖНО: ELSE 'local' срабатывает и для свежих установок (нет
-- `provider_path` вообще), и для существующих установок с unknown/legacy
-- значениями (например ручная правка БД). Это сознательное решение per
-- PRD §M12.6.1 — unknown НЕ ошибка, а fallback на новый default. UI
-- (M12.5) даёт юзеру переключить engine в любой момент.

INSERT INTO settings (key, value)
SELECT
    'local_engine.active',
    CASE
        WHEN (SELECT value FROM settings WHERE key = 'provider_path') = 'managed' THEN 'cloud_managed'
        WHEN (SELECT value FROM settings WHERE key = 'provider_path') = 'byo' THEN 'cloud_byo'
        ELSE 'local'
    END
WHERE NOT EXISTS (SELECT 1 FROM settings WHERE key = 'local_engine.active');
