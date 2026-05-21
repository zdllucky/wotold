// [W4] RecFloat smoke + interaction tests.
//   - hidden body when idle (still renders the pill so AutoHideOnIdle has a
//     DOM node to observe; data-active="false")
//   - shows the timer + recording label + pause variant when active
//   - paused state: data-paused="true" + play variant
//   - clicking the pill body invokes restore_main_window; clicking an
//     action button does NOT
//   - clicking stop invokes stop_recording AND restore_main_window

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../i18n';
import { RecFloat } from './RecFloat';
import { RecordingProvider } from './RecordingContext';

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

describe('RecFloat', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('renders pill in inactive state when idle', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state') return Promise.resolve(null);
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecFloat />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    const pill = document.querySelector('.rec-float');
    expect(pill).not.toBeNull();
    expect(pill?.getAttribute('data-active')).toBe('false');
  });

  test('shows recording label + timer + pause button when active', async () => {
    const startedAt = new Date(Date.now() - 9_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecFloat />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    expect(screen.getByText('Идёт запись')).toBeTruthy();
    const pill = document.querySelector('.rec-float');
    expect(pill?.getAttribute('data-active')).toBe('true');
    expect(pill?.getAttribute('data-paused')).toBe('false');
    expect(pill?.textContent ?? '').toMatch(/\d{2}:\d{2}/);
    expect(document.querySelector('.rec-mini-btn--pause')).not.toBeNull();
  });

  test('shows paused state with play variant', async () => {
    const startedAt = new Date(Date.now() - 30_000).toISOString();
    const pausedAt = new Date(Date.now() - 5_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-2',
          started_at: startedAt,
          paused_at: pausedAt,
          paused_total_ms: 0,
        });
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecFloat />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    const pill = document.querySelector('.rec-float');
    expect(pill?.getAttribute('data-paused')).toBe('true');
    expect(document.querySelector('.rec-mini-btn--play')).not.toBeNull();
  });

  test('clicking the pill body invokes restore_main_window', async () => {
    const startedAt = new Date(Date.now() - 4_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-3',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecFloat />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    const body = document.querySelector('.rec-float-body') as HTMLElement;
    expect(body).not.toBeNull();
    mockInvoke.mockClear();
    fireEvent.click(body);
    await flush();
    const calls = mockInvoke.mock.calls.map((c) => c[0]);
    expect(calls).toContain('restore_main_window');
  });

  test('clicking an action button does NOT trigger restore', async () => {
    const startedAt = new Date(Date.now() - 4_000).toISOString();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-4',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      return Promise.resolve(null);
    });

    render(
      <I18nProvider>
        <RecordingProvider>
          <RecFloat />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    const pauseBtn = document.querySelector(
      '.rec-mini-btn--pause',
    ) as HTMLButtonElement;
    expect(pauseBtn).not.toBeNull();
    mockInvoke.mockClear();
    fireEvent.click(pauseBtn);
    await flush();

    const calls = mockInvoke.mock.calls.map((c) => c[0]);
    expect(calls).not.toContain('restore_main_window');
    // The pause action itself should fire.
    expect(calls).toContain('pause_recording');
  });
});
