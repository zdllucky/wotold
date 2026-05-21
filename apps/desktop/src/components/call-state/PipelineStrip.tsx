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
        <span className="proc-strip-label">
          {progress.stageLabel}
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
        <div className="proc-strip-rail">
          <ProgressRail
            pct={progress.pct}
            ariaLabel={t('callState.processing')}
          />
          <span className="mono proc-strip-pct">{Math.round(progress.pct)}%</span>
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
            return (
              <div key={key} className={`step step--${state}`}>
                <div className="step-bullet" aria-hidden="true">
                  {state === 'done' ? '✓' : stepNum}
                </div>
                <div className="step-label">
                  {t(key)}
                  {state === 'active' && <span className="caret" aria-hidden="true" />}
                </div>
                <div className="step-meta">
                  {state === 'done' && '✓'}
                  {state === 'active' && `${Math.round(progress.pct)}%`}
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
