import { useEffect, useState, type ComponentType } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  Home,
  Phone,
  Users,
  Settings as SettingsIcon,
  type LucideProps,
} from 'lucide-react';

import { CallDetailPage } from './pages/CallDetailPage';
import { CallsPage } from './pages/CallsPage';
import { Coachmarks } from './pages/Coachmarks';
import { ContactsPage } from './pages/ContactsPage';
import { DesignSystemPage } from './pages/DesignSystemPage';
import { HomePage } from './pages/HomePage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { Button } from './ui';
import { getSetting, SETTINGS_KEYS } from './api/settings';

type Page = 'home' | 'calls' | 'contacts' | 'settings' | 'ds';
type Bootstrap = 'loading' | 'onboarding' | 'app';

// [B16] Topnav SVG icons из lucide-react — tree-shaken (~6kb на 4 иконки).
const NAV: Array<{
  id: Page;
  label: string;
  Icon: ComponentType<LucideProps>;
}> = [
  { id: 'home', label: 'Главная', Icon: Home },
  { id: 'calls', label: 'Звонки', Icon: Phone },
  { id: 'contacts', label: 'Контакты', Icon: Users },
  { id: 'settings', label: 'Настройки', Icon: SettingsIcon },
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

export function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [page, setPage] = useState<Page>(initialPage);
  const [detailCallId, setDetailCallId] = useState<string | null>(null);
  // [B16] Counter активных pipeline-задач. Подписки: 'pipeline:started' (++)
  // и 'pipeline:finished' (--). При >0 показываем subtle indicator в topnav.
  const [activePipelines, setActivePipelines] = useState(0);

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

  if (bootstrap === 'loading') {
    return (
      <main className="app app--center">
        <p className="hint">…</p>
      </main>
    );
  }

  if (bootstrap === 'onboarding') {
    return <OnboardingPage onComplete={() => setBootstrap('app')} />;
  }

  return (
    <>
      <nav className="topnav" aria-label="Главная навигация">
        <span className="topnav-brand" aria-hidden="true">Wotold</span>
        <div className="topnav-tabs" role="tablist">
          {NAV.map((item) => {
            const active = page === item.id;
            const Icon = item.Icon;
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                className={`topnav-tab${active ? ' topnav-tab--active' : ''}`}
                onClick={() => setPage(item.id)}
                aria-selected={active}
                aria-current={active ? 'page' : undefined}
              >
                <Icon
                  className="topnav-tab-icon"
                  size={16}
                  strokeWidth={2}
                  aria-hidden
                />
                <span className="topnav-tab-label">{item.label}</span>
              </button>
            );
          })}
        </div>
        <span className="topnav-spacer" />
        {activePipelines > 0 && (
          <span
            className="topnav-pipeline-indicator"
            role="status"
            aria-live="polite"
            title={`Обработка ${activePipelines} ${activePipelines === 1 ? 'звонка' : 'звонков'}…`}
          >
            <span className="topnav-pipeline-spinner" aria-hidden />
            <span className="topnav-pipeline-label">
              {activePipelines === 1 ? 'обрабатываем…' : `обрабатываем ${activePipelines}…`}
            </span>
          </span>
        )}
        {IS_DEV && (
          <Button
            size="sm"
            variant={page === 'ds' ? 'secondary' : 'ghost'}
            onClick={() => setPage('ds')}
            title="Design system showcase (dev only)"
          >
            🛠 DS
          </Button>
        )}
      </nav>

      <main className="app">
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
    </>
  );
}
