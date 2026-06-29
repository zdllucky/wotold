// [call-detail] Recap view — Wotold v2 прототип-макет (wk-screens Recap):
// Segmented(Оформленный/Markdown) + «Копировать .md», тело — `.md-rich`
// (рендер recap-markdown в типографику прототипа) либо `.md-raw` (сырой md).
//
// Сводит recap-таб к макету прототипа: структурные блоки (decisions/
// open-questions/tasks/evidence) НЕ рендерятся — recap = markdown-документ.
// Пустые / generating / regenerate состояния сохранены из прежнего MdPanel.

import { useState } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import { useI18n } from '../../i18n';
import { useTypewriter } from '../../hooks/useTypewriter';
import { Button, Empty, Icon, Segmented } from '../../ui';
import { isMarkdownBlank } from '../../utils/markdown';

type Mode = 'rich' | 'md';

interface RecapViewProps {
  recap: string | null;
  /** Typewriter-reveal когда recap только что сгенерирован (reduced-motion → мгновенно). */
  animate?: boolean;
  /** Идёт генерация и текста ещё нет → живой «генерируется…» блок (caret). */
  generating?: boolean;
  generatingLabel?: string;
  emptyHint: string;
  emptyBody?: string;
  onRegenerate?: () => void;
  regenerating?: boolean;
  regenerateDisabled?: boolean;
}

// Рендер markdown в типографику прототипа `.md-rich` (h→.md-h, p→.md-p,
// списки→.md-ul, code→.md-code).
const MD_RICH: Components = {
  h1: ({ children }) => <h3 className="md-h">{children}</h3>,
  h2: ({ children }) => <h3 className="md-h">{children}</h3>,
  h3: ({ children }) => <h3 className="md-h">{children}</h3>,
  h4: ({ children }) => <h3 className="md-h">{children}</h3>,
  p: ({ children }) => <p className="md-p">{children}</p>,
  ul: ({ children }) => <ul className="md-ul">{children}</ul>,
  ol: ({ children }) => <ul className="md-ul">{children}</ul>,
  code: ({ children }) => <code className="md-code">{children}</code>,
};

export function RecapView({
  recap,
  animate = false,
  generating = false,
  generatingLabel,
  emptyHint,
  emptyBody,
  onRegenerate,
  regenerating = false,
  regenerateDisabled = false,
}: RecapViewProps) {
  const { t } = useI18n();
  const [mode, setMode] = useState<Mode>('rich');
  const [copied, setCopied] = useState(false);
  const blank = isMarkdownBlank(recap);
  // Hook безусловно (rules-of-hooks) — enabled только для непустого md + animate.
  const { shown, done } = useTypewriter(recap ?? '', animate && !blank);

  if (blank) {
    if (generating) {
      return (
        <div
          className="card"
          style={{ padding: 'var(--s6)', textAlign: 'center', margin: 'var(--s4) 0' }}
          aria-busy="true"
          aria-live="polite"
        >
          <span
            style={{
              fontFamily: 'var(--font)',
              fontStyle: 'italic',
              fontSize: 'var(--t-16)',
              color: 'var(--text-3)',
            }}
          >
            {generatingLabel ?? emptyHint}
          </span>
          <span className="caret" aria-hidden="true" />
        </div>
      );
    }
    if (onRegenerate) {
      return (
        <div
          className="card"
          style={{ padding: 'var(--s6)', textAlign: 'center', margin: 'var(--s4) 0' }}
        >
          <div style={{ fontSize: 'var(--t-16)', fontWeight: 600, marginBottom: 'var(--s2)' }}>
            {t('callDetail.recapEmptyTitle')}
          </div>
          <p className="muted" style={{ margin: '0 auto var(--s4)', maxWidth: 480 }}>
            {emptyBody ?? emptyHint}
          </p>
          <Button
            variant="primary"
            onClick={onRegenerate}
            disabled={regenerating || regenerateDisabled}
          >
            {regenerating ? t('callDetail.regenerating') : t('callDetail.recapEmptyAction')}
          </Button>
        </div>
      );
    }
    return <Empty description={emptyHint} />;
  }

  const md = recap ?? '';
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(md);
    } catch {
      /* clipboard недоступен — копирование молча no-op */
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  return (
    <div style={{ marginTop: 18 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
        <Segmented<Mode>
          value={mode}
          onChange={setMode}
          options={[
            { value: 'rich', label: t('recap.modeRich'), icon: 'sparkle' },
            { value: 'md', label: t('recap.modeMd'), icon: 'code' },
          ]}
        />
        <div style={{ flex: 1 }} />
        <Button
          variant="default"
          size="sm"
          leading={<Icon name={copied ? 'check' : 'copy'} size={14} />}
          onClick={copy}
        >
          {copied ? t('recap.copied') : t('recap.copyMd')}
        </Button>
      </div>
      {mode === 'md' ? (
        <pre className="md-raw">{md}</pre>
      ) : (
        <div className="md-rich">
          <ReactMarkdown components={MD_RICH}>{shown}</ReactMarkdown>
          {!done && <span className="caret" aria-hidden="true" />}
        </div>
      )}
    </div>
  );
}
