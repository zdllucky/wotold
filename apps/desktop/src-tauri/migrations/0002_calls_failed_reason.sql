-- M2.7 (#23): UX-readable причина перехода в status=failed.
-- Пишется при retries-exhausted, auth, quota_exceeded, недоступности всех STT провайдеров.
-- nullable — у ready/recording/processing reason нет.
ALTER TABLE calls ADD COLUMN failed_reason TEXT;
