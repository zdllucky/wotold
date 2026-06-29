// Smoke tests for the Wotold v2 markdown renderer (Markdown / .md-rich).
// Покрывает полный набор элементов: заголовки (с сохранением уровня), абзацы,
// списки (ul/ol), inline/fenced код, цитаты, ссылки (rel/target), и GFM
// (таблицы, task-list, strikethrough).

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { Markdown } from './Markdown';

afterEach(() => cleanup());

describe('Markdown (.md-rich)', () => {
  test('wraps output in .md-rich', () => {
    render(<Markdown>{'привет'}</Markdown>);
    expect(document.querySelector('.md-rich')).toBeTruthy();
  });

  test('preserves heading level and adds .md-h', () => {
    render(<Markdown>{'## Заголовок'}</Markdown>);
    const h2 = document.querySelector('h2.md-h');
    expect(h2).toBeTruthy();
    expect(h2?.textContent).toBe('Заголовок');
  });

  test('renders paragraph as .md-p', () => {
    render(<Markdown>{'обычный текст'}</Markdown>);
    expect(document.querySelector('p.md-p')?.textContent).toBe('обычный текст');
  });

  test('renders unordered list as .md-ul', () => {
    render(<Markdown>{'- one\n- two'}</Markdown>);
    expect(document.querySelector('ul.md-ul')).toBeTruthy();
    expect(document.querySelectorAll('ul.md-ul li')).toHaveLength(2);
  });

  test('renders ordered list as <ol class=md-ol> (numbering preserved)', () => {
    render(<Markdown>{'1. first\n2. second'}</Markdown>);
    const ol = document.querySelector('ol.md-ol');
    expect(ol).toBeTruthy();
    expect(ol?.querySelectorAll('li')).toHaveLength(2);
  });

  test('renders inline code as .md-code', () => {
    render(<Markdown>{'текст `inline` код'}</Markdown>);
    expect(document.querySelector('code.md-code')?.textContent).toBe('inline');
  });

  test('renders fenced code block as <pre><code>', () => {
    render(<Markdown>{'```\nconst a = 1;\n```'}</Markdown>);
    const pre = document.querySelector('pre');
    expect(pre).toBeTruthy();
    expect(pre?.querySelector('code')?.textContent).toContain('const a = 1;');
  });

  test('renders blockquote', () => {
    render(<Markdown>{'> цитата'}</Markdown>);
    expect(document.querySelector('blockquote')?.textContent).toContain('цитата');
  });

  test('renders hr', () => {
    render(<Markdown>{'a\n\n---\n\nb'}</Markdown>);
    expect(document.querySelector('hr')).toBeTruthy();
  });

  test('renders link with safe rel and target', () => {
    render(<Markdown>{'[site](https://example.com)'}</Markdown>);
    const a = document.querySelector('a.md-a') as HTMLAnchorElement | null;
    expect(a).toBeTruthy();
    expect(a?.getAttribute('href')).toBe('https://example.com');
    expect(a?.getAttribute('target')).toBe('_blank');
    expect(a?.getAttribute('rel')).toContain('noreferrer');
    expect(a?.getAttribute('rel')).toContain('noopener');
  });

  test('does not render raw HTML (XSS-safe, no rehype-raw)', () => {
    render(<Markdown>{'<script>alert(1)</script> текст'}</Markdown>);
    expect(document.querySelector('script')).toBeNull();
  });

  describe('GFM', () => {
    test('renders a table with header and cells', () => {
      render(<Markdown>{'| A | B |\n| - | - |\n| 1 | 2 |'}</Markdown>);
      const table = document.querySelector('table');
      expect(table).toBeTruthy();
      expect(table?.querySelectorAll('thead th')).toHaveLength(2);
      expect(table?.querySelectorAll('tbody td')).toHaveLength(2);
    });

    test('renders a task list with checkboxes', () => {
      render(<Markdown>{'- [ ] todo\n- [x] done'}</Markdown>);
      const boxes = document.querySelectorAll<HTMLInputElement>(
        'li.task-list-item input[type="checkbox"]',
      );
      expect(boxes).toHaveLength(2);
      expect(boxes[1]?.checked).toBe(true);
    });

    test('renders strikethrough as <del>', () => {
      render(<Markdown>{'~~gone~~'}</Markdown>);
      expect(document.querySelector('del')?.textContent).toBe('gone');
    });
  });
});
