// [W3] RecStrip smoke + state-swap tests.
//   - hidden when idle (provider seeded with null state)
//   - shows the timer + "Recording" label when active
//   - swaps to "Paused" copy + signals data-paused when on pause

import { act, cleanup, render, screen } from '@testing-library/react';
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
import { RecordingProvider } from './RecordingContext';
import { RecStrip } from './RecStrip';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(() => {
  cleanup();
});

describe('RecStrip', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('renders nothing when recording state is idle', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state') return Promise.resolve(null);
      if (cmd === 'list_calls') return Promise.resolve([]);
      return Promise.resolve(null);
    });

    const { container } = render(
      <I18nProvider>
        <RecordingProvider>
          <RecStrip />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();
    expect(container.querySelector('.rec-strip')).toBeNull();
  });

  test('shows recording label + timer when active', async () => {
    const startedAt = new Date(Date.now() - 12_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      if (cmd === 'list_calls') return Promise.resolve([]);
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecStrip />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    // Russian copy (test setup pins navigator.language to ru-RU).
    expect(screen.getByText('Идёт запись')).toBeTruthy();
    // Timer is mm:ss; rough sanity — contains a colon.
    const strip = document.querySelector('.rec-strip');
    expect(strip).not.toBeNull();
    expect(strip?.getAttribute('data-paused')).toBe('false');
    expect(strip?.textContent ?? '').toMatch(/\d{2}:\d{2}/);
  });

  test('swaps to paused copy + data-paused="true" when on pause', async () => {
    const startedAt = new Date(Date.now() - 20_000).toISOString();
    const pausedAt = new Date(Date.now() - 5_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-2',
          started_at: startedAt,
          paused_at: pausedAt,
          paused_total_ms: 0,
        });
      if (cmd === 'list_calls') return Promise.resolve([]);
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecStrip />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    expect(screen.getByText('Пауза · записано')).toBeTruthy();
    const strip = document.querySelector('.rec-strip');
    expect(strip?.getAttribute('data-paused')).toBe('true');

    // Pause-action button switches to Resume (variant='play').
    const playBtn = document.querySelector('.rec-mini-btn--play');
    expect(playBtn).not.toBeNull();
  });
});
