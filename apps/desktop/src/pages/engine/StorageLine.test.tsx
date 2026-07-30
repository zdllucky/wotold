// Строка «Модели занимают X» + кнопка «Освободить Y».
//
// Главное, что проверяется: удаление гигабайтов не происходит без явного
// подтверждения, а кнопки нет вовсе, когда освобождать нечего.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import { StorageLine } from './StorageLine';

afterEach(() => cleanup());

describe('StorageLine', () => {
  test('показывает занятое место человеку, а не в байтах', () => {
    render(
      <StorageLine usedBytes={3_035_762_038} reclaimableBytes={0} onFreeSpace={vi.fn()} />,
    );
    expect(screen.getByText(/Модели занимают 2\.8 GB/)).toBeInTheDocument();
  });

  test('нечего освобождать — кнопки нет', () => {
    render(<StorageLine usedBytes={1_000} reclaimableBytes={0} onFreeSpace={vi.fn()} />);
    expect(screen.queryByRole('button')).toBeNull();
  });

  test('удаление только через подтверждение', async () => {
    const onFreeSpace = vi.fn(async () => 539_212_467);
    render(
      <StorageLine
        usedBytes={3_035_762_038}
        reclaimableBytes={539_212_467}
        onFreeSpace={onFreeSpace}
      />,
    );

    // Мелкие размеры — в мегабайтах: «0.5 GB» на кнопке читалось бы как ничто.
    const cta = screen.getByRole('button', { name: /Освободить 514 MB/ });
    act(() => cta.click());
    expect(onFreeSpace).not.toHaveBeenCalled();

    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
    await act(async () => {
      screen.getByRole('button', { name: 'Удалить' }).click();
      await Promise.resolve();
    });
    expect(onFreeSpace).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/освободили 514 MB/)).toBeInTheDocument();
  });

  test('отмена подтверждения ничего не удаляет', () => {
    const onFreeSpace = vi.fn();
    render(
      <StorageLine usedBytes={10} reclaimableBytes={2_000_000_000} onFreeSpace={onFreeSpace} />,
    );
    act(() => screen.getByRole('button', { name: /Освободить/ }).click());
    act(() => screen.getByRole('button', { name: 'Отмена' }).click());
    expect(onFreeSpace).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});
