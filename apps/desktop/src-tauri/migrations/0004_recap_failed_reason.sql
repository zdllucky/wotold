-- 0004 [B16 audit P0]: recap failure persistence.
--
-- Сейчас pipeline::run при ошибке recap'а делает silent log::warn и
-- продолжает (status остаётся 'ready' — транскрипт есть). UI видит
-- «Саммари ещё не сгенерировано» бесконечно, не понимая что recap
-- завалился (квота, бэкенд лёг).
--
-- Добавляем колонку `recap_failed_reason` отдельно от `failed_reason`
-- (последняя для STT/recording fail и блочит весь звонок). Если есть
-- — UI показывает «Не удалось создать саммари: {reason}» + кнопка
-- «↻ Пересоздать саммари».

ALTER TABLE calls ADD COLUMN recap_failed_reason TEXT;
