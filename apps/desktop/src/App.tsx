import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';

import { CallDetailPage } from './pages/CallDetailPage';
import { CallsPage } from './pages/CallsPage';
import { Coachmarks } from './pages/Coachmarks';
import { ContactsPage } from './pages/ContactsPage';
import { DesignSystemPage } from './pages/DesignSystemPage';
import { HomePage } from './pages/HomePage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { getSetting, SETTINGS_KEYS } from './api/settings';
import { getActivePipelineCount } from './api/calls';
import { I18nProvider, useI18n } from './i18n';
import { RecordingProvider, useRecording } from './recording/RecordingContext';
import { RecStrip } from './recording/RecStrip';
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
  const rec = useRecording();
  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [page, setPage] = useState<Page>(initialPage);
  const [detailCallId, setDetailCallId] = useState<string | null>(null);
  // [B16] Counter активных pipeline-задач. Subtle indicator в app-rail foot.
  // [V8.2] Источник правды — DB (list_calls на mount + pipeline events).
  // Раньше считали только +1 на started / −1 на finished — pipeline:cancelled
  // не учитывался → счётчик зависал, как и stale processing записи после
  // crash recovery (status sweep'нулся, а в-памяти counter всё равно 0).
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

  // [V8.2] Active-pipeline counter — DB source of truth. На mount считаем
  // status IN ('recording','processing') из list_calls. События только
  // инкрементят/декрементят без resync (быстро), а полная пере-сверка
  // выполняется на pipeline:cancelled и при возврате окна в фокус.
  useEffect(() => {
    const resync = async () => {
      try {
        // [V9] Источник правды — in-memory pipeline_tasks registry, а не
        // DB filter. Раньше counter показывал «3» при одной активной
        // обработке потому что DB содержал zombie processing rows от
        // прошлых crashed sessions (sweep_stale_calls пометил их failed
        // только на следующем startup). Сейчас точный realtime count.
        const n = await getActivePipelineCount();
        setActivePipelines(n);
      } catch (e) {
        console.warn('getActivePipelineCount failed:', e);
      }
    };
    void resync();

    const unlisteners: UnlistenFn[] = [];
    const attach = async () => {
      try {
        unlisteners.push(
          await listen('pipeline:started', () =>
            setActivePipelines((n) => n + 1),
          ),
        );
        unlisteners.push(
          await listen('pipeline:finished', () =>
            setActivePipelines((n) => Math.max(0, n - 1)),
          ),
        );
        // V8: cancelled event — раньше counter не декрементился и зависал.
        unlisteners.push(
          await listen('pipeline:cancelled', () => {
            // Full resync — race с finished возможен (cancel пришёл уже
            // после того как pipeline эмитнул finished), точный +/- ненадёжен.
            void resync();
          }),
        );
      } catch (e) {
        console.warn('pipeline event listeners failed:', e);
      }
    };
    void attach();

    // Перепроверяем counter когда окно вернулось в фокус — sweep_stale_calls
    // мог пометить зависшие звонки failed, а events мы пропустили (приложение
    // было свёрнуто/sleep).
    const onVisible = () => {
      if (document.visibilityState === 'visible') void resync();
    };
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      for (const u of unlisteners) u();
      document.removeEventListener('visibilitychange', onVisible);
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
  // [V8.1] Раньше был `if (IS_DEV) return;` — DEV всё ещё пускал webview-меню,
  // а это путало во время тестирования "финального" UX. Inspect остался
  // доступен через DevTools (⌥⌘I в Tauri dev shell).
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const ALLOW = 'input, textarea, [contenteditable="true"], .markdown, .markdown *, .transcript-row, .transcript-row *, .transcript-text, .title, .display, .subtitle, code, pre, kbd, [data-selectable], [data-selectable] *';
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(ALLOW)) return; // оставить нативное меню
      e.preventDefault();
    };
    document.addEventListener('contextmenu', onContextMenu);
    return () => document.removeEventListener('contextmenu', onContextMenu);
  }, []);

  // [W5] Cmd+W / Cmd+Q with active recording → confirm before close.
  // Rust side (lib.rs) уже делает graceful stop при CloseRequested, но без
  // подтверждения. Мы перехватываем keydown проактивно: если запись идёт —
  // показываем native ask(). Cancel → absorb keystroke; OK → stop + let
  // OS обработать close нормально. Не пересекается с Rust handler потому
  // что rec.stop() уже завершит запись до того как окно закроется.
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const handler = async (e: KeyboardEvent) => {
      if (!e.metaKey || e.shiftKey || e.altKey || e.ctrlKey) return;
      const isCloseKey =
        e.code === 'KeyW' || e.code === 'KeyQ';
      if (!isCloseKey) return;
      // Idle — пусть OS закроет окно нормально.
      if (rec.status.kind === 'idle') return;
      e.preventDefault();
      e.stopPropagation();
      try {
        const ok = await ask(t('home.closeWhileRecordingBody'), {
          title: t('home.closeWhileRecordingTitle'),
          kind: 'warning',
          okLabel: t('home.closeWhileRecordingOk'),
          cancelLabel: t('common.cancel'),
        });
        if (!ok) return;
        // Stop the recording cleanly before letting Rust shut down. После
        // stop'а rec.status.kind === 'idle' и Rust handler сходит с no-op.
        try {
          await rec.stop();
        } catch (err) {
          console.warn('stop on close failed', err);
        }
        // Имитируем повторное нажатие — на этот раз без перехвата.
        const reissue = new KeyboardEvent('keydown', {
          key: e.key,
          code: e.code,
          metaKey: true,
          shiftKey: false,
          altKey: false,
          ctrlKey: false,
        });
        window.dispatchEvent(reissue);
      } catch (err) {
        console.warn('close confirmation failed', err);
      }
    };
    // capture=true — get the event before page handlers (HomePage hotkey)
    // could absorb it. matchEvent в HomePage не матчит Cmd+W/Q (хоткей это
    // Cmd+Shift+R/P), но capture лишним не будет.
    window.addEventListener('keydown', handler, { capture: true });
    return () => window.removeEventListener('keydown', handler, { capture: true });
  }, [rec, t]);

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
          // [V8.3] Activity badge inline в Звонки nav-item — macOS Mail
          // convention (passive indicator + count). Заменяет standalone
          // .nav-activity button и .activity-strip banner на CallsPage.
          const showBadge = id === 'calls' && activePipelines > 0;
          return (
            <button
              key={id}
              type="button"
              className={`nav-item${active ? ' nav-item--active' : ''}`}
              onClick={() => setPage(id)}
              aria-current={active ? 'page' : undefined}
            >
              <span className="nav-item-label">{navLabel(id)}</span>
              {showBadge && (
                <span
                  className="nav-item-badge"
                  title={t('nav.processingTitle', {
                    n: activePipelines,
                    plural: pipelinePlural,
                  })}
                  aria-label={
                    activePipelines === 1
                      ? t('nav.processingOne')
                      : t('nav.processingMany', { n: activePipelines })
                  }
                >
                  <span className="dot dot--accent dot--pulse" aria-hidden />
                  <span className="nav-item-badge-count">
                    {activePipelines}
                  </span>
                </span>
              )}
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
        <div className="app-rail-foot">
          v1.0.0<br />
          {t('nav.brandFooter')}
        </div>
      </aside>

      <main className="app-main">
        {/* [W3] Persistent recording strip. Renders null when idle, so layout
            is unchanged for non-recording sessions. */}
        <RecStrip />
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
        {/* [W3] Recording state is App-level so RecStrip + future RecFloat
            window + HomePage hotkey share one source of truth. */}
        <RecordingProvider>
          <AppShell />
        </RecordingProvider>
      </ThemeProvider>
    </I18nProvider>
  );
}
