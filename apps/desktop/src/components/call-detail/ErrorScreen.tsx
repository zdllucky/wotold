import type { Call } from '../../api/recording';
import { CallStateTag } from '../call-state';
import { useI18n } from '../../i18n';
import { ErrorDiagnostics } from './ErrorDiagnostics';
import { mapFailureToUxKind } from '../../utils/failureKind';

interface ErrorScreenProps {
  call: Call;
  reprocessing: boolean;
  onRetry: () => void;
}

export function ErrorScreen({ call, reprocessing, onRetry }: ErrorScreenProps) {
  const { t } = useI18n();
  const kind = mapFailureToUxKind(call.failed_reason ?? null);

  if (kind === 'broken_recording') {
    return (
      <div className="card" style={{ marginBottom: 18 }}>
        <p
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: 10,
            textTransform: 'uppercase',
            letterSpacing: '0.1em',
            color: 'var(--signal)',
            marginBottom: 8,
          }}
        >
          {t('failure.brokenRecording.eyebrow')}
        </p>
        <h2
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 22,
            margin: '0 0 8px',
            letterSpacing: '-0.01em',
          }}
        >
          {t('failure.brokenRecording.title')}
        </h2>
        <p className="muted" style={{ fontFamily: 'var(--font-serif)', fontSize: 14, marginBottom: 18 }}>
          {t('failure.brokenRecording.body')}
        </p>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 14 }}>
          <button type="button" className="btn btn--quiet btn--sm">
            {t('failure.brokenRecording.saveWav')}
          </button>
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={onRetry}
            disabled={reprocessing}
          >
            {reprocessing ? t('callDetail.retrying') : t('failure.brokenRecording.retryCloud')}
          </button>
          <button type="button" className="btn btn--quiet btn--sm" style={{ color: 'var(--signal)' }}>
            {t('failure.brokenRecording.delete')}
          </button>
        </div>
        {call.failed_reason && (
          <details style={{ fontSize: 12 }}>
            <summary className="muted" style={{ cursor: 'pointer', marginBottom: 4 }}>
              {t('failure.brokenRecording.techLabel')}
            </summary>
            <code
              style={{
                display: 'block',
                padding: '8px 10px',
                background: 'var(--bg-2)',
                borderRadius: 'var(--radius-sm)',
                fontFamily: 'var(--font-mono)',
                fontSize: 11,
                wordBreak: 'break-all',
              }}
            >
              {call.failed_reason}
            </code>
          </details>
        )}
      </div>
    );
  }

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
