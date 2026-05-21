// [V6.1] PipelineStrip — collapsible processing card.
//
// Compact strip с CallStateTag + текущим stageLabel + ProgressRail + %.
// Кликабельный <details> → expand показывает full 5-step list. Native
// <details> = free a11y (keyboard + ARIA), без extra JS state.

import { useI18n } from '../../i18n';
import type { CallProgress } from '../../types/callState';
import { PIPELINE_STEP_KEYS } from '../../types/callState';
import { CallStateTag } from './CallStateTag';
import { ProgressRail } from './ProgressRail';

export interface PipelineStripProps {
  progress: CallProgress;
  /** Раскрыто ли по умолчанию (для тестов / debug). */
  defaultOpen?: boolean;
}

export function PipelineStrip({ progress, defaultOpen = false }: PipelineStripProps) {
  const { t } = useI18n();
  const totalSteps = PIPELINE_STEP_KEYS.length;
  return (
    <details className="proc-strip" open={defaultOpen}>
      <summary className="proc-strip-summary">
        <CallStateTag
          state="processing"
          detail={`${progress.step} / ${totalSteps}`}
        />
        {/* [V6.9] Stage label с ellipsis + tooltip — длинные строки
            («Разделили дорожки микрофона и системы») не должны ломать grid. */}
        <span
          className="proc-strip-label"
          title={progress.stageLabel}
        >
          <span className="proc-strip-label-text">{progress.stageLabel}</span>
          <span className="caret" aria-hidden="true" />
          {progress.etaSec !== undefined && (
            <span
              className="mono muted"
              style={{ marginLeft: 8, fontSize: 11 }}
            >
              ~{progress.etaSec} {t('callState.etaSec')}
            </span>
          )}
        </span>
        {/* [V6.9] Pipeline progress = пер-шаговый, но реального within-step %
            нет (партнёры STT/LLM не стримят progress). Поэтому summary rail
            всегда indeterminate (shimmer как у браузеров), без числового %.
            Step count "{step}/{total}" уже в CallStateTag даёт macro-progress. */}
        <div className="proc-strip-rail">
          <ProgressRail
            indeterminate
            ariaLabel={t('callState.processing')}
          />
        </div>
        <span className="btn btn--quiet proc-strip-toggle">
          {t('callState.details')}
        </span>
      </summary>

      <div className="proc-strip-body">
        <div className="steps">
          {PIPELINE_STEP_KEYS.map((key, i) => {
            const stepNum = (i + 1) as 1 | 2 | 3 | 4 | 5;
            const state =
              stepNum < progress.step
                ? 'done'
                : stepNum === progress.step
                  ? 'active'
                  : 'pending';
            const label = t(key);
            return (
              <div key={key} className={`step step--${state}`}>
                <div className="step-bullet" aria-hidden="true">
                  {state === 'done' ? '✓' : stepNum}
                </div>
                {/* [V6.9] Label truncate-able + tooltip — никаких переносов
                    которые ломают вертикальную сетку шагов. */}
                <div className="step-label" title={label}>
                  <span className="step-label-text">{label}</span>
                  {state === 'active' && <span className="caret" aria-hidden="true" />}
                </div>
                {/* [V6.9] Active step → shimmer dot (fake loader). Реального
                    within-step % нет — числовой процент путал юзера. */}
                <div className="step-meta">
                  {state === 'done' && '✓'}
                  {state === 'active' && (
                    <span
                      className="step-shimmer"
                      aria-label={t('callState.processing')}
                    />
                  )}
                  {state === 'pending' && t('callState.pending')}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </details>
  );
}
