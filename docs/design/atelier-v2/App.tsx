// ─────────────────────────────────────────────────────────────
// Sample · apps/desktop/src/App.tsx — rewritten with Atelier shell
// Logic preserved; navigation, recording flow, etc. unchanged.
// What changed: replaces topnav with sidebar rail, wraps in ThemeProvider,
// switches class names from .topnav-* to .app-rail / .nav-item.
// ─────────────────────────────────────────────────────────────

import { useEffect, useState } from 'react';

import { CallDetailPage } from './pages/CallDetailPage';
import { CallsPage } from './pages/CallsPage';
import { ContactsPage } from './pages/ContactsPage';
import { DesignSystemPage } from './pages/DesignSystemPage';
import { HomePage } from './pages/HomePage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { Button } from './ui';
import { getSetting, SETTINGS_KEYS } from './api/settings';
import { ThemeProvider } from './theme/useTheme';

type Page = 'home' | 'calls' | 'contacts' | 'settings' | 'ds';
type Bootstrap = 'loading' | 'onboarding' | 'app';

const NAV: Array<{ id: Page; label: string }> = [
  { id: 'home',     label: 'Главная'   },
  { id: 'calls',    label: 'Звонки'    },
  { id: 'contacts', label: 'Контакты'  },
  { id: 'settings', label: 'Настройки' },
];

const IS_DEV = import.meta.env.DEV;

function initialPage(): Page {
  if (typeof window === 'undefined') return 'home';
  const hash = window.location.hash.replace('#', '');
  if (hash === 'home' || hash === 'calls' || hash === 'contacts' || hash === 'settings') {
    return hash;
  }
  if (hash === 'ds' && IS_DEV) return 'ds';
  return 'home';
}

function AppShell() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [page, setPage] = useState<Page>(initialPage);
  const [detailCallId, setDetailCallId] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const done = await getSetting(SETTINGS_KEYS.ONBOARDING_DONE);
        setBootstrap(done === '1' ? 'app' : 'onboarding');
      } catch {
        setBootstrap('app');
      }
    })();
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.location.hash = page;
  }, [page]);

  if (bootstrap === 'loading') {
    return (
      <main className="app-shell" style={{ alignItems: 'center', justifyContent: 'center' }}>
        <p className="muted">…</p>
      </main>
    );
  }

  if (bootstrap === 'onboarding') {
    return <OnboardingPage onComplete={() => setBootstrap('app')} />;
  }

  return (
    <div className="app-shell">
      <aside className="app-rail" aria-label="Главная навигация">
        <div className="app-brand">
          Wotold
          <span className="app-brand-dot">.</span>
        </div>
        {NAV.map((item) => {
          const active = page === item.id;
          return (
            <button
              key={item.id}
              type="button"
              className={`nav-item${active ? ' nav-item--active' : ''}`}
              onClick={() => setPage(item.id)}
              aria-current={active ? 'page' : undefined}
            >
              {item.label}
            </button>
          );
        })}
        {IS_DEV && (
          <button
            type="button"
            className={`nav-item${page === 'ds' ? ' nav-item--active' : ''}`}
            onClick={() => setPage('ds')}
            title="Design system showcase (dev only)"
            style={{ marginTop: 12 }}
          >
            DS · dev
          </button>
        )}
        <div className="app-rail-foot">
          v1.0.0<br />
          Локально · macOS
        </div>
      </aside>

      <main className="app-main">
        {page === 'home' && (
          <HomePage
            onOpenCall={(id) => {
              setDetailCallId(id);
              setPage('calls');
            }}
          />
        )}
        {page === 'calls' &&
          ((detailCallId ?? new URLSearchParams(window.location.search).get('detail')) ? (
            <CallDetailPage
              callId={
                detailCallId ??
                (new URLSearchParams(window.location.search).get('detail') as string)
              }
              onBack={() => setDetailCallId(null)}
            />
          ) : (
            <CallsPage onOpen={(id) => setDetailCallId(id)} />
          ))}
        {page === 'contacts' && <ContactsPage />}
        {page === 'settings' && <SettingsPage />}
        {page === 'ds' && IS_DEV && <DesignSystemPage />}
      </main>
    </div>
  );
}

export function App() {
  return (
    <ThemeProvider>
      <AppShell />
    </ThemeProvider>
  );
}
