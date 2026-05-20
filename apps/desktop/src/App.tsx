import { useEffect, useState } from 'react';

import { CallsPage } from './pages/CallsPage';
import { ContactsPage } from './pages/ContactsPage';
import { HomePage } from './pages/HomePage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { getSetting, SETTINGS_KEYS } from './api/settings';

type Page = 'home' | 'calls' | 'contacts' | 'settings';
type Bootstrap = 'loading' | 'onboarding' | 'app';

export function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [page, setPage] = useState<Page>('home');

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

  if (bootstrap === 'loading') {
    return (
      <main className="app">
        <p className="hint">…</p>
      </main>
    );
  }

  if (bootstrap === 'onboarding') {
    return <OnboardingPage onComplete={() => setBootstrap('app')} />;
  }

  return (
    <>
      <nav className="topnav">
        <button
          type="button"
          className={page === 'home' ? 'active' : ''}
          onClick={() => setPage('home')}
        >
          Главная
        </button>
        <button
          type="button"
          className={page === 'calls' ? 'active' : ''}
          onClick={() => setPage('calls')}
        >
          Звонки
        </button>
        <button
          type="button"
          className={page === 'contacts' ? 'active' : ''}
          onClick={() => setPage('contacts')}
        >
          Контакты
        </button>
        <button
          type="button"
          className={page === 'settings' ? 'active' : ''}
          onClick={() => setPage('settings')}
        >
          Настройки
        </button>
      </nav>

      <main className="app">
        {page === 'home' && <HomePage />}
        {page === 'calls' && <CallsPage />}
        {page === 'contacts' && <ContactsPage />}
        {page === 'settings' && <SettingsPage />}
      </main>
    </>
  );
}
