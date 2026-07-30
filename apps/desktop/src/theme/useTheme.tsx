// ─────────────────────────────────────────────────────────────
// Wotold · Theme hook + global ThemeProvider
// Drop into apps/desktop/src/theme/useTheme.tsx
//
// Persists choice to settings table (uses existing api/settings.ts).
// On boot, reads the saved value and applies it to <html data-theme>.
//
// Выбор акцента убран: он остался от прошлого поколения дизайна, пикер сняли
// в B18.5, а
// в токенах нет ни одного блока [data-accent] — атрибут писался в никуда.
//
// Usage:
//   // Wrap your <App> once:
//   <ThemeProvider>
//     <App />
//   </ThemeProvider>
//
//   // Anywhere inside:
//   const { theme, setTheme } = useTheme();
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

// [B21] Реестр-ключи вместо дублированных литералов (drift-guard).
const KEY_THEME = SETTINGS_KEYS.UI_THEME;
const DEFAULT_THEME: Theme = 'system';

interface ThemeCtx {
  theme: Theme;
  resolvedTheme: 'light' | 'dark';
  setTheme: (t: Theme) => void;
}

const Ctx = createContext<ThemeCtx | null>(null);

/** Apply the theme to <html>. */
function applyToRoot(theme: Theme): 'light' | 'dark' {
  const root = document.documentElement;
  const prefersDark =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches;
  const resolved: 'light' | 'dark' =
    theme === 'system' ? (prefersDark ? 'dark' : 'light') : theme;
  root.setAttribute('data-theme', resolved);
  // [B18.0] density фиксирован cozy (без переключателя). compact-токены в wk.css.
  root.setAttribute('data-density', 'cozy');
  return resolved;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(DEFAULT_THEME);
  const [resolvedTheme, setResolvedTheme] = useState<'light' | 'dark'>('light');
  const [ready, setReady] = useState(false);

  // Load saved choice from settings on boot
  useEffect(() => {
    (async () => {
      try {
        const t = await getSetting(KEY_THEME);
        const nextTheme = (t as Theme | null) || DEFAULT_THEME;
        setThemeState(nextTheme);
        setResolvedTheme(applyToRoot(nextTheme));
      } catch {
        setResolvedTheme(applyToRoot(DEFAULT_THEME));
      } finally {
        setReady(true);
      }
    })();
  }, []);

  // Re-apply if system theme changes while user is on "system"
  useEffect(() => {
    if (theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => setResolvedTheme(applyToRoot(theme));
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [theme]);

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t);
    setResolvedTheme(applyToRoot(t));
    setSetting(KEY_THEME, t).catch((e) => console.warn('failed to persist theme', e));
  }, []);

  // Block first paint until tokens are applied — avoids flash of wrong theme
  if (!ready) return null;

  return (
    <Ctx.Provider value={{ theme, resolvedTheme, setTheme }}>
      {children}
    </Ctx.Provider>
  );
}

export function useTheme(): ThemeCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error('useTheme must be used inside <ThemeProvider>');
  return v;
}
