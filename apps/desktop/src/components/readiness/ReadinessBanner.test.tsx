// Баннер готовности: состояния и агрегированный прогресс.
//
// Проверяется поведение, за которым пришёл пользователь: пока модулей не
// хватает — видно сколько качать и кнопку; во время докачки — процент; после
// — баннера нет вовсе.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

const listeners = new Map<string, (e: { payload: unknown }) => void>();

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return () => listeners.delete(name);
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import { ReadinessBanner } from './ReadinessBanner';
import { ReadinessProvider } from './ReadinessProvider';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function emit(event: string, payload: unknown) {
  const cb = listeners.get(event);
  if (!cb) throw new Error(`нет подписки на ${event}`);
  act(() => cb({ payload }));
}

// Провайдер подписывается на три события последовательными await'ами, плюс
// тянет снимок готовности — двух микротасков не хватает, подписки не успевают
// зарегистрироваться.
async function flush() {
  await act(async () => {
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
  });
}

function renderBanner(onOpenSettings?: () => void) {
  return render(
    <ReadinessProvider>
      <ReadinessBanner onOpenSettings={onOpenSettings} />
    </ReadinessProvider>,
  );
}

const READY = { ready: true, preset: 'light', missing: [], missing_bytes_total: 0 };
const MISSING = {
  ready: false,
  preset: 'light',
  missing: [
    { id: 'silero-vad-v5', bytes_total: 885_098, state: 'absent' },
    { id: 'qwen25-0_5b', bytes_total: 397_808_192, state: 'absent' },
  ],
  missing_bytes_total: 398_693_290,
};

describe('ReadinessBanner', () => {
  beforeEach(() => {
    listeners.clear();
    mockInvoke.mockReset();
  });
  afterEach(() => cleanup());

  test('движок готов — баннера нет', async () => {
    mockInvoke.mockResolvedValue(READY);
    const { container } = renderBanner();
    await flush();
    expect(container.querySelector('.readiness-banner')).toBeNull();
  });

  test('не хватает модулей — размер и кнопка скачивания', async () => {
    mockInvoke.mockResolvedValue(MISSING);
    renderBanner();
    await flush();
    expect(screen.getByText(/не хватает части софта/i)).toBeInTheDocument();
    // 398 693 290 байт ≈ 380 MB — размер показывается человеку, а не в байтах.
    expect(screen.getByText(/380 MB/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Скачать' })).toBeInTheDocument();
  });

  test('прогресс агрегируется по всем модулям', async () => {
    mockInvoke.mockResolvedValue(MISSING);
    renderBanner();
    await flush();

    // Половина большого модуля + весь маленький ≈ 50%.
    emit('model:progress', {
      id: 'qwen25-0_5b',
      pct: 50,
      bytes_done: 198_904_096,
      bytes_total: 397_808_192,
    });
    emit('model:done', { id: 'silero-vad-v5', status: 'ok' });

    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
    expect(screen.getByText(/Скачиваем модули… 50%/)).toBeInTheDocument();
  });

  test('после докачки баннер уходит по событию готовности', async () => {
    mockInvoke.mockResolvedValue(MISSING);
    const { container } = renderBanner();
    await flush();
    expect(container.querySelector('.readiness-banner')).not.toBeNull();

    emit('readiness:changed', READY);

    expect(container.querySelector('.readiness-banner')).toBeNull();
  });

  test('размер движка не выбран — ведём в настройки, а не качаем', async () => {
    mockInvoke.mockResolvedValue({
      ready: false,
      preset: null,
      missing: [],
      missing_bytes_total: 0,
    });
    const onOpenSettings = vi.fn();
    renderBanner(onOpenSettings);
    await flush();

    expect(screen.getByText(/выберите размер движка/i)).toBeInTheDocument();
    // Кнопки скачивания быть не должно: сюрприз на несколько гигабайт.
    expect(screen.queryByRole('button', { name: 'Скачать' })).toBeNull();
    act(() => screen.getByRole('button', { name: 'Открыть настройки' }).click());
    expect(onOpenSettings).toHaveBeenCalled();
  });

  test('снимок не получен — баннера нет, но и «готов» мы не объявляем', async () => {
    // Та же ветка ловит и «команды нет» (не-macOS, R9), и обычный сбой вроде
    // занятой базы. Объявлять движок готовым на ошибке нельзя: это скрыло бы
    // единственную точку входа в докачку до перезапуска приложения.
    mockInvoke.mockRejectedValue(new Error('database is locked'));
    const { container } = renderBanner();
    await flush();
    expect(container.querySelector('.readiness-banner')).toBeNull();

    // Состояние осталось неизвестным — следующий снимок его исправит.
    emit('readiness:changed', MISSING);
    expect(container.querySelector('.readiness-banner')).not.toBeNull();
  });

  test('ошибка скачивания — сообщение и «Повторить»', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'local_engine_readiness') return MISSING;
      throw new Error('ENOSPC: no space left');
    });
    renderBanner();
    await flush();

    await act(async () => {
      screen.getByRole('button', { name: 'Скачать' }).click();
      await Promise.resolve();
      await Promise.resolve();
    });
    await flush();

    expect(screen.getByRole('alert')).toHaveTextContent(/места на диске/i);
    expect(screen.getByRole('button', { name: 'Повторить' })).toBeInTheDocument();
  });
});
