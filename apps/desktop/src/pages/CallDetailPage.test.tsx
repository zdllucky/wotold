// Phase 5 smoke test for CallDetailPage after R7 split.
//
// Goal: ensure the orchestrator + extracted sub-components + useCallDetail
// hook still mount without crashing for the three high-level states:
//  - loading (skeleton)
//  - error (allSettled rejected on call meta)
//  - notFound (allSettled resolved with null)
//
// Не покрывает: pipeline event listeners, mutating actions (delete /
// reprocess / regenerate-recap) — это outside-of-smoke и принадлежит будущим
// behavior-focused тестам. Inline component тесты остаются на месте
// (InteractiveTranscript.test.tsx, PipelineStrip.test.tsx и т.д.).

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

// Mock Tauri APIs before importing the page. `invoke` is shared across api/*
// modules (api/calls, api/contacts, api/speakers) — single mock covers all.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => `asset://${p}`,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {
    /* unlisten noop */
  }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(false),
  save: vi.fn().mockResolvedValue(null),
}));

import { invoke } from '@tauri-apps/api/core';
import { CallDetailPage } from './CallDetailPage';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

// jsdom does not implement HTMLMediaElement.{play,pause,load} — stub так
// что useCallAudio mount/cleanup не валит «Not implemented» в stderr.
if (typeof window !== 'undefined') {
  window.HTMLMediaElement.prototype.play = () => Promise.resolve();
  window.HTMLMediaElement.prototype.pause = () => {
    /* noop */
  };
  window.HTMLMediaElement.prototype.load = () => {
    /* noop */
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// Helper — Promise resolver for awaiting microtasks after async setState.
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('CallDetailPage — smoke', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('renders skeleton while initial load pending', () => {
    // All invocations stall (never resolve) — page stays in loading state.
    mockInvoke.mockImplementation(() => new Promise(() => {}));
    const { container } = render(
      <CallDetailPage callId="c-1" onBack={() => {}} />,
    );
    // Skeleton uses aria-busy="true" on the root section.
    expect(container.querySelector('[aria-busy="true"]')).not.toBeNull();
  });

  test('shows error alert when call meta load rejects', async () => {
    // First invoke is get_call — reject; all others resolve to safe defaults.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_call') return Promise.reject(new Error('boom'));
      if (cmd === 'list_contacts') return Promise.resolve([]);
      if (cmd === 'list_call_speakers') return Promise.resolve([]);
      if (cmd === 'list_call_action_items') return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(<CallDetailPage callId="c-1" onBack={() => {}} />);
    await flush();
    // humanError(Error('boom')) → 'boom'. role=alert is asserted.
    expect(screen.getByRole('alert')).toBeInTheDocument();
  });

  test('shows notFound copy when call meta resolves to null', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_call') return Promise.resolve(null);
      if (cmd === 'list_contacts') return Promise.resolve([]);
      if (cmd === 'list_call_speakers') return Promise.resolve([]);
      if (cmd === 'list_call_action_items') return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(<CallDetailPage callId="c-1" onBack={() => {}} />);
    await flush();
    // notFound copy ('Звонок не найден' / 'Call not found' depending on locale)
    // — locate by .muted class which is unique to this state branch.
    const muted = document.querySelector('p.muted');
    expect(muted).not.toBeNull();
    expect(muted?.textContent?.length ?? 0).toBeGreaterThan(0);
  });
});
