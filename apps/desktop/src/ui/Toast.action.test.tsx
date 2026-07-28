// Тост с действием. До этого `ToastOptions` нёс только строку — кнопки не
// было ни в типе, ни в разметке, ни в CSS.
import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ToastProvider, useToast } from './Toast';

afterEach(() => cleanup());

function Harness({ onAction }: { onAction?: () => void }) {
  const toast = useToast();
  return (
    <>
      <button
        type="button"
        onClick={() =>
          toast.show({
            message: 'Вышла версия 1.2.0',
            ...(onAction ? { action: { label: 'Обновить', onClick: onAction } } : {}),
          })
        }
      >
        raise
      </button>
    </>
  );
}

function renderHarness(onAction?: () => void) {
  return render(
    <ToastProvider>
      <Harness onAction={onAction} />
    </ToastProvider>,
  );
}

describe('toast action', () => {
  it('renders the action button and calls it', async () => {
    const onAction = vi.fn();
    const user = userEvent.setup();
    renderHarness(onAction);

    await user.click(screen.getByRole('button', { name: 'raise' }));
    const action = await screen.findByRole('button', { name: 'Обновить' });

    await user.click(action);
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it('dismisses itself once the action fires', async () => {
    const user = userEvent.setup();
    renderHarness(vi.fn());

    await user.click(screen.getByRole('button', { name: 'raise' }));
    await user.click(await screen.findByRole('button', { name: 'Обновить' }));

    expect(screen.queryByText('Вышла версия 1.2.0')).not.toBeInTheDocument();
  });

  /// Предложение, исчезающее через 4.5 секунды, — предложение, которое
  /// пользователь не успел прочесть.
  it('does not auto-dismiss when it carries an action', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      renderHarness(vi.fn());

      await user.click(screen.getByRole('button', { name: 'raise' }));
      expect(await screen.findByText('Вышла версия 1.2.0')).toBeInTheDocument();

      await act(async () => {
        vi.advanceTimersByTime(30_000);
      });

      expect(screen.getByText('Вышла версия 1.2.0')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  /// Обычный тост поведение сохраняет — иначе новая опция чинила бы одно и
  /// ломала всё остальное.
  it('still auto-dismisses without an action', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      renderHarness();

      await user.click(screen.getByRole('button', { name: 'raise' }));
      expect(await screen.findByText('Вышла версия 1.2.0')).toBeInTheDocument();

      await act(async () => {
        vi.advanceTimersByTime(10_000);
      });

      expect(screen.queryByText('Вышла версия 1.2.0')).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});
