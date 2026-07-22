-- [B23] Sync-ready contacts schema (паспорт M5.3; точка расширения для M6.4
-- импорта — DEFERRED). Поведение не меняется: source по умолчанию 'local',
-- external_* остаются NULL до появления синка/импорта.

ALTER TABLE contacts ADD COLUMN source TEXT NOT NULL DEFAULT 'local';
ALTER TABLE contacts ADD COLUMN external_id TEXT;
ALTER TABLE contacts ADD COLUMN external_etag TEXT;

-- vCard-метка (home/work) для будущего импорта; UI её пока не показывает.
ALTER TABLE contact_identifiers ADD COLUMN label TEXT;

-- Legacy replace-all update мог накопить дубли (contact_id, kind, value);
-- CREATE UNIQUE INDEX падает на дублях — сначала чистим, оставляя самую
-- раннюю строку по rowid (детерминированно).
DELETE FROM contact_identifiers
 WHERE rowid NOT IN (
   SELECT MIN(rowid)
     FROM contact_identifiers
    GROUP BY contact_id, kind, value
 );

CREATE UNIQUE INDEX IF NOT EXISTS contact_identifiers_uniq
  ON contact_identifiers(contact_id, kind, value);

-- Один локальный контакт на одну запись провайдера. Partial: у локальных
-- контактов external_id IS NULL — они под индекс не попадают.
CREATE UNIQUE INDEX IF NOT EXISTS contacts_external_uniq
  ON contacts(source, external_id)
  WHERE external_id IS NOT NULL;
