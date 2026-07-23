-- 0022: local-only переход — вычистить облачные настройки.
--
-- Cloud/proxy-путь (managed STT Soniox/Gladia, cloud LLM, auth/SSO, квота)
-- удалён; локальный движок — единственный путь обработки. Настройки, которые
-- управляли выбором cloud/BYO-провайдера и proxy, больше не читаются кодом —
-- удаляем их, чтобы существующие установки (в т.ч. бывшие `cloud_managed`)
-- шли по локальному пути без мусорных ключей.
--
-- Миграции append-only: 0011 (backfill provider_path → local_engine.active)
-- не редактируется; этот шаг лишь нейтрализует её эффект для local-only.
DELETE FROM settings
WHERE key IN (
    'provider_path',
    'proxy_base_url',
    'stt_provider',
    'llm_model',
    'local_engine.active'
);
