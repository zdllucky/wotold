// [V6.5] ErrorScreen — спокойный fail-state. Объясняет что аудио сохранено,
// показывает 3 retry actions и diagnostics block. Бывший small card с одной
// кнопкой заменён на полный layout per handoff design.
//
// provider hint извлекается из call.provider (последний фактически
// использованный STT). Кнопка «попробовать через другого провайдера»
// показывается только если provider не пустой — иначе оставляем generic.

import type { Call } from '../../api/recording';
import { CallStateTag } from '../call-state';
import { useI18n } from '../../i18n';
import { ErrorDiagnostics } from './ErrorDiagnostics';

interface ErrorScreenProps {
  call: Call;
  reprocessing: boolean;
  onRetry: () => void;
}

export function ErrorScreen({ call, reprocessing, onRetry }: ErrorScreenProps) {
  const { t } = useI18n();
  const reason = call.failed_reason?.trim() || t('callDetail.failBadge');
  const provider = call.provider?.trim() || null;
  const alternativeProvider = provider
    ? provider === 'soniox'
      ? 'gladia'
      : provider === 'gladia'
        ? 'soniox'
        : null
    : null;
  return (
    <div className="card" style={{ marginBottom: 18 }}>
      <CallStateTag state="error" />
      <h2
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 22,
          margin: '12px 0 4px',
          letterSpacing: '-0.01em',
        }}
      >
        {t('callDetail.errorTitle')}
      </h2>
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 15,
          margin: '0 0 12px',
          color: 'var(--text)',
        }}
      >
        {reason}
      </p>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 14,
          margin: '0 0 16px',
        }}
      >
        {t('callDetail.errorAudioSaved')}
      </p>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        <button
          type="button"
          className="btn btn--primary btn--sm"
          onClick={onRetry}
          disabled={reprocessing}
        >
          {reprocessing ? t('callDetail.retrying') : t('callDetail.errorRetry')}
        </button>
        {alternativeProvider && (
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={onRetry}
            disabled={reprocessing}
            title={t('callDetail.errorRetryProvider', { provider: alternativeProvider })}
          >
            {t('callDetail.errorRetryProvider', { provider: alternativeProvider })}
          </button>
        )}
      </div>
      <ErrorDiagnostics call={call} />
    </div>
  );
}
