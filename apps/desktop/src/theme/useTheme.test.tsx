// Tests for useTheme.tsx — ThemeProvider context, theme persistence,
// applyToRoot behavior, and useTheme hook.
//
// Выбор акцента удалён вместе с наследием прошлого поколения: пикер сняли
// в B18.5, а
// атрибут data-accent писался в никуда — в токенах нет ни одного блока под него.

import { cleanup, render, renderHook, screen, act } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

// Mock settings API before importing useTheme
vi.mock('../api/settings', () => ({
  getSetting: vi.fn(),
  setSetting: vi.fn(),
  // [B21] useTheme читает реестр-ключи вместо локальных литералов.
  SETTINGS_KEYS: { UI_THEME: 'ui.theme' },
}));

import { getSetting, setSetting } from '../api/settings';
import { ThemeProvider, useTheme } from './useTheme';

const mockGetSetting = getSetting as ReturnType<typeof vi.fn>;
const mockSetSetting = setSetting as ReturnType<typeof vi.fn>;

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  // Reset html attributes
  document.documentElement.removeAttribute('data-theme');
});

// ─── ThemeProvider boot ──────────────────────────────────────────────────────

describe('ThemeProvider — boot', () => {
  test('loads theme from settings on mount', async () => {
    mockGetSetting.mockImplementation((key: string) => {
      if (key === 'ui.theme') return Promise.resolve('dark');
      return Promise.resolve(null);
    });
    mockSetSetting.mockResolvedValue(undefined);

    let result!: ReturnType<typeof useTheme>;
    function Inner() {
      result = useTheme();
      return null;
    }

    await act(async () => {
      render(
        <ThemeProvider>
          <Inner />
        </ThemeProvider>,
      );
    });

    expect(result.theme).toBe('dark');
  });

  test('falls back to defaults when settings return null', async () => {
    mockGetSetting.mockResolvedValue(null);
    mockSetSetting.mockResolvedValue(undefined);

    let result!: ReturnType<typeof useTheme>;
    function Inner() {
      result = useTheme();
      return null;
    }

    await act(async () => {
      render(
        <ThemeProvider>
          <Inner />
        </ThemeProvider>,
      );
    });

    expect(result.theme).toBe('system');
    // Атрибута акцента на корне быть не должно — механика удалена.
    expect(document.documentElement.getAttribute('data-accent')).toBeNull();
  });

  test('[B18.0] pins data-density="cozy" on root', async () => {
    mockGetSetting.mockResolvedValue(null);
    mockSetSetting.mockResolvedValue(undefined);

    await act(async () => {
      render(
        <ThemeProvider>
          <div />
        </ThemeProvider>,
      );
    });

    expect(document.documentElement.getAttribute('data-density')).toBe('cozy');
  });

  test('applies data-theme attribute to document root on boot', async () => {
    mockGetSetting.mockImplementation((key: string) => {
      if (key === 'ui.theme') return Promise.resolve('light');
      if (key === 'ui.accent') return Promise.resolve('bordeaux');
      return Promise.resolve(null);
    });
    mockSetSetting.mockResolvedValue(undefined);

    await act(async () => {
      render(<ThemeProvider><div /></ThemeProvider>);
    });

    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });

  test('сохранённый акцент больше ни на что не влияет', async () => {
    // Регрессия против возврата мёртвой механики: значение в базе может
    // остаться у старых пользователей, но на корень оно попадать не должно.
    mockGetSetting.mockImplementation((key: string) => {
      if (key === 'ui.theme') return Promise.resolve('light');
      if (key === 'ui.accent') return Promise.resolve('bordeaux');
      return Promise.resolve(null);
    });
    mockSetSetting.mockResolvedValue(undefined);

    await act(async () => {
      render(<ThemeProvider><div /></ThemeProvider>);
    });

    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(document.documentElement.getAttribute('data-accent')).toBeNull();
  });

  test('recovers gracefully when getSetting rejects', async () => {
    mockGetSetting.mockRejectedValue(new Error('DB error'));
    mockSetSetting.mockResolvedValue(undefined);

    await act(async () => {
      render(<ThemeProvider><div data-testid="child" /></ThemeProvider>);
    });

    // Should still render children (not crash)
    expect(screen.getByTestId('child')).toBeInTheDocument();
  });
});

// ─── setTheme ────────────────────────────────────────────────────────────────

describe('useTheme — setTheme', () => {
  test('setTheme updates theme state and persists', async () => {
    mockGetSetting.mockResolvedValue(null);
    mockSetSetting.mockResolvedValue(undefined);

    let result!: ReturnType<typeof useTheme>;
    function Inner() {
      result = useTheme();
      return null;
    }

    await act(async () => {
      render(
        <ThemeProvider>
          <Inner />
        </ThemeProvider>,
      );
    });

    await act(async () => {
      result.setTheme('dark');
    });

    expect(result.theme).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(mockSetSetting).toHaveBeenCalledWith('ui.theme', 'dark');
  });

  test('setTheme with "light" sets data-theme=light', async () => {
    mockGetSetting.mockResolvedValue(null);
    mockSetSetting.mockResolvedValue(undefined);

    let result!: ReturnType<typeof useTheme>;
    function Inner() {
      result = useTheme();
      return null;
    }

    await act(async () => {
      render(
        <ThemeProvider>
          <Inner />
        </ThemeProvider>,
      );
    });

    await act(async () => {
      result.setTheme('light');
    });

    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });
});

// ─── useTheme hook guard ─────────────────────────────────────────────────────

describe('useTheme — guard', () => {
  test('throws when used outside ThemeProvider', () => {
    // Suppress React's error boundary output
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => {
      renderHook(() => useTheme());
    }).toThrow('useTheme must be used inside <ThemeProvider>');
    consoleSpy.mockRestore();
  });
});

// ─── resolvedTheme ───────────────────────────────────────────────────────────

describe('useTheme — resolvedTheme', () => {
  test('resolvedTheme is "light" or "dark" (never "system")', async () => {
    mockGetSetting.mockResolvedValue(null);
    mockSetSetting.mockResolvedValue(undefined);

    let result!: ReturnType<typeof useTheme>;
    function Inner() {
      result = useTheme();
      return null;
    }

    await act(async () => {
      render(
        <ThemeProvider>
          <Inner />
        </ThemeProvider>,
      );
    });

    expect(['light', 'dark']).toContain(result.resolvedTheme);
  });
});
