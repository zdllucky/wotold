// [V6.4 / V9 / P11.2] ProcessingPanel — единый 5-step PipelineStrip +
// reassurance строчка + conditional ChunkFailureAccordion при failed chunks.
//
// [P11.2 — unified UX] Раньше был switch между ChunkProgressStrip vs
// PipelineStrip по `chunks.length > 0`. У user'а получалось два разных
// mental model на одну фичу: пятишаговый pipeline или сегментная полоска.
//
// Реальность: chunks = implementation detail STT-параллелизации, замаскированный
// step 2 «Распознаём речь». Pipeline один и тот же из 5 шагов независимо
// от длины записи.
//
// Новая модель:
// 1. Всегда рендерим `PipelineStrip` с 5 stages.
// 2. Если step=2 и chunks непустой — PipelineStrip показывает inline-badge
//    «N из M сегментов» (без раскрытия списка).
// 3. Если есть failed chunks (status='failed') — снизу появляется
//    `ChunkFailureAccordion`, collapsed by default. User раскрывает и
//    жмёт retry на нужном сегменте.
// 4. Когда все chunks done → accordion исчезает, P11.1 backend auto-spawns
//    next pipeline stages (diarize → recap).

import type { Call, CallChunk } from '../../api/recording';
import { ChunkFailureAccordion, PipelineStrip } from '../call-state';
import {
  PIPELINE_STEP_KEYS,
  pipelineStepKey,
  type CallProgress,
} from '../../types/callState';
import { useI18n } from '../../i18n';

interface ProcessingPanelProps {
  call: Call;
  /** Chunks для chunked-pipeline записей. Используется в inline-badge
   *  PipelineStrip'а на step 2 и в ChunkFailureAccordion при failed. */
  chunks?: CallChunk[];
  /** Callback retry failed chunk. Прокидывается в ChunkFailureAccordion. */
  onRetryChunk?: (chunkIdx: number) => void;
}

export function ProcessingPanel({ call, chunks, onRetryChunk }: ProcessingPanelProps) {
  const { t } = useI18n();

  // Step может быть NULL до первого emit_progress — показываем step=1 (upload).
  const step = (Math.min(
    Math.max(call.pipeline_step ?? 1, 1),
    PIPELINE_STEP_KEYS.length,
  ) as CallProgress['step']);
  const pct = Math.max(0, Math.min(100, call.pipeline_pct ?? 0));
  const eta = call.pipeline_eta_sec ?? undefined;
  // Единый источник лейбла шага (тот же helper, что CallsPage тег) — список и
  // деталь не могут показать разный шаг для одного звонка.
  const progress: CallProgress = {
    step,
    pct,
    stageLabel: t(pipelineStepKey(call.pipeline_step)),
    etaSec: eta,
  };
  const hasFailedChunks = (chunks ?? []).some((c) => c.status === 'failed');
  return (
    <div style={{ marginBottom: 18 }}>
      <PipelineStrip progress={progress} chunks={chunks} />
      {hasFailedChunks && onRetryChunk && chunks && (
        <ChunkFailureAccordion chunks={chunks} onRetryChunk={onRetryChunk} />
      )}
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font)',
          fontSize: 14,
          fontStyle: 'italic',
          marginTop: 14,
          marginBottom: 0,
        }}
      >
        {t('callDetail.reassureCanClose')}
      </p>
    </div>
  );
}
