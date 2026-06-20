// [P11.2] ChunkFailureAccordion — collapsed-by-default accordion который
// появляется **только при наличии failed chunks**. Заменяет старый
// ChunkProgressStrip как top-level visible component, потому что в P11.2
// chunks стали implementation detail step 2 PipelineStrip'а — успешные
// chunks не должны загромождать UI.
//
// User UX model:
//   - Pipeline идёт нормально → user видит только 5-step PipelineStrip,
//     accordion отсутствует.
//   - Любой chunk failed → accordion появляется под PipelineStrip с заголовком
//     «Не удалось распознать сегменты», collapsed. User раскрывает чтобы
//     ретрайнуть конкретный сегмент.
//   - После retry → status failed→pending→processing→done. Когда все done →
//     accordion скрывается (parent ProcessingPanel перестаёт его рендерить,
//     потому что `failedChunks.length === 0`).
//
// A11y: native `<details>` даёт keyboard + ARIA expanded/collapsed бесплатно.
// Failure counter `aria-live="polite"` оповещает SR при изменении.

import { useEffect, useState } from 'react';

import { useI18n } from '../../i18n';
import type { CallChunk } from '../../api/recording';

export interface ChunkFailureAccordionProps {
  chunks: CallChunk[];
  /** Callback на retry failed chunk. */
  onRetryChunk: (chunkIdx: number) => void;
  /** Опциональный override для тестов: раскрыть accordion по умолчанию. */
  defaultOpen?: boolean;
}

/** mm:ss из ms. Negative-safe → 0:00. */
function formatRange(startMs: number, endMs: number | null): string {
  const fmt = (ms: number): string => {
    const sec = Math.max(0, Math.floor(ms / 1000));
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  };
  return `${fmt(startMs)}—${endMs !== null ? fmt(endMs) : '…'}`;
}

export function ChunkFailureAccordion({
  chunks,
  onRetryChunk,
  defaultOpen = false,
}: ChunkFailureAccordionProps) {
  const { t } = useI18n();
  const [retrying, setRetrying] = useState<Set<number>>(() => new Set());

  // Очищаем optimistic state когда chunk вышел из failed (pending/processing/done пришли).
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

  const failedChunks = chunks.filter((c) => c.status === 'failed');
  if (failedChunks.length === 0) {
    return null;
  }
  const total = chunks.length;
  const summary = t('chunkProgress.failedSummary')
    .replace('{n}', String(failedChunks.length))
    .replace('{total}', String(total));

  return (
    <details
      className="chunk-fail-accordion"
      open={defaultOpen}
      style={{
        marginTop: 14,
        background: 'var(--bg-2)',
        border: '1px solid var(--line)',
        borderRadius: 'var(--radius-md)',
        padding: '10px 14px',
      }}
    >
      <summary
        className="chunk-fail-accordion-summary"
        style={{
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          listStyle: 'none',
          fontFamily: 'var(--font-sans)',
          fontSize: 13,
          color: 'var(--signal)',
        }}
      >
        <span aria-hidden="true" style={{ fontSize: 14 }}>
          ⚠
        </span>
        <span
          aria-live="polite"
          style={{ flex: 1, fontWeight: 500 }}
        >
          {t('chunkProgress.accordionTitle')} · {failedChunks.length} / {total}
        </span>
        <span
          className="muted"
          style={{ fontSize: 11, fontStyle: 'italic' }}
        >
          {t('chunkProgress.accordionHint')}
        </span>
      </summary>

      <p
        className="muted"
        style={{
          fontSize: 12,
          fontFamily: 'var(--font-sans)',
          margin: '10px 0 8px',
        }}
      >
        {summary}
      </p>
      <ul
        style={{
          listStyle: 'none',
          padding: 0,
          margin: 0,
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        {failedChunks.map((chunk) => {
          const range = formatRange(chunk.start_ms, chunk.end_ms);
          const isRetrying = retrying.has(chunk.chunk_idx);
          return (
            <li
              key={chunk.chunk_idx}
              className="chunk-fail-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 12,
                padding: '6px 10px',
                background: 'var(--paper)',
                border: '1px solid var(--line)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              <span
                aria-label={t('chunkProgress.statusFailed')}
                style={{ color: 'var(--signal)', fontSize: 14 }}
              >
                !
              </span>
              <span
                className="mono"
                style={{ flex: 1, fontSize: 13 }}
                title={range}
              >
                {range}
              </span>
              <button
                type="button"
                className="btn btn--quiet"
                style={{
                  fontSize: 11,
                  padding: '4px 10px',
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
                {isRetrying ? t('chunkProgress.retrying') : t('chunkProgress.retry')}
              </button>
            </li>
          );
        })}
      </ul>
    </details>
  );
}
