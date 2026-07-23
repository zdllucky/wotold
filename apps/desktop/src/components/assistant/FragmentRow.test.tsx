// [B26.6] FragmentRow: усечённый текст, lazy-раскрытие, очистка DOM при
// сворачивании, ошибка загрузки.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { AssistantFragment } from '@wotold/contracts';

import { FragmentRow } from './FragmentRow';

const getText = vi.fn();
vi.mock('../../api/assistant', () => ({
  getAssistantFragmentText: (...args: unknown[]) => getText(...args),
}));

function frag(overrides: Partial<AssistantFragment> = {}): AssistantFragment {
  return {
    callId: 'c1',
    callTitle: 'Синхрон',
    kind: 'transcript',
    speaker: 'owner',
    startMs: 1000,
    text: 'короткое превью…',
    textTruncated: true,
    ...overrides,
  };
}

beforeEach(() => {
  getText.mockReset();
});

describe('FragmentRow', () => {
  it('раскрытие лениво грузит полный текст, сворачивание вычищает его из DOM', async () => {
    getText.mockResolvedValue('полный длинный текст фрагмента целиком');
    render(<FragmentRow fragment={frag()} index={3} messageId="m42" />);

    expect(screen.getByText(/короткое превью/)).toBeInTheDocument();
    const btn = screen.getByRole('button', { name: /показать целиком/ });
    expect(btn).toHaveAttribute('aria-expanded', 'false');

    fireEvent.click(btn);
    await waitFor(() =>
      expect(screen.getByText(/полный длинный текст фрагмента/)).toBeInTheDocument(),
    );
    expect(getText).toHaveBeenCalledWith('m42', 3);

    // Сворачивание: полный текст ушёл из DOM (state вычищен).
    fireEvent.click(screen.getByRole('button', { name: /свернуть/ }));
    expect(screen.queryByText(/полный длинный текст фрагмента/)).not.toBeInTheDocument();
    expect(screen.getByText(/короткое превью/)).toBeInTheDocument();
  });

  it('не-truncated фрагмент рендерится без кнопки раскрытия', () => {
    render(
      <FragmentRow
        fragment={frag({ text: 'весь текст', textTruncated: false })}
        index={0}
        messageId="m1"
      />,
    );
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(getText).not.toHaveBeenCalled();
  });

  it('ошибка загрузки показывает ноту и откатывает раскрытие', async () => {
    getText.mockRejectedValue(new Error('boom'));
    render(<FragmentRow fragment={frag()} index={0} messageId="m1" />);
    fireEvent.click(screen.getByRole('button', { name: /показать целиком/ }));
    await waitFor(() =>
      expect(screen.getByText(/Не удалось загрузить фрагмент/)).toBeInTheDocument(),
    );
    expect(
      screen.getByRole('button', { name: /показать целиком/ }),
    ).toHaveAttribute('aria-expanded', 'false');
  });
});
