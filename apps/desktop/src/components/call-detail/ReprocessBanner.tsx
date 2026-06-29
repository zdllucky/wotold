// [V8] ReprocessBanner — компактный overlay над уже видимым контентом
// звонка. Юзер видит что reprocess идёт, но **старые** recap/transcript
// остаются в табах под баннером. Cancel кнопка → backend abort'ает
// pipeline task + restore статуса на 'ready'.
//
// Отличие от ProcessingPanel: без ghost-rows (контент уже есть) и с
// Cancel кнопкой (первичная обработка не отменяется до 'ready' — нечего
// восстанавливать).

import type { Call } from '../../api/recording';
import { PipelineStrip } from '../call-state';
import { PIPELINE_STEP_KEYS, type CallProgress } from '../../types/callState';
import { useI18n } from '../../i18n';

interface ReprocessBannerProps {
  call: Call;
  onCancel: () => void;
}

export function ReprocessBanner({ call, onCancel }: ReprocessBannerProps) {
  const { t } = useI18n();
  const step = (Math.min(
    Math.max(call.pipeline_step ?? 1, 1),
    PIPELINE_STEP_KEYS.length,
  ) as CallProgress['step']);
  const stageKey =
    PIPELINE_STEP_KEYS[step - 1] ?? PIPELINE_STEP_KEYS[0];
  const progress: CallProgress = {
    step,
    pct: 0,
    stageLabel: t(stageKey!),
    etaSec: call.pipeline_eta_sec ?? undefined,
  };
  return (
    <div style={{ marginBottom: 18 }}>
      <PipelineStrip progress={progress} />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 12,
          marginTop: 10,
          fontFamily: 'var(--font)',
          fontStyle: 'italic',
          fontSize: 13,
          color: 'var(--text-3)',
        }}
      >
        <span style={{ flex: 1, minWidth: 0 }}>
          {t('callDetail.reprocessRunning')}
        </span>
        <button
          type="button"
          className="btn btn--quiet btn--sm"
          onClick={onCancel}
          data-comment-anchor="reprocess-cancel"
        >
          {t('callDetail.reprocessCancel')}
        </button>
      </div>
    </div>
  );
}
