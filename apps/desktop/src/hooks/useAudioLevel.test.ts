// Tests for useAudioLevel.ts — rolling audio levels from Tauri event.

import { renderHook, act, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

// Mock Tauri event module before importing the hook
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

import { listen } from '@tauri-apps/api/event';
import { useAudioLevel } from './useAudioLevel';

const mockListen = listen as ReturnType<typeof vi.fn>;

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

// ─── Initial state ───────────────────────────────────────────────────────────

describe('useAudioLevel — initial state', () => {
  test('returns zero arrays when inactive', () => {
    mockListen.mockResolvedValue(() => {});
    const { result } = renderHook(() => useAudioLevel(false));
    expect(result.current.mic.every((v) => v === 0)).toBe(true);
    expect(result.current.system.every((v) => v === 0)).toBe(true);
    expect(result.current.connected).toBe(false);
    expect(result.current.lastUpdate).toBe(0);
  });

  test('mic and system buffers have default length (140)', () => {
    mockListen.mockResolvedValue(() => {});
    const { result } = renderHook(() => useAudioLevel(false));
    expect(result.current.mic.length).toBe(140);
    expect(result.current.system.length).toBe(140);
  });

  test('custom bufferSize respected', () => {
    mockListen.mockResolvedValue(() => {});
    const { result } = renderHook(() => useAudioLevel(false, 50));
    expect(result.current.mic.length).toBe(50);
    expect(result.current.system.length).toBe(50);
  });
});

// ─── Subscribe when active ───────────────────────────────────────────────────

describe('useAudioLevel — active subscribe', () => {
  test('calls listen("audio:level") when active=true', async () => {
    let capturedHandler: ((e: { payload: { mic: number; system: number } }) => void) | null = null;
    mockListen.mockImplementation((_event: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    renderHook(() => useAudioLevel(true));
    await act(async () => {});

    expect(mockListen).toHaveBeenCalledWith('audio:level', expect.any(Function));
  });

  test('does NOT call listen when active=false', () => {
    mockListen.mockResolvedValue(() => {});
    renderHook(() => useAudioLevel(false));
    expect(mockListen).not.toHaveBeenCalled();
  });
});

// ─── Deactivation reset ──────────────────────────────────────────────────────

describe('useAudioLevel — deactivation', () => {
  test('resets to zero arrays on deactivation (active→false)', async () => {
    mockListen.mockResolvedValue(() => {});
    const { result, rerender } = renderHook(
      ({ active }: { active: boolean }) => useAudioLevel(active),
      { initialProps: { active: true } },
    );

    await act(async () => {});

    rerender({ active: false });

    expect(result.current.mic.every((v) => v === 0)).toBe(true);
    expect(result.current.connected).toBe(false);
    expect(result.current.lastUpdate).toBe(0);
  });
});

// ─── Unlisten on unmount ─────────────────────────────────────────────────────

describe('useAudioLevel — cleanup', () => {
  test('unlisten is called on unmount', async () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValue(unlisten);

    const { unmount } = renderHook(() => useAudioLevel(true));
    await act(async () => {});

    unmount();
    expect(unlisten).toHaveBeenCalled();
  });

  test('listen rejection is handled gracefully (no crash)', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mockListen.mockRejectedValue(new Error('no tauri'));

    const { result } = renderHook(() => useAudioLevel(true));
    await act(async () => {});

    // Should still have valid initial state
    expect(result.current.mic.length).toBe(140);
    warnSpy.mockRestore();
  });
});

// ─── clamp01 edge cases via payload ──────────────────────────────────────────

describe('useAudioLevel — payload processing', () => {
  test('processes audio:level event and updates buffers', async () => {
    vi.useFakeTimers();

    let capturedHandler: ((e: { payload: { mic: number; system: number } }) => void) | null = null;
    mockListen.mockImplementation((_event: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    const { result } = renderHook(() => useAudioLevel(true));
    await act(async () => {});

    // Fire event with valid payload
    await act(async () => {
      capturedHandler?.({ payload: { mic: 0.75, system: 0.5 } });
      // Advance timer to trigger RAF tick
      vi.advanceTimersByTime(100);
    });

    // Last element of buffer should reflect the new value
    expect(result.current.mic[result.current.mic.length - 1]).toBe(0.75);
    expect(result.current.system[result.current.system.length - 1]).toBe(0.5);
  });

  test('clamps values above 1 to 1', async () => {
    vi.useFakeTimers();

    let capturedHandler: ((e: { payload: { mic: number; system: number } }) => void) | null = null;
    mockListen.mockImplementation((_event: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    const { result } = renderHook(() => useAudioLevel(true));
    await act(async () => {});

    await act(async () => {
      capturedHandler?.({ payload: { mic: 2.5, system: -0.3 } });
      vi.advanceTimersByTime(100);
    });

    expect(result.current.mic[result.current.mic.length - 1]).toBe(1);
    expect(result.current.system[result.current.system.length - 1]).toBe(0);
  });

  test('clamps NaN to 0', async () => {
    vi.useFakeTimers();

    let capturedHandler: ((e: { payload: { mic: number; system: number } }) => void) | null = null;
    mockListen.mockImplementation((_event: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    const { result } = renderHook(() => useAudioLevel(true));
    await act(async () => {});

    await act(async () => {
      capturedHandler?.({ payload: { mic: NaN, system: Infinity } });
      vi.advanceTimersByTime(100);
    });

    // clamp01: !Number.isFinite → returns 0 for both NaN and Infinity
    expect(result.current.mic[result.current.mic.length - 1]).toBe(0);
    expect(result.current.system[result.current.system.length - 1]).toBe(0);
  });
});
