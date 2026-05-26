// Markdown panel wrapper for recap/transcript tabs in CallDetailPage.
// Renders ReactMarkdown when content present, otherwise Empty placeholder
// либо explicit empty-state с CTA (P14.2) если caller передал handlers.

import ReactMarkdown from 'react-markdown';
import { useI18n } from '../../i18n';
import { Empty } from '../../ui';

interface MdPanelProps {
  md: string | null;
  emptyHint: string;
  /** [P14.2] Explicit empty-state с CTA «Создать саммари». Если undefined —
   *  fallback на silent `<Empty>`. Когда передан — caller также должен
   *  передать `regenerating` для disabled state. */
  onRegenerate?: () => void;
  regenerating?: boolean;
  /** Дополнительный body — обычно `humanError(call.recap_failed_reason)`
   *  или hint о processing pipeline. Рендерится под title. */
  emptyBody?: string;
  /** Когда true → CTA disabled (например pipeline ещё работает). */
  regenerateDisabled?: boolean;
}

export function MdPanel({
  md,
  emptyHint,
  onRegenerate,
  regenerating = false,
  emptyBody,
  regenerateDisabled = false,
}: MdPanelProps) {
  const { t } = useI18n();
  if (!md) {
    // [P14.2] Когда caller передал onRegenerate — рендерим actionable card
    // с CTA вместо silent placeholder. Помогает user'у понять что recap
    // ещё не создан и что можно сделать.
    if (onRegenerate) {
      return (
        <div
          className="card"
          style={{
            padding: 'var(--space-6, 24px)',
            textAlign: 'center',
            margin: 'var(--space-4, 16px) 0',
          }}
        >
          <div
            style={{
              fontSize: 'var(--font-size-lg, 16px)',
              fontWeight: 600,
              marginBottom: 'var(--space-2, 8px)',
            }}
          >
            {t('callDetail.recapEmptyTitle')}
          </div>
          <p
            className="muted"
            style={{
              margin: '0 0 var(--space-4, 16px)',
              maxWidth: 480,
              marginLeft: 'auto',
              marginRight: 'auto',
            }}
          >
            {emptyBody ?? emptyHint}
          </p>
          <button
            type="button"
            className="btn btn--primary"
            onClick={onRegenerate}
            disabled={regenerating || regenerateDisabled}
          >
            {regenerating
              ? t('callDetail.regenerating')
              : t('callDetail.recapEmptyAction')}
          </button>
        </div>
      );
    }
    return <Empty description={emptyHint} />;
  }
  return (
    <div className="markdown">
      <ReactMarkdown>{md}</ReactMarkdown>
    </div>
  );
}
