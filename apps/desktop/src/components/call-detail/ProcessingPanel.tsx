// [V6.4 / V9] ProcessingPanel — PipelineStrip + reassurance строчка.
//
// [V9] Изменения по user feedback:
// - Ghost-rows удалены (визуальный шум — табы CallDetailPage уже показывают
//   skeleton'ы пока транскрипт грузится, дублировать не надо)
// - PipelineStrip collapsed by default — юзер сам разворачивает «подробнее»
//   если хочет видеть шаги. Сама компактная полоска с прогрессом — достаточно.
// - ProgressRail внутри strip переходит на real macro-progress
//   ((step-1 + pct/100) / 5 → 0-100%) вместо indeterminate shimmer.
//
// [M13.3.1] Когда `chunks.length > 0` (chunked-pipeline запись) — рендерим
// `ChunkProgressStrip` с per-segment progress. Иначе fallback на классический
// 5-step PipelineStrip (cloud-managed, legacy local, или local-engine без
// chunked флага).

import type { Call, CallChunk } from '../../api/recording';
import { ChunkProgressStrip, PipelineStrip } from '../call-state';
import { PIPELINE_STEP_KEYS, type CallProgress } from '../../types/callState';
import { useI18n } from '../../i18n';

interface ProcessingPanelProps {
  call: Call;
  /** [M13.3.1] Список chunks для chunked-pipeline записей. Non-empty —
   *  показываем ChunkProgressStrip; пусто — fallback на PipelineStrip. */
  chunks?: CallChunk[];
}

export function ProcessingPanel({ call, chunks }: ProcessingPanelProps) {
  const { t } = useI18n();
  const useChunkStrip = chunks !== undefined && chunks.length > 0;

  // Step может быть NULL до первого emit_progress — показываем step=1 (upload).
  const step = (Math.min(
    Math.max(call.pipeline_step ?? 1, 1),
    PIPELINE_STEP_KEYS.length,
  ) as CallProgress['step']);
  const pct = Math.max(0, Math.min(100, call.pipeline_pct ?? 0));
  const eta = call.pipeline_eta_sec ?? undefined;
  const stageKey =
    PIPELINE_STEP_KEYS[step - 1] ?? PIPELINE_STEP_KEYS[0];
  const progress: CallProgress = {
    step,
    pct,
    stageLabel: t(stageKey),
    etaSec: eta,
  };
  return (
    <div style={{ marginBottom: 18 }}>
      {useChunkStrip ? (
        <ChunkProgressStrip chunks={chunks} />
      ) : (
        <PipelineStrip progress={progress} />
      )}
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
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
