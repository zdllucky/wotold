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

import { useI18n } from '../../i18n';
import type { CallChunk } from '../../api/recording';
import { CallStateTag } from './CallStateTag';
import { ProgressRail } from './ProgressRail';

export interface ChunkProgressStripProps {
  chunks: CallChunk[];
  /** Раскрыто ли по умолчанию (тесты / debug). */
  defaultOpen?: boolean;
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

export function ChunkProgressStrip({ chunks, defaultOpen = false }: ChunkProgressStripProps) {
  const { t } = useI18n();
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
        <div className="steps">
          {chunks.map((chunk) => {
            const bullet = chunkBullet(chunk.status);
            const range = formatRange(chunk.start_ms, chunk.end_ms);
            const ariaStatus = t(`chunkProgress.${bullet.ariaLabelKey}`);
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
                  {bullet.klass === 'failed' && ariaStatus}
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
