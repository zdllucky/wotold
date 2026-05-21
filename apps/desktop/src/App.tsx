import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { CallDetailPage } from './pages/CallDetailPage';
import { CallsPage } from './pages/CallsPage';
import { Coachmarks } from './pages/Coachmarks';
import { ContactsPage } from './pages/ContactsPage';
import { DesignSystemPage } from './pages/DesignSystemPage';
import { HomePage } from './pages/HomePage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { getSetting, SETTINGS_KEYS } from './api/settings';
import { I18nProvider, useI18n } from './i18n';
import { ThemeProvider } from './theme/useTheme';

type Page = 'home' | 'calls' | 'contacts' | 'settings' | 'ds';
type Bootstrap = 'loading' | 'onboarding' | 'app';

// [B17] Atelier v2: text-only nav rail (handoff §1). Active-indicator bar
// carries the affordance — see `.nav-item--active::before` in wotold.css.
const NAV_IDS: ReadonlyArray<Exclude<Page, 'ds'>> = [
  'home',
  'calls',
  'contacts',
  'settings',
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
  const { t } = useI18n();
  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [page, setPage] = useState<Page>(initialPage);
  const [detailCallId, setDetailCallId] = useState<string | null>(null);
  // [B16] Counter активных pipeline-задач. Subtle indicator в app-rail foot.
  const [activePipelines, setActivePipelines] = useState(0);

  const navLabel = (id: Exclude<Page, 'ds'>): string => {
    switch (id) {
      case 'home':
        return t('nav.home');
      case 'calls':
        return t('nav.calls');
      case 'contacts':
        return t('nav.contacts');
      case 'settings':
        return t('nav.settings');
    }
  };

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
    let unStart: UnlistenFn | undefined;
    let unFinish: UnlistenFn | undefined;
    listen('pipeline:started', () => setActivePipelines((n) => n + 1))
      .then((fn) => (unStart = fn))
      .catch((e: unknown) => console.warn('listen pipeline:started failed', e));
    listen('pipeline:finished', () => setActivePipelines((n) => Math.max(0, n - 1)))
      .then((fn) => (unFinish = fn))
      .catch((e: unknown) => console.warn('listen pipeline:finished failed', e));
    return () => {
      unStart?.();
      unFinish?.();
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.location.hash = page;
  }, [page]);

  // [Native UX] ПКМ context menu по умолчанию открывает webview-меню («Inspect»,
  // «Reload», «Back/Forward») — это веб-чувство, неприемлемо в Tauri-приложении.
  // Блокируем глобально, но whitelist'ом разрешаем на inputs/markdown/транскрипте
  // — там copy/paste/inspect-spelling действительно полезны юзеру.
  // В DEV оставляем меню работать чтобы можно было использовать Inspect.
  useEffect(() => {
    if (typeof document === 'undefined') return;
    if (IS_DEV) return;
    const ALLOW = 'input, textarea, [contenteditable="true"], .markdown, .markdown *, .transcript-row, .transcript-row *, .transcript-text, .title, .display, .subtitle, code, pre, kbd, [data-selectable], [data-selectable] *';
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(ALLOW)) return; // оставить нативное меню
      e.preventDefault();
    };
    document.addEventListener('contextmenu', onContextMenu);
    return () => document.removeEventListener('contextmenu', onContextMenu);
  }, []);

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

  const pipelinePlural =
    activePipelines === 1 ? t('nav.callsPluralOne') : t('nav.callsPluralMany');

  return (
    <div className="app-shell">
      <aside className="app-rail" aria-label={t('nav.main')}>
        <div className="app-brand" aria-hidden="true">
          Wotold
          <span className="app-brand-dot">.</span>
        </div>
        {NAV_IDS.map((id) => {
          const active = page === id;
          return (
            <button
              key={id}
              type="button"
              className={`nav-item${active ? ' nav-item--active' : ''}`}
              onClick={() => setPage(id)}
              aria-current={active ? 'page' : undefined}
            >
              {navLabel(id)}
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
            {t('nav.ds')}
          </button>
        )}
        {activePipelines > 0 && (
          <div
            role="status"
            aria-live="polite"
            title={t('nav.processingTitle', {
              n: activePipelines,
              plural: pipelinePlural,
            })}
            style={{
              marginTop: 'auto',
              padding: '10px 6px',
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontFamily: 'var(--font-sans)',
              fontSize: 11,
              color: 'var(--muted)',
              letterSpacing: '0.04em',
              textTransform: 'uppercase',
              fontWeight: 600,
            }}
          >
            <span className="dot dot--accent dot--pulse" aria-hidden />
            <span>
              {activePipelines === 1
                ? t('nav.processingOne')
                : t('nav.processingMany', { n: activePipelines })}
            </span>
          </div>
        )}
        <div className="app-rail-foot">
          v1.0.0<br />
          {t('nav.brandFooter')}
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
      <Coachmarks />
    </div>
  );
}

export function App() {
  return (
    <I18nProvider>
      <ThemeProvider>
        <AppShell />
      </ThemeProvider>
    </I18nProvider>
  );
}
