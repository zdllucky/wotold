import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { CallDetailPage } from './pages/CallDetailPage';
import { InboxView } from './pages/InboxView';
import { Coachmarks } from './pages/Coachmarks';
import { ContactsPage } from './pages/ContactsPage';
import { DesignSystemPage } from './pages/DesignSystemPage';
import { OnboardingPage } from './pages/OnboardingPage';
import { SettingsPage } from './pages/SettingsPage';
import { getSetting, setSetting, SETTINGS_KEYS } from './api/settings';
import { getActivePipelineCount } from './api/calls';
import { listCalls, type Call } from './api/recording';
import { humanError } from './api/errors';
import { localEngineGetActiveEngine } from './api/local-engine';
import type { EngineKind } from './components/EngineChip';
import { Sidebar, MiniRail, type RailView } from './components/AppSidebar';
import { CommandPalette } from './components/CommandPalette';
import { UpdateBanner } from './components/UpdateBanner';
import { useFocusTrap } from './hooks/useFocusTrap';
import { I18nProvider, useI18n } from './i18n';
import { RecordingProvider, useRecording } from './recording/RecordingContext';
import { RecStrip } from './recording/RecStrip';
import { SuggestBanner } from './recording/SuggestBanner';
import { ThemeProvider, useTheme } from './theme/useTheme';
import {
  DEFAULT_PAUSE_HOTKEY,
  DEFAULT_TOGGLE_HOTKEY,
  matchEvent,
  parseHotkey,
  type ParsedHotkey,
} from './utils/hotkey';

type Bootstrap = 'loading' | 'onboarding' | 'app';

const IS_DEV = import.meta.env.DEV;
const RAIL_MIN = 216;
const RAIL_MAX = 380;
const RAIL_DEFAULT = 256;
const RAIL_COLLAPSE_AT = 198;

function initialView(): RailView {
  if (typeof window === 'undefined') return 'inbox';
  const hash = window.location.hash.replace('#', '');
  // [B18.1a] 'home'/'calls' старого роутинга → 'inbox'.
  if (hash === 'contacts' || hash === 'settings') return hash;
  if (hash === 'ds' && IS_DEV) return 'ds';
  return 'inbox';
}

function readSavedRailW(): number {
  try {
    const v = parseInt(localStorage.getItem('wk-railw') ?? '', 10);
    return v >= RAIL_MIN && v <= RAIL_MAX ? v : RAIL_DEFAULT;
  } catch {
    return RAIL_DEFAULT;
  }
}

function AppShell() {
  const { t } = useI18n();
  const rec = useRecording();
  const { resolvedTheme, setTheme } = useTheme();

  const [bootstrap, setBootstrap] = useState<Bootstrap>('loading');
  const [view, setView] = useState<RailView>(initialView);
  const [detailCallId, setDetailCallId] = useState<string | null>(() => {
    if (typeof window === 'undefined') return null;
    return new URLSearchParams(window.location.search).get('detail');
  });
  const [activePipelines, setActivePipelines] = useState(0);
  const [activeEngine, setActiveEngine] = useState<EngineKind | null>(null);
  const [recent, setRecent] = useState<Call[]>([]);

  // [B18.1a] collapsible rail state.
  const [collapsed, setCollapsed] = useState(false);
  const [railW, setRailW] = useState<number>(readSavedRailW);
  const [paletteOpen, setPaletteOpen] = useState(false);

  // [B18.1a] recording consent (lifted from HomePage). C1/R2: persisted once on
  // first «Записать звонок».
  const [consentAt, setConsentAt] = useState<string | null>(null);
  const [showConsent, setShowConsent] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const consentRef = useRef<HTMLDivElement>(null);
  useFocusTrap(consentRef, showConsent, { onClose: () => setShowConsent(false) });

  // [B18.1a] configurable recording hotkeys (lifted from HomePage).
  const [toggleHotkey, setToggleHotkey] = useState<ParsedHotkey>(DEFAULT_TOGGLE_HOTKEY);
  const [pauseHotkey, setPauseHotkey] = useState<ParsedHotkey>(DEFAULT_PAUSE_HOTKEY);

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
    localEngineGetActiveEngine()
      .then(setActiveEngine)
      .catch(() => {
        /* graceful — engine may be unavailable on first run */
      });
  }, []);

  // Consent + hotkeys load.
  useEffect(() => {
    void getSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT)
      .then(setConsentAt)
      .catch((e: unknown) => console.warn('getSetting consent failed', e));
    void getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE).then((raw) => {
      const hk = parseHotkey(raw);
      if (hk) setToggleHotkey(hk);
    });
    void getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE).then((raw) => {
      const hk = parseHotkey(raw);
      if (hk) setPauseHotkey(hk);
    });
  }, []);

  // Recent calls for the rail. Refetch when a pipeline finishes (titles land).
  useEffect(() => {
    const load = () =>
      listCalls()
        .then((calls) => setRecent(calls.slice(0, 50)))
        .catch((e: unknown) => console.warn('listCalls (rail) failed', e));
    void load();
    let unlisten: UnlistenFn | null = null;
    void listen('pipeline:finished', () => void load())
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  // [V8.2/V9] Active-pipeline counter — full resync on every lifecycle event
  // (точен; реестр in-memory). НЕ менять на ±1 (дрейфовал).
  useEffect(() => {
    const resync = async () => {
      try {
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
        unlisteners.push(await listen('pipeline:started', () => void resync()));
        unlisteners.push(await listen('pipeline:finished', () => void resync()));
        unlisteners.push(await listen('pipeline:cancelled', () => void resync()));
      } catch (e) {
        console.warn('pipeline event listeners failed:', e);
      }
    };
    void attach();

    const onVisible = () => {
      if (document.visibilityState === 'visible') void resync();
    };
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      for (const u of unlisteners) u();
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, []);

  // Persist hash for back/forward + reopen.
  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.location.hash = view;
  }, [view]);

  // Persist rail width.
  useEffect(() => {
    try {
      localStorage.setItem('wk-railw', String(railW));
    } catch {
      /* ignore */
    }
  }, [railW]);

  // [Native UX] Block webview context menu globally (whitelist text surfaces).
  useEffect(() => {
    if (typeof document === 'undefined') return;
    if (import.meta.env.DEV) return; // right-click allowed in dev
    const ALLOW =
      'input, textarea, [contenteditable="true"], .markdown, .markdown *, .transcript-row, .transcript-row *, .transcript-text, .turn, .turn *, .turn-text, .title, .display, .subtitle, .doc-title, code, pre, kbd, [data-selectable], [data-selectable] *';
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(ALLOW)) return;
      e.preventDefault();
    };
    document.addEventListener('contextmenu', onContextMenu);
    return () => document.removeEventListener('contextmenu', onContextMenu);
  }, []);

  // [W4] Floating recording widget on main-window minimize while recording.
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const attach = async () => {
      try {
        unlisteners.push(
          await listen('main-window:minimized', () => {
            if (rec.status.kind === 'idle') return;
            void invoke('show_recording_widget').catch((e) => {
              console.warn('show_recording_widget failed', e);
            });
          }),
        );
        unlisteners.push(
          await listen('main-window:restored', () => {
            void invoke('hide_recording_widget').catch((e) => {
              console.warn('hide_recording_widget failed', e);
            });
          }),
        );
      } catch (e) {
        console.warn('main-window event listeners failed:', e);
      }
    };
    void attach();
    return () => {
      for (const u of unlisteners) u();
    };
  }, [rec.status.kind]);

  // ── Recording actions (lifted from HomePage, consent-gated). ──
  const startFlow = useCallback(async () => {
    setLocalError(null);
    try {
      await rec.start();
    } catch (e) {
      setLocalError(humanError(e));
    }
  }, [rec]);

  const onStart = useCallback(async () => {
    if (!consentAt) {
      setShowConsent(true);
      return;
    }
    await startFlow();
  }, [consentAt, startFlow]);

  const onAcceptConsent = useCallback(async () => {
    const ts = new Date().toISOString();
    try {
      await setSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT, ts);
      setConsentAt(ts);
      setShowConsent(false);
      await startFlow();
    } catch (e) {
      setLocalError(humanError(e));
    }
  }, [startFlow]);

  const onStop = useCallback(async () => {
    setLocalError(null);
    try {
      const result = await rec.stop();
      setDetailCallId(result.callId);
      setView('call');
    } catch (e) {
      setLocalError(humanError(e));
    }
  }, [rec]);

  // Rail record button + ⌘⇧R toggle: idle→start, recording→stop, paused→resume.
  const onRecordToggle = useCallback(() => {
    const kind = rec.status.kind;
    if (kind === 'idle') void onStart();
    else if (kind === 'recording') void onStop();
    else void rec.resume().catch(() => {});
  }, [rec, onStart, onStop]);

  const onPauseToggle = useCallback(() => {
    const kind = rec.status.kind;
    if (kind === 'recording') void rec.pause().catch(() => {});
    else if (kind === 'paused') void rec.resume().catch(() => {});
  }, [rec]);

  // Global hotkeys: toggle (⌘⇧R), pause (⌘⇧P), collapse rail (⌘\).
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // ⌘K palette — opens from anywhere (no input guard).
      if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
        e.preventDefault();
        setPaletteOpen((o) => !o);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
        e.preventDefault();
        setCollapsed((c) => !c);
        return;
      }
      const target = (e.target as HTMLElement | null)?.tagName?.toLowerCase();
      if (target === 'input' || target === 'textarea' || target === 'select') return;
      if (rec.busy) return;
      if (matchEvent(e, toggleHotkey)) {
        e.preventDefault();
        onRecordToggle();
        return;
      }
      if (matchEvent(e, pauseHotkey)) {
        e.preventDefault();
        onPauseToggle();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [rec.busy, toggleHotkey, pauseHotkey, onRecordToggle, onPauseToggle]);

  // ── Rail collapse / resize handlers. ──
  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const sx = e.clientX;
      const sw = railW;
      const move = (ev: MouseEvent) => {
        const w = sw + (ev.clientX - sx);
        if (w < RAIL_COLLAPSE_AT) {
          setCollapsed(true);
          end();
          return;
        }
        setRailW(Math.max(RAIL_MIN, Math.min(RAIL_MAX, w)));
      };
      const end = () => {
        document.removeEventListener('mousemove', move);
        document.removeEventListener('mouseup', end);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
      };
      document.addEventListener('mousemove', move);
      document.addEventListener('mouseup', end);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    },
    [railW],
  );

  const onExpandResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const sx = e.clientX;
    const move = (ev: MouseEvent) => {
      const w = 56 + (ev.clientX - sx);
      if (w > 150) {
        setCollapsed(false);
        setRailW(Math.max(RAIL_MIN, Math.min(RAIL_MAX, w)));
      } else {
        setCollapsed(true);
      }
    };
    const end = () => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', end);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', end);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  const onNav = useCallback((v: RailView) => {
    setView(v);
    setDetailCallId(null);
  }, []);
  const onOpenCall = useCallback((id: string) => {
    setDetailCallId(id);
    setView('call');
  }, []);
  const onToggleTheme = useCallback(() => {
    setTheme(resolvedTheme === 'dark' ? 'light' : 'dark');
  }, [resolvedTheme, setTheme]);

  if (bootstrap === 'loading') {
    return (
      <main className="app" style={{ alignItems: 'center', justifyContent: 'center' }}>
        <p className="u-muted">…</p>
      </main>
    );
  }

  if (bootstrap === 'onboarding') {
    return <OnboardingPage onComplete={() => setBootstrap('app')} />;
  }

  const railProps = {
    view,
    recKind: rec.status.kind,
    elapsed: rec.elapsedSec,
    busy: rec.busy,
    pipelineCount: activePipelines,
    recent,
    isDev: IS_DEV,
    resolvedTheme,
    onRecord: onRecordToggle,
    onPause: onPauseToggle,
    onNav,
    onOpenCall,
    onSearch: () => setPaletteOpen(true),
    onCollapse: () => setCollapsed(true),
    onExpand: () => setCollapsed(false),
    onToggleTheme,
    onResizeStart: collapsed ? onExpandResize : onResizeStart,
  } as const;

  return (
    <div
      className="app"
      data-collapsed={collapsed ? 'true' : undefined}
      style={{ ['--rail-w']: `${railW}px` } as CSSProperties}
    >
      {collapsed ? <MiniRail {...railProps} /> : <Sidebar {...railProps} />}

      <main className="app-main">
        {/* [B18.1b] RecStrip renders a fixed footer dock (.composer-dock) when
            recording — its position in the tree is irrelevant. */}
        <RecStrip activeEngine={activeEngine} collapsed={collapsed} />
        <SuggestBanner />
        <UpdateBanner />
        {localError && (
          <p
            role="alert"
            style={{ color: 'var(--danger)', margin: '0 0 14px', fontFamily: 'var(--font)' }}
          >
            {localError}
          </p>
        )}

        {view === 'inbox' && <InboxView onOpen={onOpenCall} />}
        {view === 'call' && detailCallId && (
          <CallDetailPage
            callId={detailCallId}
            onBack={() => {
              setDetailCallId(null);
              setView('inbox');
            }}
          />
        )}
        {view === 'contacts' && <ContactsPage onOpenCall={onOpenCall} />}
        {view === 'settings' && <SettingsPage />}
        {view === 'ds' && IS_DEV && <DesignSystemPage />}
      </main>

      {showConsent && (
        <div className="overlay">
          <div
            ref={consentRef}
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="consent-title"
            style={{ width: 460 }}
          >
            <div className="modal-body" style={{ paddingBottom: 4 }}>
              <div className="eyebrow" style={{ marginBottom: 10 }}>
                {t('home.consentEyebrow')}
              </div>
              <h3 id="consent-title" className="title" style={{ marginBottom: 14 }}>
                {t('home.consentTitle')}
              </h3>
              <p style={{ fontSize: 16, lineHeight: 1.55 }}>{t('home.consentBody')}</p>
              <p className="u-muted" style={{ marginTop: 8 }}>
                {t('home.consentSubnote')}
              </p>
            </div>
            <div className="modal-foot">
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => setShowConsent(false)}
                disabled={rec.busy}
              >
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void onAcceptConsent()}
                disabled={rec.busy}
              >
                {t('home.consentAccept')}
              </button>
            </div>
          </div>
        </div>
      )}

      {paletteOpen && (
        <CommandPalette
          onClose={() => setPaletteOpen(false)}
          onNav={(v) => {
            onNav(v);
            setPaletteOpen(false);
          }}
          onOpenCall={(id) => {
            onOpenCall(id);
            setPaletteOpen(false);
          }}
          onRecord={() => {
            onRecordToggle();
            setPaletteOpen(false);
          }}
          recent={recent}
        />
      )}

      <Coachmarks />
    </div>
  );
}

export function App() {
  return (
    <I18nProvider>
      <ThemeProvider>
        {/* [W3] Recording state is App-level so the rail, RecStrip, and the
            floating widget window share one source of truth. */}
        <RecordingProvider>
          <AppShell />
        </RecordingProvider>
      </ThemeProvider>
    </I18nProvider>
  );
}
