import { describe, expect, test, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { CallErrorRow } from './CallErrorRow';
import type { CallError } from '../../types/callState';
// useI18n() в test environment без Provider'а возвращает fallback ctx,
// который через navigator.language=ru-RU (см. src/test/setup.ts) отдаёт
// строки из ru-локали. Тесты без Provider'а быстрее и не trigger'ят
// async useEffect → settings invoke.

const mkError = (msg: string): CallError => ({
  code: 'STT_TIMEOUT',
  message: msg,
  attempts: 1,
  quotaConsumed: false,
});

describe('CallErrorRow', () => {
  test('shows short first phrase + audio reassurance', () => {
    const { container } = render(
      <CallErrorRow
        error={mkError('Превышено время ожидания — сервер не ответил')}
        onOpenDetails={() => {}}
      />,
    );
    const row = container.querySelector('.call-error-row');
    expect(row?.textContent).toContain('Превышено время ожидания');
    expect(row?.textContent).toContain('аудио сохранено');
  });

  test('falls back to default when message empty', () => {
    const { container } = render(
      <CallErrorRow error={mkError('')} onOpenDetails={() => {}} />,
    );
    const row = container.querySelector('.call-error-row');
    expect(row?.textContent).toContain('не удалось распознать');
  });

  test('details button fires onOpenDetails', async () => {
    const onOpen = vi.fn();
    render(<CallErrorRow error={mkError('boom')} onOpenDetails={onOpen} />);
    await userEvent.click(screen.getByRole('button'));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  test('splits on em-dash to keep label short', () => {
    const { container } = render(
      <CallErrorRow
        error={mkError('Сеть недоступна — попробуй ещё раз')}
        onOpenDetails={() => {}}
      />,
    );
    const row = container.querySelector('.call-error-row');
    expect(row?.textContent).toContain('Сеть недоступна');
    expect(row?.textContent).not.toContain('попробуй ещё раз');
  });
});
