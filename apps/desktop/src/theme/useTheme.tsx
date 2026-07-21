// ─────────────────────────────────────────────────────────────
// Wotold · Theme hook + global ThemeProvider
// Drop into apps/desktop/src/theme/useTheme.tsx
//
// Persists choice to settings table (uses existing api/settings.ts).
// On boot, reads saved values and applies to <html data-theme data-accent>.
//
// Usage:
//   // Wrap your <App> once:
//   <ThemeProvider>
//     <App />
//   </ThemeProvider>
//
//   // Anywhere inside:
//   const { theme, setTheme, accent, setAccent } = useTheme();
// ─────────────────────────────────────────────────────────────

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';

export type Theme = 'light' | 'dark' | 'system';
// [B18.0] Wotold v2 = моно-графит (ink), один акцент. Тип сохраняем чтобы не
// ломать AppearanceSection; picker удаляется в B18.5. Не-ink значения визуально
// no-op (в tokens.css нет [data-accent] блоков).
export type Accent = 'bordeaux' | 'persian' | 'ink';

// [B21] Реестр-ключи вместо дублированных литералов (drift-guard).
const KEY_THEME = SETTINGS_KEYS.UI_THEME;
const KEY_ACCENT = SETTINGS_KEYS.UI_ACCENT;
const DEFAULT_THEME: Theme = 'system';
const DEFAULT_ACCENT: Accent = 'ink';

interface ThemeCtx {
  theme: Theme;
  resolvedTheme: 'light' | 'dark';
  setTheme: (t: Theme) => void;
  accent: Accent;
  setAccent: (a: Accent) => void;
}

const Ctx = createContext<ThemeCtx | null>(null);

/** Apply the theme/accent atomically to <html>. */
function applyToRoot(theme: Theme, accent: Accent): 'light' | 'dark' {
  const root = document.documentElement;
  const prefersDark =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches;
  const resolved: 'light' | 'dark' =
    theme === 'system' ? (prefersDark ? 'dark' : 'light') : theme;
  root.setAttribute('data-theme', resolved);
  root.setAttribute('data-accent', accent);
  // [B18.0] density фиксирован cozy (без переключателя). compact-токены в wk.css.
  root.setAttribute('data-density', 'cozy');
  return resolved;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(DEFAULT_THEME);
  const [accent, setAccentState] = useState<Accent>(DEFAULT_ACCENT);
  const [resolvedTheme, setResolvedTheme] = useState<'light' | 'dark'>('light');
  const [ready, setReady] = useState(false);

  // Load saved choice from settings on boot
  useEffect(() => {
    (async () => {
      try {
        const [t, a] = await Promise.all([
          getSetting(KEY_THEME),
          getSetting(KEY_ACCENT),
        ]);
        const nextTheme = (t as Theme | null) || DEFAULT_THEME;
        const nextAccent = (a as Accent | null) || DEFAULT_ACCENT;
        setThemeState(nextTheme);
        setAccentState(nextAccent);
        setResolvedTheme(applyToRoot(nextTheme, nextAccent));
      } catch {
        setResolvedTheme(applyToRoot(DEFAULT_THEME, DEFAULT_ACCENT));
      } finally {
        setReady(true);
      }
    })();
  }, []);

  // Re-apply if system theme changes while user is on "system"
  useEffect(() => {
    if (theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => setResolvedTheme(applyToRoot(theme, accent));
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [theme, accent]);

  const setTheme = useCallback(
    (t: Theme) => {
      setThemeState(t);
      setResolvedTheme(applyToRoot(t, accent));
      setSetting(KEY_THEME, t).catch((e) =>
        console.warn('failed to persist theme', e),
      );
    },
    [accent],
  );

  const setAccent = useCallback(
    (a: Accent) => {
      setAccentState(a);
      applyToRoot(theme, a);
      setSetting(KEY_ACCENT, a).catch((e) =>
        console.warn('failed to persist accent', e),
      );
    },
    [theme],
  );

  // Block first paint until tokens are applied — avoids flash of wrong theme
  if (!ready) return null;

  return (
    <Ctx.Provider value={{ theme, resolvedTheme, setTheme, accent, setAccent }}>
      {children}
    </Ctx.Provider>
  );
}

export function useTheme(): ThemeCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error('useTheme must be used inside <ThemeProvider>');
  return v;
}
