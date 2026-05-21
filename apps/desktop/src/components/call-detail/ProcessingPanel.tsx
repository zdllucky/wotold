// [V6.4 / V9] ProcessingPanel — PipelineStrip + reassurance строчка.
//
// [V9] Изменения по user feedback:
// - Ghost-rows удалены (визуальный шум — табы CallDetailPage уже показывают
//   skeleton'ы пока транскрипт грузится, дублировать не надо)
// - PipelineStrip collapsed by default — юзер сам разворачивает «подробнее»
//   если хочет видеть шаги. Сама компактная полоска с прогрессом — достаточно.
// - ProgressRail внутри strip переходит на real macro-progress
//   ((step-1 + pct/100) / 5 → 0-100%) вместо indeterminate shimmer.

import type { Call } from '../../api/recording';
import { PipelineStrip } from '../call-state';
import { PIPELINE_STEP_KEYS, type CallProgress } from '../../types/callState';
import { useI18n } from '../../i18n';

interface ProcessingPanelProps {
  call: Call;
}

export function ProcessingPanel({ call }: ProcessingPanelProps) {
  const { t } = useI18n();
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
      <PipelineStrip progress={progress} />
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
