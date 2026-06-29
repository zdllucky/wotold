// Smoke tests for RecapView — Wotold v2 recap макет (rich/markdown toggle).

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { RecapView } from './RecapView';

afterEach(() => cleanup());

describe('RecapView', () => {
  test('renders recap markdown as .md-rich by default', () => {
    render(<RecapView recap={'## Сводка\n\nТекст звонка'} emptyHint="пусто" />);
    expect(document.querySelector('.md-rich')).toBeTruthy();
    expect(screen.getByRole('heading')).toHaveTextContent('Сводка');
    // rich-маппинг: заголовок → .md-h
    expect(document.querySelector('.md-h')).toBeTruthy();
  });

  test('renders rich markdown elements (gfm table) through Markdown', () => {
    render(
      <RecapView recap={'## Итоги\n\n| Тема | Статус |\n| - | - |\n| A | ok |'} emptyHint="пусто" />,
    );
    expect(document.querySelector('.md-rich table')).toBeTruthy();
    expect(document.querySelectorAll('.md-rich tbody td')).toHaveLength(2);
  });

  test('toggles to markdown mode (pre.md-raw with raw source)', () => {
    render(<RecapView recap={'# Заголовок\n\nТело документа'} emptyHint="пусто" />);
    fireEvent.click(screen.getByText('Markdown'));
    const pre = document.querySelector('pre.md-raw');
    expect(pre).toBeTruthy();
    expect(pre?.textContent).toContain('# Заголовок');
  });

  test('copy button present for non-empty recap', () => {
    render(<RecapView recap={'текст'} emptyHint="пусто" />);
    expect(screen.getByRole('button', { name: /копировать|copy/i })).toBeTruthy();
  });

  test('shows regenerate CTA when recap blank and onRegenerate provided', () => {
    const onRegenerate = vi.fn();
    render(
      <RecapView
        recap={null}
        emptyHint="пусто"
        emptyBody="саммари ещё не создано"
        onRegenerate={onRegenerate}
      />,
    );
    expect(screen.getByText('саммари ещё не создано')).toBeInTheDocument();
    expect(document.querySelector('.md-rich')).toBeNull();
  });

  test('shows generating block (caret) when blank + generating', () => {
    render(
      <RecapView
        recap={null}
        emptyHint="пусто"
        generating
        generatingLabel="генерируется…"
      />,
    );
    expect(screen.getByText('генерируется…')).toBeInTheDocument();
    expect(document.querySelector('.caret')).toBeTruthy();
  });
});
