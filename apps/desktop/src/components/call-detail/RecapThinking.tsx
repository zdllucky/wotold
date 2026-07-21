// [F3] RecapThinking — thinking-блок генерации рекапа (стиль «размышлений»):
// живой список шагов chain'а (classify → refine i/N → post_pass → narrative
// или единый generate для cloud/короткого local). Каждый done-шаг с превью
// разворачивается в промежуточный результат (title + первые key_points).
//
// Жизненный цикл: рендерится ТОЛЬКО пока идёт генерация — useCallDetail
// очищает steps на pipeline:finished/cancelled и при смене звонка, блок
// исчезает насовсем (решение F3: не персистим «размышления»).
//
// Реюз DS: native <details> (a11y бесплатно, mirror .proc-strip) +
// .steps/.step--done|active|failed/.step-bullet/.step-shimmer/.caret.

import { useI18n } from '../../i18n';
import { Icon } from '../../ui/Icon';
import type { RecapStepEvent } from '../../api/recording';

interface RecapThinkingProps {
  steps: RecapStepEvent[];
}

type StepUiState = 'done' | 'active' | 'failed';

function uiState(s: RecapStepEvent): StepUiState {
  if (s.status === 'done') return 'done';
  if (s.status === 'failed') return 'failed';
  return 'active';
}

export function RecapThinking({ steps }: RecapThinkingProps) {
  const { t } = useI18n();
  if (steps.length === 0) return null;

  // total_steps=0 = «ещё неизвестно» — берём максимум из увиденных событий.
  const total = steps.reduce((m, s) => Math.max(m, s.total_steps), 0);
  const doneCount = steps.filter((s) => s.status === 'done').length;

  const labelFor = (s: RecapStepEvent): string => {
    switch (s.kind) {
      case 'classify':
        return t('callDetail.think.classify');
      case 'refine':
        return t('callDetail.think.refine', {
          no: s.chunk_no ?? s.step_idx,
          total: s.chunk_total ?? '…',
        });
      case 'post_pass':
        return t('callDetail.think.postPass');
      case 'narrative':
        return t('callDetail.think.narrative');
      case 'finalize':
        return t('callDetail.think.finalize');
      case 'generate':
        return t('callDetail.think.generate');
    }
  };

  return (
    <details className="recap-think" open>
      <summary className="recap-think-summary">
        {/* Icon без title сам ставит aria-hidden. */}
        <Icon name="chevronRight" size={12} className="recap-think-chevron" />
        <span className="recap-think-title">
          {t('callDetail.think.title')}
          <span className="caret" aria-hidden="true" />
        </span>
        <span className="mono muted recap-think-count" aria-hidden="true">
          ·
        </span>
        <span className="mono muted recap-think-count">
          {doneCount} / {total > 0 ? total : '…'}
        </span>
      </summary>
      <div className="steps recap-think-steps" aria-live="polite">
        {steps.map((s) => {
          const state = uiState(s);
          const label = labelFor(s);
          return (
            <div key={s.step_idx}>
              <div className={`step step--${state}`}>
                <div className="step-bullet" aria-hidden="true">
                  {state === 'done' ? '✓' : state === 'failed' ? '!' : s.step_idx + 1}
                </div>
                <div className="step-label" title={label}>
                  <span className="step-label-text">{label}</span>
                  {state === 'active' && <span className="caret" aria-hidden="true" />}
                </div>
                <div className="step-meta">
                  {state === 'done' && '✓'}
                  {state === 'active' && (
                    <span
                      className="step-shimmer"
                      aria-label={t('callDetail.think.inProgress')}
                    />
                  )}
                  {state === 'failed' && t('callDetail.think.stepFailed')}
                </div>
              </div>
              {s.preview && (
                <details className="recap-think-preview">
                  <summary>{t('callDetail.think.preview')}</summary>
                  <div className="recap-think-preview-body">
                    <strong>{s.preview.title}</strong>
                    {s.preview.key_points.length > 0 && (
                      <ul>
                        {s.preview.key_points.map((kp, i) => (
                          <li key={i}>{kp}</li>
                        ))}
                      </ul>
                    )}
                  </div>
                </details>
              )}
            </div>
          );
        })}
      </div>
    </details>
  );
}
