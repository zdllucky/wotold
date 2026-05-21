// Smoke tests for i18n provider, locale detection, and template
// substitution. Covers: detectSystemLocale heuristic, t() lookup and
// fallback chain, {placeholder} substitution, and Provider persistence
// through SETTINGS_KEYS.UI_LOCALE.

import { describe, expect, test, vi, beforeEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { detectSystemLocale, I18nProvider, useI18n, bcp47 } from './index';

// Mock the settings API so tests do not require a Tauri backend.
const settingsStore = new Map<string, string>();

vi.mock('../api/settings', async (orig) => {
  const actual = await orig<typeof import('../api/settings')>();
  return {
    ...actual,
    getSetting: vi.fn(async (key: string) => settingsStore.get(key) ?? null),
    setSetting: vi.fn(async (key: string, value: string) => {
      settingsStore.set(key, value);
    }),
  };
});

beforeEach(() => {
  settingsStore.clear();
});

describe('detectSystemLocale', () => {
  test('returns "ru" for ru-RU', () => {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'ru-RU',
    });
    expect(detectSystemLocale()).toBe('ru');
  });

  test('returns "kk" for kk-KZ', () => {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'kk-KZ',
    });
    expect(detectSystemLocale()).toBe('kk');
  });

  test('returns "kk" for legacy kz', () => {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'kz',
    });
    expect(detectSystemLocale()).toBe('kk');
  });

  test('returns "en" for en-US', () => {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'en-US',
    });
    expect(detectSystemLocale()).toBe('en');
  });

  test('falls back to "en" for unknown locale', () => {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'fr-FR',
    });
    expect(detectSystemLocale()).toBe('en');
  });
});

describe('bcp47', () => {
  test('maps locale codes to BCP47 strings', () => {
    expect(bcp47('ru')).toBe('ru-RU');
    expect(bcp47('kk')).toBe('kk-KZ');
    expect(bcp47('en')).toBe('en-US');
  });
});

function Probe() {
  const { locale, t, setLocale } = useI18n();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <span data-testid="home-title">{t('home.readyHeadline')}</span>
      <span data-testid="templated">
        {t('home.savedHint', { sec: 42 })}
      </span>
      <button onClick={() => setLocale('en')}>switch-en</button>
      <button onClick={() => setLocale('kk')}>switch-kk</button>
    </div>
  );
}

async function renderWithProvider() {
  // Force Russian as the detected default before mounting; setup.ts already
  // pins navigator.language, but be explicit for clarity.
  Object.defineProperty(navigator, 'language', {
    configurable: true,
    get: () => 'ru-RU',
  });
  let utils: ReturnType<typeof render> | undefined;
  await act(async () => {
    utils = render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
  });
  return utils!;
}

describe('I18nProvider + useI18n', () => {
  test('mounts with detected ru locale and translates a key', async () => {
    await renderWithProvider();
    expect(screen.getByTestId('locale').textContent).toBe('ru');
    expect(screen.getByTestId('home-title').textContent).toBe('Готов записывать.');
  });

  test('substitutes {placeholders} via params', async () => {
    await renderWithProvider();
    expect(screen.getByTestId('templated').textContent).toContain('42');
  });

  test('setLocale switches strings and persists to settings', async () => {
    await renderWithProvider();
    expect(screen.getByTestId('home-title').textContent).toBe('Готов записывать.');
    await act(async () => {
      screen.getByText('switch-en').click();
    });
    expect(screen.getByTestId('locale').textContent).toBe('en');
    expect(screen.getByTestId('home-title').textContent).toBe('Ready to record.');
    // Persistence check — value should land in our mock store.
    const { getSetting } = await import('../api/settings');
    await Promise.resolve();
    expect(await getSetting('ui_locale')).toBe('en');
  });

  test('setLocale → kk produces Kazakh translation', async () => {
    await renderWithProvider();
    await act(async () => {
      screen.getByText('switch-kk').click();
    });
    expect(screen.getByTestId('locale').textContent).toBe('kk');
    expect(screen.getByTestId('home-title').textContent).toBe('Жазуға дайын.');
  });
});

describe('useI18n outside Provider (fallback)', () => {
  test('returns a working t() without crashing', () => {
    // Render Probe without provider — should resolve via fallback context.
    expect(() => render(<Probe />)).not.toThrow();
    // Locale defaults to detected (ru per setup.ts).
    expect(screen.getByTestId('locale').textContent).toBe('ru');
  });
});
