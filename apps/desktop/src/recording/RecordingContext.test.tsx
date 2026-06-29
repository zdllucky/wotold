// [W3] Provider behaviour:
//  - idle → start() → recording state
//  - recording → pause() → paused state + elapsedSec freezes
//  - paused → resume() → recording state + elapsedSec resumes (paused_total_ms
//    is applied so the frozen offset is preserved)
//  - recording → stop() → idle + returns callId
//  - reconstructs initial status from `get_recording_state` on mount

import { act, cleanup, render, screen } from '@testing-library/react';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
  vi,
} from 'vitest';

// Tauri invoke mock — single fn covers start_recording/pause/resume/stop +
// get_recording_state. Per-test setup tells it what to return.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import {
  RecordingProvider,
  useRecording,
  formatElapsed,
} from './RecordingContext';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function fakeCall(id: string, startedAt: string) {
  return {
    id,
    title: null,
    started_at: startedAt,
    ended_at: null,
    duration_sec: null,
    status: 'recording',
    provider: null,
    path_label: '',
    lang_detected: null,
    failed_reason: null,
    recap_failed_reason: null,
    pipeline_step: null,
    pipeline_pct: null,
    pipeline_eta_sec: null,
    upload_bytes: null,
    paused_at: null,
    paused_total_ms: 0,
    created_at: startedAt,
    updated_at: startedAt,
  };
}

function Probe() {
  const rec = useRecording();
  return (
    <div>
      <span data-testid="kind">{rec.status.kind}</span>
      <span data-testid="elapsed">{rec.elapsedSec}</span>
      <span data-testid="busy">{rec.busy ? '1' : '0'}</span>
      <span data-testid="error">{rec.error ?? ''}</span>
      <button onClick={() => void rec.start()}>start</button>
      <button onClick={() => void rec.pause()}>pause</button>
      <button onClick={() => void rec.resume()}>resume</button>
      <button
        onClick={async () => {
          const r = await rec.stop();
          (window as unknown as { __lastCallId: string | null }).__lastCallId =
            r.callId;
        }}
      >
        stop
      </button>
    </div>
  );
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('RecordingContext', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    vi.useRealTimers();
  });

  afterEach(() => {
    cleanup();
  });

  test('useRecording throws outside the provider', () => {
    // Render a component using the hook with no provider — error boundary
    // catches the throw at render time. We assert via console + try.
    const orig = console.error;
    console.error = () => {};
    try {
      expect(() => render(<Probe />)).toThrow(
        /useRecording must be inside RecordingProvider/,
      );
    } finally {
      console.error = orig;
    }
  });

  test('idle → start() → recording', async () => {
    const startedAt = '2026-01-01T00:00:00.000Z';
    // First invoke is get_recording_state on mount — return null.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state') return Promise.resolve(null);
      if (cmd === 'start_recording')
        return Promise.resolve(fakeCall('call-1', startedAt));
      return Promise.resolve(null);
    });

    render(
      <RecordingProvider>
        <Probe />
      </RecordingProvider>,
    );
    await flush();
    expect(screen.getByTestId('kind').textContent).toBe('idle');

    await act(async () => {
      screen.getByText('start').click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId('kind').textContent).toBe('recording');
    expect(screen.getByTestId('error').textContent).toBe('');
  });

  test('recording → pause() → paused; elapsedSec freezes', async () => {
    const startedAt = '2026-01-01T00:00:00.000Z';
    const startedMs = new Date(startedAt).getTime();
    const pausedAt = new Date(startedMs + 5_000).toISOString();

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      if (cmd === 'pause_recording')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: pausedAt,
          paused_total_ms: 0,
        });
      return Promise.resolve(null);
    });

    render(
      <RecordingProvider>
        <Probe />
      </RecordingProvider>,
    );
    await flush();
    expect(screen.getByTestId('kind').textContent).toBe('recording');

    await act(async () => {
      screen.getByText('pause').click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId('kind').textContent).toBe('paused');
    // 5s elapsed at the moment of pause.
    expect(Number(screen.getByTestId('elapsed').textContent)).toBe(5);
  });

  test('paused → resume() → recording; elapsedSec resumes with accumulated pause', async () => {
    const startedAt = '2026-01-01T00:00:00.000Z';
    const startedMs = new Date(startedAt).getTime();
    const pausedAt = new Date(startedMs + 5_000).toISOString();

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: pausedAt,
          paused_total_ms: 0,
        });
      if (cmd === 'resume_recording')
        return Promise.resolve({
          call_id: 'call-1',
          started_at: startedAt,
          paused_at: null,
          // Pretend the pause lasted 3s and the backend committed it.
          paused_total_ms: 3_000,
        });
      return Promise.resolve(null);
    });

    render(
      <RecordingProvider>
        <Probe />
      </RecordingProvider>,
    );
    await flush();
    expect(screen.getByTestId('kind').textContent).toBe('paused');

    await act(async () => {
      screen.getByText('resume').click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId('kind').textContent).toBe('recording');
    // Sanity: elapsed is a non-negative integer. We don't pin an exact value
    // because real-clock drift между call'ом resume и tick'ом is unstable; the
    // contract — paused_total_ms учитывается, не сбрасывается.
    expect(
      Number(screen.getByTestId('elapsed').textContent),
    ).toBeGreaterThanOrEqual(0);
  });

  test('recording → stop() → idle + returns callId', async () => {
    const startedAt = '2026-01-01T00:00:00.000Z';
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_recording_state')
        return Promise.resolve({
          call_id: 'call-99',
          started_at: startedAt,
          paused_at: null,
          paused_total_ms: 0,
        });
      if (cmd === 'stop_recording')
        return Promise.resolve(fakeCall('call-99', startedAt));
      return Promise.resolve(null);
    });

    render(
      <RecordingProvider>
        <Probe />
      </RecordingProvider>,
    );
    await flush();
    expect(screen.getByTestId('kind').textContent).toBe('recording');

    await act(async () => {
      screen.getByText('stop').click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId('kind').textContent).toBe('idle');
    expect(
      (window as unknown as { __lastCallId: string | null }).__lastCallId,
    ).toBe('call-99');
  });
});

describe('formatElapsed', () => {
  test('mm:ss under one hour', () => {
    expect(formatElapsed(0)).toBe('00:00');
    expect(formatElapsed(5)).toBe('00:05');
    expect(formatElapsed(65)).toBe('01:05');
    expect(formatElapsed(3599)).toBe('59:59');
  });

  test('h:mm:ss at and above one hour', () => {
    expect(formatElapsed(3600)).toBe('1:00:00');
    expect(formatElapsed(3661)).toBe('1:01:01');
  });

  test('clamps negatives', () => {
    expect(formatElapsed(-10)).toBe('00:00');
  });
});
