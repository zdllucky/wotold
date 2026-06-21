// Markdown panel wrapper for recap/transcript tabs in CallDetailPage.
// Renders ReactMarkdown when content present, otherwise Empty placeholder
// либо explicit empty-state с CTA (P14.2) если caller передал handlers.

import ReactMarkdown from 'react-markdown';
import { useI18n } from '../../i18n';
import { useTypewriter } from '../../hooks/useTypewriter';
import { Empty } from '../../ui';
import { isMarkdownBlank } from '../../utils/markdown';

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
  /** [P-fix10] Typewriter-reveal: «печатать» текст по буквам (эффект «ИИ
   *  печатает»). Включается one-shot когда recap только что сгенерирован.
   *  reduced-motion → мгновенно. */
  animate?: boolean;
  /** [P-fix10] Идёт генерация (pipeline/regen) и текста ещё нет → живой
   *  «генерируется…» блок (caret) вместо пустоты/CTA, чтобы не казалось
   *  зависшим. */
  generating?: boolean;
  /** Подпись живого блока (напр. «Генерируется саммари… 30s»). */
  generatingLabel?: string;
}

export function MdPanel({
  md,
  emptyHint,
  onRegenerate,
  regenerating = false,
  emptyBody,
  regenerateDisabled = false,
  animate = false,
  generating = false,
  generatingLabel,
}: MdPanelProps) {
  const { t } = useI18n();
  const blank = isMarkdownBlank(md);
  // Hook вызывается безусловно (rules-of-hooks) — enabled только для непустого
  // md + animate. При выключенном — shown=md целиком, done=true.
  const { shown, done } = useTypewriter(md ?? '', animate && !blank);
  // [Fix] Семантически-пустой рекап (`"# Рекап\n\n"` от старых до-фиксных
  // звонков) — строка непустая, но тела нет. Раньше рендерился голый заголовок
  // без CTA → юзер видел пустоту и не мог пересоздать. Теперь трактуем как пусто.
  if (blank) {
    // [P-fix10] Идёт генерация — живой «генерируется…» блок с мигающей кареткой
    // (aria-live) вместо пустого/CTA, чтобы юзер видел что софт работает.
    if (generating) {
      return (
        <div
          className="card"
          style={{
            padding: 'var(--space-6, 24px)',
            textAlign: 'center',
            margin: 'var(--space-4, 16px) 0',
          }}
          aria-busy="true"
          aria-live="polite"
        >
          <span
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontSize: 17,
              color: 'var(--muted)',
            }}
          >
            {generatingLabel ?? emptyHint}
          </span>
          <span className="caret" aria-hidden="true" />
        </div>
      );
    }
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
      <ReactMarkdown>{shown}</ReactMarkdown>
      {!done && <span className="caret" aria-hidden="true" />}
    </div>
  );
}
