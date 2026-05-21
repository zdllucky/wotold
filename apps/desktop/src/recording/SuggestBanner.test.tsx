// [S5] SuggestBanner smoke tests.
//   - hidden by default
//   - shows banner when `recording:suggested` event arrives
//   - dismiss button removes the banner

import { act, cleanup, render, screen, fireEvent } from '@testing-library/react';
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

// Capture the listener registered by SuggestBanner so the test can fire
// fake `recording:suggested` events directly. listen() returns an unlisten fn.
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
import { SuggestBanner } from './SuggestBanner';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_recording_state') return Promise.resolve(null);
    return Promise.resolve(null);
  });
  for (const k of Object.keys(listeners)) delete listeners[k];
});

afterEach(() => {
  cleanup();
});

describe('SuggestBanner', () => {
  test('renders nothing when no suggestion event fired', async () => {
    const { container } = render(
      <I18nProvider>
        <RecordingProvider>
          <SuggestBanner />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();
    expect(container.querySelector('.suggest-banner')).toBeNull();
  });

  test('shows banner when recording:suggested event arrives', async () => {
    render(
      <I18nProvider>
        <RecordingProvider>
          <SuggestBanner />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    expect(listeners['recording:suggested']).toBeDefined();
    await act(async () => {
      listeners['recording:suggested']?.({
        payload: {
          bundle_id: 'us.zoom.xos',
          app_name: 'Zoom',
          reason: 'mic_busy_whitelisted_frontmost',
        },
      });
    });
    await flush();

    const banner = document.querySelector('.suggest-banner');
    expect(banner).not.toBeNull();
    // Title contains the app name (Russian copy uses {app} substitution).
    expect(banner?.textContent ?? '').toContain('Zoom');
  });

  test('dismiss removes banner', async () => {
    render(
      <I18nProvider>
        <RecordingProvider>
          <SuggestBanner />
        </RecordingProvider>
      </I18nProvider>,
    );
    await flush();

    await act(async () => {
      listeners['recording:suggested']?.({
        payload: {
          bundle_id: 'us.zoom.xos',
          app_name: 'Zoom',
          reason: 'mic_busy_whitelisted_frontmost',
        },
      });
    });
    await flush();

    expect(document.querySelector('.suggest-banner')).not.toBeNull();
    const dismiss = screen.getByText('Скрыть');
    fireEvent.click(dismiss);
    await flush();
    expect(document.querySelector('.suggest-banner')).toBeNull();
  });
});
