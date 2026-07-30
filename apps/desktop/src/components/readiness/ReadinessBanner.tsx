// Баннер «не хватает софта» — единственное место, откуда стартует докачка.
//
// Почему не тост: тост транзиентен по устройству (авто-скрытие, пауза по
// наведению, одно действие) и его API зафиксирован тестами на `show()`.
// Нехватка модулей — состояние, а не событие: оно живёт, пока его не устранят,
// и обязано показывать прогресс.

import { Button, Icon, Progress } from '../../ui';
import { useI18n } from '../../i18n';
import { isMissingModules, needsPresetChoice, useReadiness } from './ReadinessProvider';

function formatSize(bytes: number): string {
  if (bytes < 1024 ** 3) return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

export function ReadinessBanner({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const { t } = useI18n();
  const state = useReadiness();
  const { readiness, downloading, aggregate, ensure, lastError } = state;

  if (!readiness || readiness.ready) return null;

  // Размер движка не выбран — качать нечего, ведём в настройки. Скачивание
  // «на угад» с баннера было бы сюрпризом на несколько гигабайт.
  if (needsPresetChoice(state)) {
    return (
      <div className="readiness-banner" role="status">
        <div className="readiness-banner-body">
          <span className="readiness-banner-title">{t('readiness.eyebrow')}</span>
          <span className="readiness-banner-text">{t('readiness.choosePreset')}</span>
        </div>
        {onOpenSettings && (
          <Button variant="primary" size="sm" onClick={onOpenSettings}>
            {t('readiness.openSettings')}
          </Button>
        )}
      </div>
    );
  }

  if (!isMissingModules(state)) return null;

  const busy = downloading || (aggregate != null && aggregate.doneBytes > 0);

  return (
    <div className="readiness-banner" role="status">
      <div className="readiness-banner-body">
        <span className="readiness-banner-title">{t('readiness.eyebrow')}</span>
        <span className="readiness-banner-text">
          {busy
            ? t('readiness.downloading', { pct: aggregate?.pct ?? 0 })
            : t('readiness.missing', { size: formatSize(readiness.missing_bytes_total) })}
        </span>
        {busy && aggregate && (
          <>
            <Progress value={aggregate.pct} ariaLabel={t('readiness.downloadingAria')} />
            <span className="readiness-banner-meta mono">
              {formatSize(aggregate.doneBytes)} / {formatSize(aggregate.totalBytes)}
            </span>
          </>
        )}
        {lastError && (
          <span className="readiness-banner-error" role="alert">
            {lastError}
          </span>
        )}
      </div>
      {!busy && (
        <Button
          variant="primary"
          size="sm"
          leading={<Icon name="download" size={14} />}
          onClick={ensure}
        >
          {lastError ? t('readiness.retry') : t('readiness.download')}
        </Button>
      )}
    </div>
  );
}
