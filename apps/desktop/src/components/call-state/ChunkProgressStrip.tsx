// [M13.3.1] ChunkProgressStrip — N-chunk progress strip для chunked-pipeline
// записей (M13 Phase 1+2). Параллель к V6.4 PipelineStrip (5 macro-шагов),
// но семантика разная: PipelineStrip = upload → STT → speakers → merge →
// recap (atomic per-call), ChunkProgressStrip = N×10мин сегментов где каждый
// идёт через тот же 5-step pipeline параллельно.
//
// Backend источник: `list_call_chunks(call_id)` initial load +
// `transcript:chunk_done` event для delta-updates (см. useCallDetail).
//
// Empty state (нет chunks rows в DB — cloud-managed или legacy local) —
// silent null render. Caller (ProcessingPanel) fallback'ит на классический
// PipelineStrip.
//
// [Tech-debt P0.2] Failed chunks получают retry-кнопку. На click invoke
// retry_chunk Tauri command + локальный optimistic state Set<chunk_idx>
// чтобы button показывал "Повторяем…" disabled пока ждём transcript:chunk_done.

import { useEffect, useState } from 'react';

import { useI18n } from '../../i18n';
import type { CallChunk } from '../../api/recording';
import { CallStateTag } from './CallStateTag';
import { ProgressRail } from './ProgressRail';

export interface ChunkProgressStripProps {
  chunks: CallChunk[];
  /** Раскрыто ли по умолчанию (тесты / debug). */
  defaultOpen?: boolean;
  /** [Tech-debt P0.2] Callback на retry failed chunk. Если undefined —
   *  кнопка не рендерится (read-only mode для legacy mounts). */
  onRetryChunk?: (chunkIdx: number) => void;
}

/** mm:ss из ms. Сохраняет negative-safe → 0:00. */
function formatRange(start: number, end: number | null): string {
  const fmt = (ms: number): string => {
    const sec = Math.max(0, Math.floor(ms / 1000));
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  };
  const startStr = fmt(start);
  const endStr = end !== null ? fmt(end) : '…';
  return `${startStr}—${endStr}`;
}

interface BulletState {
  klass: 'done' | 'active' | 'pending' | 'failed';
  symbol: string;
  ariaLabelKey: 'statusDone' | 'statusFailed' | 'statusProcessing' | 'statusPending';
}

function chunkBullet(status: CallChunk['status']): BulletState {
  switch (status) {
    case 'done':
      return { klass: 'done', symbol: '✓', ariaLabelKey: 'statusDone' };
    case 'failed':
      return { klass: 'failed', symbol: '!', ariaLabelKey: 'statusFailed' };
    case 'processing':
      return { klass: 'active', symbol: '·', ariaLabelKey: 'statusProcessing' };
    case 'pending':
    default:
      return { klass: 'pending', symbol: '◦', ariaLabelKey: 'statusPending' };
  }
}

export function ChunkProgressStrip({
  chunks,
  defaultOpen = false,
  onRetryChunk,
}: ChunkProgressStripProps) {
  const { t } = useI18n();
  // [Tech-debt P0.2] Optimistic retry state — chunk idx → "ждём что event
  // отметит status не-failed". Очищаем когда chunk выходит из failed (т.е.
  // pending/processing/done пришли через chunks prop).
  const [retrying, setRetrying] = useState<Set<number>>(() => new Set());
  useEffect(() => {
    setRetrying((prev) => {
      const next = new Set(prev);
      let changed = false;
      for (const idx of prev) {
        const chunk = chunks.find((c) => c.chunk_idx === idx);
        if (!chunk || chunk.status !== 'failed') {
          next.delete(idx);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [chunks]);

  if (chunks.length === 0) {
    // Caller (ProcessingPanel) разруливает fallback на классический PipelineStrip.
    return null;
  }
  const total = chunks.length;
  const done = chunks.filter((c) => c.status === 'done').length;
  const failed = chunks.filter((c) => c.status === 'failed').length;
  const macroPct = total > 0 ? Math.round((done / total) * 100) : 0;
  // Detail badge: «3 / 8» или «3 / 8 · 1 ✗» если есть failed.
  const detail = failed > 0 ? `${done} / ${total} · ${failed} ✗` : `${done} / ${total}`;
  // Label — i18n с подстановками {done}/{total}.
  const label = t('chunkProgress.ofN')
    .replace('{done}', String(done))
    .replace('{total}', String(total));

  return (
    <details className="proc-strip" open={defaultOpen}>
      <summary className="proc-strip-summary">
        <CallStateTag state="processing" detail={detail} />
        <span className="proc-strip-label" title={label}>
          <span className="proc-strip-label-text">{t('chunkProgress.label')}</span>
          <span className="caret" aria-hidden="true" />
          <span className="mono muted" style={{ marginLeft: 8, fontSize: 11 }}>
            {label}
          </span>
        </span>
        <div className="proc-strip-rail">
          <ProgressRail
            pct={macroPct}
            ariaLabel={`${t('callState.processing')} · ${macroPct}%`}
          />
          <span className="mono proc-strip-pct">{macroPct}%</span>
        </div>
        <span className="btn btn--quiet proc-strip-toggle">{t('callState.details')}</span>
      </summary>

      <div className="proc-strip-body">
        {/* [Tech-debt P0.2] Failure summary banner — выше списка сегментов. */}
        {failed > 0 && (
          <p
            className="muted"
            style={{
              fontSize: 12,
              fontFamily: 'var(--font-sans)',
              margin: '0 0 10px',
            }}
          >
            {t('chunkProgress.failedSummary')
              .replace('{n}', String(failed))
              .replace('{total}', String(total))}
          </p>
        )}
        <div className="steps">
          {chunks.map((chunk) => {
            const bullet = chunkBullet(chunk.status);
            const range = formatRange(chunk.start_ms, chunk.end_ms);
            const ariaStatus = t(`chunkProgress.${bullet.ariaLabelKey}`);
            const isRetrying = retrying.has(chunk.chunk_idx);
            return (
              <div key={chunk.chunk_idx} className={`step step--${bullet.klass}`}>
                <div className="step-bullet" aria-label={ariaStatus}>
                  {bullet.symbol}
                </div>
                <div className="step-label" title={range}>
                  <span className="step-label-text mono">{range}</span>
                  {bullet.klass === 'active' && <span className="caret" aria-hidden="true" />}
                </div>
                <div className="step-meta">
                  {bullet.klass === 'done' && '✓'}
                  {bullet.klass === 'active' && (
                    <span className="step-shimmer" aria-label={ariaStatus} />
                  )}
                  {bullet.klass === 'failed' && (
                    <>
                      <span style={{ marginRight: 8 }}>{ariaStatus}</span>
                      {onRetryChunk && (
                        <button
                          type="button"
                          className="btn btn--quiet"
                          style={{
                            fontSize: 11,
                            padding: '2px 8px',
                            fontFamily: 'var(--font-sans)',
                          }}
                          disabled={isRetrying}
                          onClick={() => {
                            setRetrying((s) => {
                              const next = new Set(s);
                              next.add(chunk.chunk_idx);
                              return next;
                            });
                            onRetryChunk(chunk.chunk_idx);
                          }}
                        >
                          {isRetrying
                            ? t('chunkProgress.retrying')
                            : t('chunkProgress.retry')}
                        </button>
                      )}
                    </>
                  )}
                  {bullet.klass === 'pending' && ariaStatus}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </details>
  );
}
