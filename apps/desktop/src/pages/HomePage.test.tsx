// [W5] HomePage smoke + Hero copy + Stop→onOpenCall integration.
//
// Подходом mirrors RecStrip.test.tsx: реальный RecordingProvider seed'нут
// `get_recording_state` Tauri-моком. Это даёт настоящий useRecording() flow
// без отдельного vi.mock на RecordingContext.

import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
  vi,
} from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../i18n';
import { RecordingProvider } from '../recording/RecordingContext';
import { HomePage } from './HomePage';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

function setupInvokeMock(opts: {
  recordingState?: {
    call_id: string;
    started_at: string;
    paused_at: string | null;
    paused_total_ms: number;
  } | null;
  stopReturns?: { id: string; started_at: string; duration_sec: number | null };
  consentAt?: string | null;
}) {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'get_recording_state':
        return Promise.resolve(opts.recordingState ?? null);
      case 'list_calls':
        return Promise.resolve([]);
      case 'get_setting':
        // Тестам нужен `recording.consent.at` пресет — иначе onStart
        // покажет consent modal, а не запустит запись.
        return Promise.resolve(opts.consentAt ?? null);
      case 'check_for_update':
        return Promise.resolve(null);
      case 'stop_recording':
        return Promise.resolve(
          opts.stopReturns ?? {
            id: 'call-stub',
            started_at: new Date().toISOString(),
            duration_sec: 5,
          },
        );
      case 'start_recording':
        return Promise.resolve({
          id: 'call-new',
          started_at: new Date().toISOString(),
          duration_sec: null,
        });
      default:
        return Promise.resolve(null);
    }
  });
}

function renderHome(props: { onOpenCall?: (id: string) => void } = {}) {
  return render(
    <I18nProvider>
      <RecordingProvider>
        <HomePage onOpenCall={props.onOpenCall} />
      </RecordingProvider>
    </I18nProvider>,
  );
}

afterEach(() => {
  cleanup();
});

describe('HomePage', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('renders idle hero with start button', async () => {
    setupInvokeMock({ recordingState: null });
    renderHome();
    await flush();

    // Hero headline — ru fallback (test setup pins ru-RU).
    expect(screen.getByText('Готов записывать.')).toBeTruthy();
    // Big red start button is visible (its aria-label).
    expect(screen.getByLabelText('Начать запись')).toBeTruthy();
  });

  test('shows "recording in background" headline when status is recording', async () => {
    setupInvokeMock({
      recordingState: {
        call_id: 'call-1',
        started_at: new Date(Date.now() - 5_000).toISOString(),
        paused_at: null,
        paused_total_ms: 0,
      },
    });
    renderHome();
    await flush();

    await waitFor(() =>
      expect(screen.getByText('Запись идёт фоном.')).toBeTruthy(),
    );
    // Big red start button is hidden when not idle.
    expect(screen.queryByLabelText('Начать запись')).toBeNull();
  });

  test('shows "paused" headline + subtitle when status is paused', async () => {
    setupInvokeMock({
      recordingState: {
        call_id: 'call-2',
        started_at: new Date(Date.now() - 20_000).toISOString(),
        paused_at: new Date(Date.now() - 5_000).toISOString(),
        paused_total_ms: 0,
      },
    });
    renderHome();
    await flush();

    await waitFor(() =>
      expect(screen.getByText('Запись на паузе.')).toBeTruthy(),
    );
    expect(
      screen.getByText(/Звук сейчас не пишется/),
    ).toBeTruthy();
    expect(screen.queryByLabelText('Начать запись')).toBeNull();
  });

  test('does not surface the legacy "saved" card anymore', async () => {
    setupInvokeMock({ recordingState: null });
    renderHome();
    await flush();
    // Раньше после stop появлялась карточка с «✓ Звонок сохранён». В W5
    // её роль занял переход на CallDetailPage; на idle экране её быть не должно.
    expect(screen.queryByText(/Звонок сохранён/)).toBeNull();
  });
});
