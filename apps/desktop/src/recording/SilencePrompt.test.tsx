// [T7/R15] SilencePrompt — баннер «в записи тишина, остановить?».
//
// Проверяем клей, а не вёрстку: событие → баннер, обе кнопки → правильное
// действие, авто-стоп → баннер снят. Логика решения о тишине живёт в Rust
// (`audio/silence_watch`) и покрыта там.

import { act, cleanup, render, screen, fireEvent } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

type EventCb = (event: { payload: unknown }) => void;
const listeners: Record<string, EventCb> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: EventCb) => {
    listeners[name] = cb;
    return () => {
      delete listeners[name];
    };
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../i18n';
import { RecordingProvider } from './RecordingContext';
import { SilencePrompt } from './SilencePrompt';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

/** [T7 review] Стоп идёт через App.onStop, а не через rec.stop() напрямую. */
let onStop: ReturnType<typeof vi.fn>;

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

/** Смонтировать баннер поверх активной записи. */
async function mount() {
  const view = render(
    <I18nProvider>
      <RecordingProvider>
        <SilencePrompt onStop={onStop} />
      </RecordingProvider>
    </I18nProvider>,
  );
  await flush();
  return view;
}

async function firePrompt(autoStopInMs: number | null = 900_000) {
  await act(async () => {
    listeners['recording:silence_prompt']?.({
      payload: {
        call_id: 'c1',
        silent_for_ms: 900_000,
        auto_stop_in_ms: autoStopInMs,
      },
    });
  });
  await flush();
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    // Активная запись: иначе баннер снимет себя как неактуальный.
    if (cmd === 'get_recording_state') {
      return Promise.resolve({
        call_id: 'c1',
        started_at: new Date().toISOString(),
        paused_at: null,
        paused_total_ms: 0,
      });
    }
    return Promise.resolve(null);
  });
  for (const k of Object.keys(listeners)) delete listeners[k];
  onStop = vi.fn(async () => {});
});

afterEach(() => {
  cleanup();
});

describe('SilencePrompt', () => {
  test('renders nothing until a silence event arrives', async () => {
    const { container } = await mount();
    expect(container.querySelector('.suggest-banner')).toBeNull();
  });

  test('shows the banner with minutes and the auto-stop deadline', async () => {
    await mount();
    await firePrompt(600_000);

    const banner = screen.getByTestId('silence-prompt');
    expect(banner.textContent ?? '').toContain('15');
    expect(banner.textContent ?? '').toContain('10');
    expect(banner.getAttribute('role')).toBe('status');
  });

  test('omits the deadline when auto-stop is set to never', async () => {
    await mount();
    await firePrompt(null);

    const banner = screen.getByTestId('silence-prompt');
    // «Через N мин запись остановится сама» не должно появляться.
    expect(banner.textContent ?? '').not.toContain('остановится сама');
  });

  // [T7 review] Регрессия: прямой rec.stop() не показывал тост «слишком
  // коротко» для отброшенной записи и не уводил на страницу звонка — ровно
  // тот же обход общего флоу, что чинили у SuggestBanner со стартом.
  test('stop button goes through the app stop flow', async () => {
    await mount();
    await firePrompt();

    fireEvent.click(screen.getByText('Остановить запись'));
    await flush();

    expect(onStop).toHaveBeenCalledTimes(1);
    expect(mockInvoke).not.toHaveBeenCalledWith('stop_recording');
    expect(screen.queryByTestId('silence-prompt')).toBeNull();
  });

  // [T7 review] Регрессия: на паузе наблюдатель сбрасывает счётчик тишины, и
  // обещание «через N мин остановится сама» становится враньём — юзер решил
  // бы, что запись уже кончилась, пока она стоит на паузе.
  test('banner disappears when the recording is paused', async () => {
    await mount();
    await firePrompt();
    expect(screen.queryByTestId('silence-prompt')).not.toBeNull();

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state') {
        return Promise.resolve({
          call_id: 'c1',
          started_at: new Date().toISOString(),
          paused_at: new Date().toISOString(),
          paused_total_ms: 0,
        });
      }
      return Promise.resolve(null);
    });
    await act(async () => {
      listeners['recording:state']?.({ payload: null });
    });
    await flush();

    expect(screen.queryByTestId('silence-prompt')).toBeNull();
  });

  test('continue button snoozes the watcher instead of stopping', async () => {
    await mount();
    await firePrompt();

    fireEvent.click(screen.getByText('Продолжить'));
    await flush();

    expect(mockInvoke).toHaveBeenCalledWith('snooze_silence_watch');
    expect(mockInvoke).not.toHaveBeenCalledWith('stop_recording');
    expect(screen.queryByTestId('silence-prompt')).toBeNull();
  });

  test('auto-stop event clears the banner and announces itself', async () => {
    await mount();
    await firePrompt();
    expect(screen.queryByTestId('silence-prompt')).not.toBeNull();

    await act(async () => {
      listeners['recording:auto_stopped']?.({
        payload: { call_id: 'c1', silent_for_ms: 1_800_000, trimmed_ms: 1_795_000 },
      });
    });
    await flush();

    expect(screen.queryByTestId('silence-prompt')).toBeNull();
    // Уведомление про остановку уходит через ту же Rust-команду, что и
    // подсказка: строки живут в i18n, а не в Rust (правило 4).
    expect(mockInvoke).toHaveBeenCalledWith(
      'show_notification',
      expect.objectContaining({ title: 'Запись остановлена' }),
    );
  });

  test('does not auto-dismiss on a timer', async () => {
    // SC 2.2.1: у вопроса с последствиями не должно быть таймаута, который
    // принимает решение за пользователя. SuggestBanner гасится за 30с — здесь
    // это было бы ошибкой.
    vi.useFakeTimers();
    try {
      await mount();
      await firePrompt();
      await act(async () => {
        vi.advanceTimersByTime(120_000);
      });
      expect(screen.queryByTestId('silence-prompt')).not.toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
