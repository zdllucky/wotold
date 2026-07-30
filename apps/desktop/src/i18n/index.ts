// i18n entrypoint — runtime locale state + translation function.
//
// Three locales:
//   - 'ru' — Russian (default fallback for STT-detected ru/ru-RU systems)
//   - 'kk' — Қазақша (Kazakh)
//   - 'en' — English (universal fallback when system locale matches nothing)
//
// Persisted via api/settings under SETTINGS_KEYS.UI_LOCALE. Empty = auto-detect.
// User override available in Settings → Внешний вид → Язык интерфейса.

import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import { en } from './en';
import { kk } from './kk';
import { ru, type TranslationStrings } from './ru';

export type Locale = 'ru' | 'kk' | 'en';

export const SUPPORTED_LOCALES: ReadonlyArray<{
  code: Locale;
  label: string;
  nativeLabel: string;
}> = [
  { code: 'ru', label: 'Russian', nativeLabel: 'Русский' },
  { code: 'kk', label: 'Kazakh', nativeLabel: 'Қазақша' },
  { code: 'en', label: 'English', nativeLabel: 'English' },
];

const STRINGS: Record<Locale, TranslationStrings> = { ru, kk, en };

/**
 * Detect system locale from navigator.language. Returns one of supported,
 * or 'en' as a universal fallback.
 *
 * Rules:
 *   - 'ru-*' / 'ru' → 'ru'
 *   - 'kk-*' / 'kz' (legacy) → 'kk'
 *   - 'en-*' / 'en' → 'en'
 *   - anything else → 'en'
 */
export function detectSystemLocale(): Locale {
  if (typeof navigator === 'undefined') return 'en';
  const raw = (navigator.language ?? '').toLowerCase();
  if (!raw) return 'en';
  if (raw.startsWith('ru')) return 'ru';
  if (raw.startsWith('kk') || raw.startsWith('kz')) return 'kk';
  if (raw.startsWith('en')) return 'en';
  return 'en';
}

function isLocale(v: unknown): v is Locale {
  return v === 'ru' || v === 'kk' || v === 'en';
}

// Translation keys are deep dotted paths into TranslationStrings. We use a
// recursive mapped type to enforce that callers stick to known keys.
type DotPath<T> = T extends string
  ? never
  : {
      [K in keyof T & string]: T[K] extends string
        ? K
        : `${K}.${DotPath<T[K]>}`;
    }[keyof T & string];

export type TranslationKey = DotPath<TranslationStrings>;

/**
 * Resolve a dotted key like `home.startAria` against the locale dictionary.
 * Falls back to ru → en if the key is missing in the active locale (defensive
 * — TypeScript enforces shape equality at compile time, but runtime safety
 * matters if dictionaries are loaded async in the future).
 */
function resolve(strings: TranslationStrings, key: string): string | null {
  const parts = key.split('.');
  let cursor: unknown = strings;
  for (const p of parts) {
    if (cursor && typeof cursor === 'object' && p in (cursor as Record<string, unknown>)) {
      cursor = (cursor as Record<string, unknown>)[p];
    } else {
      return null;
    }
  }
  return typeof cursor === 'string' ? cursor : null;
}

/**
 * Substitute `{name}` placeholders in a template string with values from
 * `params`. Missing params are left as `{name}` so they show up loudly in QA
 * rather than silently dropping.
 */
function template(str: string, params?: Record<string, string | number>): string {
  if (!params) return str;
  return str.replace(/\{(\w+)\}/g, (m, k: string) => {
    if (Object.prototype.hasOwnProperty.call(params, k)) {
      return String(params[k]);
    }
    return m;
  });
}

interface I18nCtx {
  locale: Locale;
  setLocale: (l: Locale) => void;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
}

/**
 * Default context — used when components are rendered outside a Provider
 * (RTL unit tests, Storybook). Falls back to detected locale and ru/en
 * dictionaries. setLocale is a no-op in that mode.
 */
function makeFallbackT(locale: Locale) {
  return (
    key: TranslationKey,
    params?: Record<string, string | number>,
  ): string => {
    const direct = resolve(STRINGS[locale], key);
    if (direct !== null) return template(direct, params);
    const fallbackRu = resolve(STRINGS.ru, key);
    if (fallbackRu !== null) return template(fallbackRu, params);
    const fallbackEn = resolve(STRINGS.en, key);
    if (fallbackEn !== null) return template(fallbackEn, params);
    return key;
  };
}

const Ctx = createContext<I18nCtx | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  // Start with detected locale so first paint never shows the wrong language
  // if persisted preference is empty/missing. If a stored value exists, we
  // upgrade after the async settings read resolves.
  const [locale, setLocaleState] = useState<Locale>(() => detectSystemLocale());
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const stored = await getSetting(SETTINGS_KEYS.UI_LOCALE);
        if (cancelled) return;
        if (isLocale(stored)) {
          setLocaleState(stored);
        }
      } catch (e) {
        console.warn('failed to load ui_locale setting', e);
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const setLocale = useCallback((l: Locale) => {
    setLocaleState(l);
    setSetting(SETTINGS_KEYS.UI_LOCALE, l).catch((e) =>
      console.warn('failed to persist ui_locale', e),
    );
  }, []);

  const t = useCallback(
    (key: TranslationKey, params?: Record<string, string | number>): string => {
      const active = STRINGS[locale];
      const direct = resolve(active, key);
      if (direct !== null) return template(direct, params);
      // Defensive fallback: ru, then en.
      const fallbackRu = resolve(STRINGS.ru, key);
      if (fallbackRu !== null) return template(fallbackRu, params);
      const fallbackEn = resolve(STRINGS.en, key);
      if (fallbackEn !== null) return template(fallbackEn, params);
      return key;
    },
    [locale],
  );

  const value = useMemo<I18nCtx>(
    () => ({ locale, setLocale, t }),
    [locale, setLocale, t],
  );

  // Block first paint until persisted preference (if any) is read — prevents
  // a one-render flash of the detected locale being overwritten by stored.
  if (!ready) return null;

  return createElement(Ctx.Provider, { value }, children);
}

/**
 * Кэш фолбэк-контекстов по локали.
 *
 * Идентичность обязана быть стабильной между рендерами: `t` попадает в
 * зависимости эффектов, и новая функция на каждый рендер означает
 * переподписку на события — а в худшем случае бесконечный цикл
 * «эффект → setState → рендер → новый t → эффект». Внутри провайдера `t`
 * стабилен (useCallback по локали), и фолбэк не имеет права быть слабее.
 *
 * Не useMemo: фолбэк отдаётся после раннего `return v`, то есть условно, а
 * условный хук — нарушение правил хуков. Модульная карта решает то же самое
 * без хуков.
 */
const FALLBACK_CTX = new Map<Locale, I18nCtx>();

function fallbackCtx(locale: Locale): I18nCtx {
  const cached = FALLBACK_CTX.get(locale);
  if (cached) return cached;
  const ctx: I18nCtx = {
    locale,
    setLocale: () => {
      /* noop outside Provider */
    },
    t: makeFallbackT(locale),
  };
  FALLBACK_CTX.set(locale, ctx);
  return ctx;
}

export function useI18n(): I18nCtx {
  const v = useContext(Ctx);
  if (v) return v;
  // Fallback for unit tests / Storybook rendering outside a Provider:
  // detect locale from navigator and use a no-op setLocale. Keeps tests
  // working without forcing every test to wrap with <I18nProvider>.
  return fallbackCtx(detectSystemLocale());
}

/**
 * Locale → BCP47 string suitable for `Intl.DateTimeFormat` / `toLocaleString`.
 * Currently mostly cosmetic — Russian dates work fine in both ru-RU and kk-KZ
 * locales, but exposing this keeps date formatting honest.
 */
export function bcp47(locale: Locale): string {
  switch (locale) {
    case 'ru':
      return 'ru-RU';
    case 'kk':
      return 'kk-KZ';
    case 'en':
      return 'en-US';
  }
}
