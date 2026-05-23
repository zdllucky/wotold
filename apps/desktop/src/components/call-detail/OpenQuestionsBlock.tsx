// [M14 T-11] OpenQuestionsBlock — нерешённые вопросы поднятые в звонке.
// Symmetric с DecisionsBlock но с раzed_by chip + question-mark icon.

import { useI18n } from '../../i18n';
import type { OpenQuestion } from '../../api/calls';
import { EvidenceTooltip } from './EvidenceTooltip';

export interface OpenQuestionsBlockProps {
  openQuestions: OpenQuestion[];
  onJumpToTranscript?: (ms: number) => void;
}

export function OpenQuestionsBlock({
  openQuestions,
  onJumpToTranscript,
}: OpenQuestionsBlockProps) {
  const { t } = useI18n();
  if (openQuestions.length === 0) {
    return null;
  }
  return (
    <section className="v2-block open-questions-block">
      <h3 className="v2-block-title">{t('openQuestionsBlock.title')}</h3>
      <ul className="v2-block-list">
        {openQuestions.map((q) => (
          <li key={q.id} className="open-question-row">
            <span className="dot dot--warning" aria-hidden="true">
              ?
            </span>
            <span className="open-question-text">
              {q.text}
              {q.raised_by && (
                <span className="open-question-raised-by">
                  {' '}
                  <span className="mono muted" style={{ fontSize: 11 }}>
                    ({t('openQuestionsBlock.raisedBy')} {q.raised_by})
                  </span>
                </span>
              )}
            </span>
            <span className="open-question-meta">
              {q.evidence_quote && (
                <EvidenceTooltip
                  quote={q.evidence_quote}
                  speaker={q.evidence_speaker}
                  startMs={q.evidence_start_ms}
                  onJumpToTranscript={onJumpToTranscript}
                >
                  💬
                </EvidenceTooltip>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
