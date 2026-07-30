import type { Call } from '../../api/recording';
import { CallStateTag } from '../call-state';
import { useI18n } from '../../i18n';
import { humanError } from '../../api/errors';
import { ErrorDiagnostics } from './ErrorDiagnostics';
import { mapFailureToUxKind } from '../../utils/failureKind';
import { useReadiness } from '../readiness/ReadinessProvider';

interface ErrorScreenProps {
  call: Call;
  reprocessing: boolean;
  onRetry: () => void;
  /** [P16.1] Gate — если есть failed chunks, reprocess гарантированно
   *  упадёт через P13 `ensure_all_chunks_done`. Disable retry + tooltip
   *  чтобы user сначала повторил chunks через ChunkFailureAccordion. */
  hasFailedChunks?: boolean;
}

export function ErrorScreen({
  call,
  reprocessing,
  onRetry,
  hasFailedChunks = false,
}: ErrorScreenProps) {
  const { t } = useI18n();
  const readiness = useReadiness();
  const kind = mapFailureToUxKind(call.failed_reason ?? null);

  // Звонок не сломан — ждёт софта. Кнопка «Переобработать» здесь была бы
  // ловушкой: она упала бы на том же гейте. Предлагаем то, что реально
  // помогает, и обещаем автоматику — после докачки звонок поднимется сам.
  if (kind === 'parked') {
    const busy = readiness.downloading || !!readiness.aggregate?.doneBytes;
    return (
      <div className="card" style={{ marginBottom: 18 }}>
        <CallStateTag state="queued" />
        <h2
          style={{
            fontFamily: 'var(--font)',
            fontSize: 22,
            margin: '12px 0 4px',
            letterSpacing: '-0.01em',
          }}
        >
          {t('readiness.eyebrow')}
        </h2>
        <p style={{ fontFamily: 'var(--font)', fontSize: 15, margin: '0 0 16px' }}>
          {t('readiness.callParked')}
        </p>
        <button
          type="button"
          className="btn btn--primary btn--sm"
          onClick={readiness.ensure}
          disabled={busy}
        >
          {busy
            ? t('readiness.downloading', { pct: readiness.aggregate?.pct ?? 0 })
            : t('readiness.callParkedDownload')}
        </button>
        <ErrorDiagnostics call={call} />
      </div>
    );
  }

  if (kind === 'broken_recording') {
    return (
      <div className="card" style={{ marginBottom: 18 }}>
        <p
          style={{
            fontFamily: 'var(--mono)',
            fontSize: 10,
            textTransform: 'uppercase',
            letterSpacing: '0.1em',
            color: 'var(--danger)',
            marginBottom: 8,
          }}
        >
          {t('failure.brokenRecording.eyebrow')}
        </p>
        <h2
          style={{
            fontFamily: 'var(--font)',
            fontSize: 22,
            margin: '0 0 8px',
            letterSpacing: '-0.01em',
          }}
        >
          {t('failure.brokenRecording.title')}
        </h2>
        <p className="muted" style={{ fontFamily: 'var(--font)', fontSize: 14, marginBottom: 18 }}>
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
            disabled={reprocessing || hasFailedChunks}
            title={hasFailedChunks ? t('chunkProgress.resumeBlockedHint') : undefined}
          >
            {reprocessing ? t('callDetail.retrying') : t('failure.brokenRecording.retryCloud')}
          </button>
          <button type="button" className="btn btn--quiet btn--sm" style={{ color: 'var(--danger)' }}>
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
                background: 'var(--sunken)',
                borderRadius: 'var(--r-xs)',
                fontFamily: 'var(--mono)',
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

  // [Bug-fix] humanError маппит raw backend strings (вкл. Cloudflare-wrapped
  // 429/upstream-error JSON) в дружелюбное сообщение. Raw оригинал остаётся
  // в ErrorDiagnostics для траблшутинга.
  const rawReason = call.failed_reason?.trim();
  const reason = rawReason ? humanError(rawReason, t) : t('callDetail.failBadge');
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
          fontFamily: 'var(--font)',
          fontSize: 22,
          margin: '12px 0 4px',
          letterSpacing: '-0.01em',
        }}
      >
        {t('callDetail.errorTitle')}
      </h2>
      <p
        style={{
          fontFamily: 'var(--font)',
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
          fontFamily: 'var(--font)',
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
          disabled={reprocessing || hasFailedChunks}
          title={hasFailedChunks ? t('chunkProgress.resumeBlockedHint') : undefined}
        >
          {reprocessing ? t('callDetail.retrying') : t('callDetail.errorRetry')}
        </button>
        {alternativeProvider && (
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={onRetry}
            disabled={reprocessing || hasFailedChunks}
            title={
              hasFailedChunks
                ? t('chunkProgress.resumeBlockedHint')
                : t('callDetail.errorRetryProvider', { provider: alternativeProvider })
            }
          >
            {t('callDetail.errorRetryProvider', { provider: alternativeProvider })}
          </button>
        )}
      </div>
      {hasFailedChunks && (
        <p
          className="muted"
          style={{ fontSize: 12, fontStyle: 'italic', margin: '10px 0 0' }}
        >
          {t('chunkProgress.resumeBlockedHint')}
        </p>
      )}
      <ErrorDiagnostics call={call} />
    </div>
  );
}
