// [B24.3] AnswerMsg — презентационный, без invoke-мока (шаблон CommandPalette.test).
// i18n падает на ru (navigator пиннится в test/setup.ts).

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { AssistantAnswer } from '@wotold/contracts';

// [B27.6] share_text — Rust-команда; мокаем api-модуль (канон FragmentRow.test).
const shareMock = vi.fn();
vi.mock('../../api/assistant', () => ({
  shareText: (...args: unknown[]) => shareMock(...args),
  getAssistantFragmentText: vi.fn(),
}));

import { AnswerMsg, fmtSourceClock, speakerColor } from './AnswerMsg';

function answer(overrides: Partial<AssistantAnswer> = {}): AssistantAnswer {
  return {
    kind: 'answer',
    text: 'Договорились о пилоте.',
    sources: [
      { callId: 'c1', callTitle: 'Синхрон по пилоту', startMs: 62000 },
      { callId: 'c2', callTitle: 'Планёрка', startMs: null },
    ],
    fragments: [
      {
        callId: 'c1',
        callTitle: 'Синхрон по пилоту',
        kind: 'transcript',
        speaker: 'owner',
        startMs: 62000,
        text: 'фиксируем локальный режим',
      },
      {
        callId: 'c2',
        callTitle: 'Планёрка',
        kind: 'recap',
        speaker: null,
        startMs: null,
        text: 'итог планёрки',
      },
    ],
    fragmentTokens: 1400,
    windowTokens: 8192,
    ...overrides,
  };
}

describe('AnswerMsg', () => {
  beforeEach(() => {
    shareMock.mockReset();
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it('answer: текст, источники, контекст, mono-строка', () => {
    render(<AnswerMsg messageId="m1" createdAt="2026-07-23T10:00:00Z" answer={answer()} question="q" onOpenCall={() => {}} />);
    expect(screen.getByText('Договорились о пилоте.')).toBeInTheDocument();
    // Чужие звонки: «Название · т/к» и «Название» без таймкода (чипы-кнопки).
    expect(screen.getByRole('button', { name: 'Синхрон по пилоту · 1:02' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Планёрка' })).toBeInTheDocument();
    expect(screen.getByText('Контекст поиска')).toBeInTheDocument();
    expect(screen.getByText('фрагментов: 2 · ≈1.4K токенов · окно 8K')).toBeInTheDocument();
  });

  it('свой звонок: чип = таймкод → onSeek; чужой → onOpenCall', () => {
    const onSeek = vi.fn();
    const onOpenCall = vi.fn();
    render(
      <AnswerMsg messageId="m1" createdAt="2026-07-23T10:00:00Z" answer={answer()} question="q" callId="c1" onSeek={onSeek} onOpenCall={onOpenCall} />,
    );
    fireEvent.click(screen.getByRole('button', { name: '1:02' }));
    expect(onSeek).toHaveBeenCalledWith(62000);
    fireEvent.click(screen.getByRole('button', { name: 'Планёрка' }));
    expect(onOpenCall).toHaveBeenCalledWith('c2');
  });

  it('refusal: нота без «Контекста поиска» и без действий', () => {
    render(
      <AnswerMsg
        messageId="m1"
        createdAt="2026-07-23T10:00:00Z"
        answer={answer({ kind: 'refusal', sources: [], fragments: [], fragmentTokens: 0 })}
        question="q"
      />,
    );
    expect(screen.getByText('Вне области ассистента')).toBeInTheDocument();
    expect(screen.queryByText('Контекст поиска')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Скопировать')).not.toBeInTheDocument();
  });

  it('empty + escalate: чип «Искать во всех звонках» → onAskGlobal(question)', () => {
    const onAskGlobal = vi.fn();
    render(
      <AnswerMsg
        messageId="m1"
        createdAt="2026-07-23T10:00:00Z"
        answer={answer({
          kind: 'empty',
          text: 'В этом звонке этого не нашлось.',
          sources: [],
          fragments: [],
          fragmentTokens: 0,
          escalate: true,
        })}
        question="мой вопрос"
        onAskGlobal={onAskGlobal}
      />,
    );
    fireEvent.click(screen.getByText('Искать во всех звонках'));
    expect(onAskGlobal).toHaveBeenCalledWith('мой вопрос');
  });

  it('copy копирует текст; share зовёт нативный пикер с источниками', () => {
    shareMock.mockResolvedValue(undefined);
    render(<AnswerMsg messageId="m1" createdAt="2026-07-23T10:00:00Z" answer={answer()} question="q" />);
    fireEvent.click(screen.getByLabelText('Скопировать'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('Договорились о пилоте.');

    fireEvent.click(screen.getByLabelText('Поделиться'));
    expect(shareMock).toHaveBeenCalledWith(
      'Договорились о пилоте.\n\nИсточники: Синхрон по пилоту · 1:02; Планёрка',
      expect.objectContaining({ x: expect.any(Number), w: expect.any(Number) }),
    );
  });

  it('share: отказ команды → тихий фоллбек копией с источниками', async () => {
    shareMock.mockRejectedValue(new Error('unsupported platform'));
    render(<AnswerMsg messageId="m1" createdAt="2026-07-23T10:00:00Z" answer={answer()} question="q" />);
    fireEvent.click(screen.getByLabelText('Поделиться'));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        'Договорились о пилоте.\n\nИсточники: Синхрон по пилоту · 1:02; Планёрка',
      ),
    );
  });

  // [B27.8] Интерактивные [N]-ссылки: клик раскрывает контекст + подсвечивает.
  it('[N] в тексте — кнопка; клик раскрывает контекст и подсвечивает фрагмент', () => {
    const { container } = render(
      <AnswerMsg
        messageId="m1"
        createdAt="2026-07-23T10:00:00Z"
        answer={answer({ text: 'Итог (фрагменты [2]).' })}
        question="q"
        onOpenCall={() => {}}
      />,
    );
    const ref = screen.getByRole('button', { name: 'Фрагмент 2' });
    expect(ref).toHaveTextContent('[2]');
    const details = container.querySelector('details.ctx') as HTMLDetailsElement;
    expect(details.open).toBe(false);

    fireEvent.click(ref);
    expect(details.open).toBe(true);
    const wraps = container.querySelectorAll('.frag-wrap');
    expect(wraps[1]!.className).toContain('frag--flash');
    expect(wraps[0]!.className).not.toContain('frag--flash');
  });

  it('[N] вне диапазона остаётся текстом; controlled summary работает', () => {
    const { container } = render(
      <AnswerMsg
        messageId="m1"
        createdAt="2026-07-23T10:00:00Z"
        answer={answer({ text: 'см. [7]' })}
        question="q"
        onOpenCall={() => {}}
      />,
    );
    expect(screen.queryByRole('button', { name: 'Фрагмент 7' })).not.toBeInTheDocument();
    expect(screen.getByText(/см\. \[7\]/)).toBeInTheDocument();

    const details = container.querySelector('details.ctx') as HTMLDetailsElement;
    fireEvent.click(screen.getByText('Контекст поиска'));
    expect(details.open).toBe(true);
    fireEvent.click(screen.getByText('Контекст поиска'));
    expect(details.open).toBe(false);
  });

  // [B27.1] Время и действия — в одном ряду .ans-foot.
  it('время сообщения соседствует с действиями в .ans-foot', () => {
    const { container } = render(
      <AnswerMsg
        messageId="m1"
        createdAt="2026-07-23T10:00:00Z"
        answer={answer()}
        question="q"
        onOpenCall={() => {}}
      />,
    );
    const foot = container.querySelector('.ans-foot') as HTMLElement;
    expect(foot.querySelector('.ans-acts')).not.toBeNull();
    expect(foot.querySelector('.msg-time')).not.toBeNull();
  });

  it('fmtSourceClock: минуты без часов', () => {
    expect(fmtSourceClock(0)).toBe('0:00');
    expect(fmtSourceClock(62000)).toBe('1:02');
    expect(fmtSourceClock(4400000)).toBe('73:20');
  });

  it('speakerColor: owner → sp1, стабильный hash для прочих', () => {
    expect(speakerColor('c1', 'owner')).toBe('var(--sp1)');
    expect(speakerColor('c1', null)).toBe('var(--sp1)');
    const a = speakerColor('c1', 'Speaker 0');
    expect(a).toBe(speakerColor('c1', 'Speaker 0'));
    expect(a).toMatch(/^var\(--sp[2-5]\)$/);
  });
});
