// [V6.4] ProcessingPanel — рендерит PipelineStrip + reassurance card +
// ghost-rows mockup транскрипта. Юзер видит «что-то происходит» вместо
// пустого экрана. DB-state восстанавливается на reload, событие
// `call:progress` обновляет live tick без F5.

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
      <PipelineStrip progress={progress} defaultOpen />
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
      <div className="transcript" style={{ marginTop: 18 }}>
        {/* Ghost-rows — намёк что транскрипт скоро появится. Без дёрганий
            при загрузке (skeletons mounted один раз, до получения transcript). */}
        {[0, 1, 2].map((i) => (
          <div key={i} className="transcript-row transcript-row--ghost">
            <div className="transcript-speaker" aria-hidden="true">···</div>
            <div className="transcript-text" aria-hidden="true">···</div>
            <div className="transcript-time" aria-hidden="true">···</div>
          </div>
        ))}
      </div>
    </div>
  );
}
