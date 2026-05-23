// [M14 T-11] DecisionsBlock — список решений принятых в звонке.
//
// Рендерится в табе «Рекап» (CallDetailPage) над markdown, когда не пуст.
// Каждое решение: ✓ dot + text + (опц.) EvidenceTooltip 💬 + confidence
// badge при низкой уверенности (< 0.7).
// Empty state: рендерим null (caller выводит fallback на markdown narrative).

import { useI18n } from '../../i18n';
import type { Decision } from '../../api/calls';
import { EvidenceTooltip } from './EvidenceTooltip';

export interface DecisionsBlockProps {
  decisions: Decision[];
  onJumpToTranscript?: (ms: number) => void;
}

export function DecisionsBlock({ decisions, onJumpToTranscript }: DecisionsBlockProps) {
  const { t } = useI18n();
  if (decisions.length === 0) {
    return null;
  }
  return (
    <section className="v2-block decisions-block">
      <h3 className="v2-block-title">{t('decisionsBlock.title')}</h3>
      <ul className="v2-block-list">
        {decisions.map((d) => (
          <li key={d.id} className="decision-row">
            <span className="dot dot--success" aria-hidden="true" />
            <span className="decision-text">{d.text}</span>
            <span className="decision-meta">
              {d.evidence_quote && (
                <EvidenceTooltip
                  quote={d.evidence_quote}
                  speaker={d.evidence_speaker}
                  startMs={d.evidence_start_ms}
                  onJumpToTranscript={onJumpToTranscript}
                >
                  💬
                </EvidenceTooltip>
              )}
              {d.confidence !== null && d.confidence < 0.7 && (
                <span
                  className="confidence-low"
                  title={t('evidence.lowConfidence')}
                  aria-label={t('evidence.lowConfidence')}
                >
                  ?
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
