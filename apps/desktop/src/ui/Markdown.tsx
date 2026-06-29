// [recap] Wotold v2 markdown renderer — полное покрытие markdown-элементов в
// типографику прототипа (`.md-rich`, uikit.css). react-markdown v10 + remark-gfm
// (таблицы, task-list `- [ ]`, strikethrough, autolinks).
//
// Security: rehype-raw НЕ подключаем — recap-markdown приходит из LLM. react-markdown
// рендерит React-узлы (не HTML-строки) и санитайзит опасные URL (`javascript:`) по
// умолчанию → сырой HTML/скрипты не исполняются.

import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

function cx(...parts: Array<string | undefined | null>): string {
  return parts.filter(Boolean).join(' ');
}

// Markdown → v2-разметка. Заголовки сохраняют семантический уровень (a11y) +
// получают `.md-h`. Списки/inline-код сохраняют классы плагина (`contains-task-list`,
// `language-*`). Остальные элементы (blockquote / pre / strong / em / del / hr /
// table / li.task-list-item) рендерятся семантическими тегами и стилятся bare-tag
// правилами под `.md-rich` в wk.css.
const MD_COMPONENTS: Components = {
  h1: ({ children }) => <h1 className="md-h">{children}</h1>,
  h2: ({ children }) => <h2 className="md-h">{children}</h2>,
  h3: ({ children }) => <h3 className="md-h">{children}</h3>,
  h4: ({ children }) => <h4 className="md-h">{children}</h4>,
  h5: ({ children }) => <h5 className="md-h">{children}</h5>,
  h6: ({ children }) => <h6 className="md-h">{children}</h6>,
  p: ({ children }) => <p className="md-p">{children}</p>,
  ul: ({ children, className }) => <ul className={cx('md-ul', className)}>{children}</ul>,
  ol: ({ children, className }) => <ol className={cx('md-ol', className)}>{children}</ol>,
  code: ({ children, className }) => <code className={cx('md-code', className)}>{children}</code>,
  a: ({ href, children }) => (
    <a className="md-a" href={href} target="_blank" rel="noreferrer noopener">
      {children}
    </a>
  ),
};

export interface MarkdownProps {
  /** Markdown-источник. */
  children: string;
  /** Доп. класс на обёртку `.md-rich`. */
  className?: string;
}

/** Рендер markdown-документа в типографику Wotold v2 (`.md-rich`). */
export function Markdown({ children, className }: MarkdownProps) {
  return (
    <div className={cx('md-rich', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
        {children}
      </ReactMarkdown>
    </div>
  );
}
