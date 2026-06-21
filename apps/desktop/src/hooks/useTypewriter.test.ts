import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

import { useTypewriter } from './useTypewriter';

function setReducedMotion(reduce: boolean) {
  window.matchMedia = vi.fn().mockImplementation((q: string) => ({
    matches: reduce,
    media: q,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    onchange: null,
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('useTypewriter', () => {
  beforeEach(() => setReducedMotion(false));

  test('enabled=false → текст сразу целиком', () => {
    const { result } = renderHook(() => useTypewriter('привет мир', false));
    expect(result.current.shown).toBe('привет мир');
    expect(result.current.done).toBe(true);
  });

  test('reduced-motion → текст сразу целиком даже при enabled', () => {
    setReducedMotion(true);
    const { result } = renderHook(() => useTypewriter('привет мир', true));
    expect(result.current.shown).toBe('привет мир');
    expect(result.current.done).toBe(true);
  });

  test('enabled → постепенно растёт до полного', () => {
    vi.useFakeTimers();
    const text = 'это довольно длинный текст саммари для проверки reveal';
    const { result } = renderHook(() => useTypewriter(text, true));
    // старт — пусто, не done.
    expect(result.current.shown.length).toBeLessThan(text.length);
    expect(result.current.done).toBe(false);
    // прокрутить всю анимацию.
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(result.current.shown).toBe(text);
    expect(result.current.done).toBe(true);
  });

  test('пустой текст → done сразу', () => {
    const { result } = renderHook(() => useTypewriter('', true));
    expect(result.current.shown).toBe('');
    expect(result.current.done).toBe(true);
  });
});
