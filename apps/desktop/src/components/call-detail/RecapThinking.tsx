// [F3, B20.1] RecapThinking — thinking-блок генерации рекапа в стиле
// «рассуждений» Claude Code: поток приглушённых строк-шагов у левой
// направляющей, без нумерованных кружков и галочек. Активный шаг — text-shimmer,
// done-шаг с превью показывает промежуточный результат тихим инлайн-текстом
// (без вложенного <details>: нет превью — нет и пустого аффорданса).
//
// Жизненный цикл: рендерится ТОЛЬКО пока идёт генерация — useCallDetail
// очищает steps на pipeline:finished/cancelled и при смене звонка, блок
// исчезает насовсем (решение F3: не персистим «размышления»).

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
      <div className="rthink-stream" aria-live="polite">
        {steps.map((s) => {
          const state = uiState(s);
          const label = labelFor(s);
          return (
            <div key={s.step_idx} className={`rthink-line rthink-line--${state}`}>
              <div
                className="rthink-label"
                title={label}
                aria-label={state === 'active' ? `${label} — ${t('callDetail.think.inProgress')}` : undefined}
              >
                <span className="rthink-label-text">{label}</span>
                {state === 'failed' && (
                  <span className="rthink-skip mono">{t('callDetail.think.stepFailed')}</span>
                )}
              </div>
              {state === 'done' && s.preview && (
                <div className="rthink-preview">
                  <span className="rthink-preview-title">{s.preview.title}</span>
                  {s.preview.key_points.map((kp, i) => (
                    <span key={i}>— {kp}</span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </details>
  );
}
