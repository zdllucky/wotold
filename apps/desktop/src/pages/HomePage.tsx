// [W5] HomePage — Atelier v2. Single-surface idle hero. После W3 запись
// живёт в RecordingProvider, а strip над page content (RecStrip) показывает
// state универсально. HomePage больше не рендерит fullscreen recording
// overlay — только idle-страницу с большой кнопкой, hot-key hint, stats и
// списком недавних звонков. Stop приводит к navigate в CallDetailPage.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import { humanError } from '../api/errors';
import { localEngineGetActiveEngine } from '../api/local-engine';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  listCalls,
  RECORDING_DURATION_EVENT,
  type Call,
  type RecordingDurationEvent,
} from '../api/recording';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import { listCallSpeakers } from '../api/speakers';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { bcp47, useI18n } from '../i18n';
import { useRecording } from '../recording/RecordingContext';
import {
  DEFAULT_PAUSE_HOTKEY,
  DEFAULT_TOGGLE_HOTKEY,
  formatHotkey,
  matchEvent,
  parseHotkey,
  type ParsedHotkey,
} from '../utils/hotkey';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

interface HomePageProps {
  /** Опциональный колбэк навигации в детали звонка. */
  onOpenCall?: (callId: string) => void;
  /** [M12.7.5] Колбэк навигации в Settings → Engine для announcement banner. */
  onOpenSettings?: () => void;
}

export function HomePage({ onOpenCall, onOpenSettings }: HomePageProps = {}) {
  const { locale, t } = useI18n();
  const rec = useRecording();
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);

  // Local error отдельно от rec.error — для consent gate / updater. Recording
  // errors приходят через rec.error.
  const [localError, setLocalError] = useState<string | null>(null);
  // #39 (C1): recording consent — сохраняется один раз при первом «Начать запись».
  const [consentAt, setConsentAt] = useState<string | null>(null);
  const [showConsent, setShowConsent] = useState(false);
  const consentRef = useRef<HTMLDivElement>(null);
  useFocusTrap(consentRef, showConsent, { onClose: () => setShowConsent(false) });
  // [B16] Hero stats.
  const [recentCalls, setRecentCalls] = useState<Call[]>([]);
  // [B17] «Ждут подтверждения» — сумма неподтверждённых спикеров по всем
  // ready-звонкам. Делается one-shot после listCalls; дёшево на N≤50.
  const [pendingSpeakers, setPendingSpeakers] = useState(0);
  // [M12.7.5] Local engine announcement — показывается existing users
  // (≥1 ready call). [M12-v1.1] 7-day re-show + variant.
  const [showEngineAnnouncement, setShowEngineAnnouncement] = useState(false);
  type BannerVariant = 'default' | 'failures' | 'quota';
  const [bannerVariant, setBannerVariant] = useState<BannerVariant>('default');
  const [bannerFailureCount, setBannerFailureCount] = useState(0);

  // [P5.2] Live duration update во время active recording — patch'ит
  // call.duration_sec в recentCalls list на каждый sidecar `rotated`
  // event. Без этого HomePage показывал stale «1:56» для 30+ мин записей.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<RecordingDurationEvent>(RECORDING_DURATION_EVENT, (e) => {
      setRecentCalls((prev) =>
        prev.map((c) =>
          c.id === e.payload.call_id ? { ...c, duration_sec: e.payload.duration_sec } : c,
        ),
      );
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('recording:duration listener:', err));
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    invoke<AvailableUpdate | null>('check_for_update')
      .then((u) => {
        if (u) setUpdate(u);
      })
      .catch((e: unknown) => console.warn('updater check failed', e));

    getSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT)
      .then(setConsentAt)
      .catch((e: unknown) => console.warn('getSetting consent failed', e));

    listCalls()
      .then((calls) => {
        setRecentCalls(calls.slice(0, 50));
        // [M12.7.5/v1.1] Banner trigger: existing user (≥1 ready call) +
        // 7-day re-show logic.
        const hasReady = calls.some((c) => c.status === 'ready');
        if (hasReady) {
          void (async () => {
            try {
              const dismissedAt = await getSetting(SETTINGS_KEYS.LOCAL_ENGINE_ANNOUNCEMENT_DISMISSED_AT);
              const shouldShow =
                !dismissedAt ||
                Date.now() - new Date(dismissedAt).getTime() > 7 * 24 * 3600 * 1000;
              if (!shouldShow) return;
              // Don't show banner if already on local engine.
              const activeEngine = await localEngineGetActiveEngine().catch(() => null);
              if (activeEngine === 'local') return;
              // Determine variant: count failures in last 24h
              const cutoff = Date.now() - 24 * 3600 * 1000;
              const failCount = calls.filter(
                (c) =>
                  c.status === 'failed' &&
                  new Date(c.updated_at).getTime() > cutoff,
              ).length;
              if (failCount >= 3) {
                setBannerVariant('failures');
                setBannerFailureCount(failCount);
              } else {
                setBannerVariant('default');
              }
              setShowEngineAnnouncement(true);
            } catch {
              /* best-effort */
            }
          })();
        }
      })
      .catch((e: unknown) => console.warn('listCalls (home) failed', e));
  }, []);

  const dismissEngineAnnouncement = () => {
    setShowEngineAnnouncement(false);
    void setSetting(
      SETTINGS_KEYS.LOCAL_ENGINE_ANNOUNCEMENT_DISMISSED_AT,
      new Date().toISOString(),
    ).catch((e: unknown) => console.warn('persist announcement dismiss failed', e));
  };

  // [B17] Aggregate unconfirmed speakers across ready calls.
  useEffect(() => {
    const ready = recentCalls.filter((c) => c.status === 'ready').slice(0, 50);
    if (ready.length === 0) {
      setPendingSpeakers(0);
      return;
    }
    void (async () => {
      const results = await Promise.allSettled(
        ready.map((c) => listCallSpeakers(c.id)),
      );
      let count = 0;
      for (const r of results) {
        if (r.status === 'fulfilled') {
          count += r.value.filter((s) => !s.confirmed).length;
        }
      }
      setPendingSpeakers(count);
    })();
  }, [recentCalls]);

  const startRecordingFlow = async () => {
    setLocalError(null);
    try {
      await rec.start();
    } catch (e) {
      // rec.error уже выставлен; localError для UI на idle экране.
      setLocalError(humanError(e));
    }
  };

  const onStart = async () => {
    if (!consentAt) {
      setShowConsent(true);
      return;
    }
    await startRecordingFlow();
  };

  const onAcceptConsent = async () => {
    const ts = new Date().toISOString();
    try {
      await setSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT, ts);
      setConsentAt(ts);
      setShowConsent(false);
      await startRecordingFlow();
    } catch (e) {
      setLocalError(humanError(e));
    }
  };

  const onStop = async () => {
    setLocalError(null);
    try {
      const result = await rec.stop();
      if (onOpenCall) {
        onOpenCall(result.callId);
      }
    } catch (e) {
      setLocalError(humanError(e));
    }
  };

  const applyUpdate = async () => {
    setInstalling(true);
    try {
      await invoke('apply_update');
    } catch (e) {
      setInstalling(false);
      setLocalError(humanError(e));
    }
  };

  // [W1/W5] Configurable hotkeys — toggle (⌘⇧R) + pause (⌘⇧P).
  // toggle поведение:
  //   idle      → start
  //   recording → stop + navigate
  //   paused    → resume
  // pause поведение (только не-idle):
  //   recording → pause
  //   paused    → resume
  // [Bug-fix #3] toggleHotkey/pauseHotkey подняты в state — JSX рендерит
  // актуальный chord через formatHotkey(), а не хардкод "⌘ ⇧ R". Раньше
  // хоткеи жили локальными var'ами в useEffect → UI не обновлялся.
  const [toggleHotkey, setToggleHotkey] = useState<ParsedHotkey>(DEFAULT_TOGGLE_HOTKEY);
  const [pauseHotkey, setPauseHotkey] = useState<ParsedHotkey>(DEFAULT_PAUSE_HOTKEY);

  useEffect(() => {
    void getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE).then((raw) => {
      const fromDb = parseHotkey(raw);
      if (fromDb) setToggleHotkey(fromDb);
    });
    void getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE).then((raw) => {
      const fromDb = parseHotkey(raw);
      if (fromDb) setPauseHotkey(fromDb);
    });
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = (e.target as HTMLElement | null)?.tagName?.toLowerCase();
      if (target === 'input' || target === 'textarea' || target === 'select') return;
      if (rec.busy) return;

      if (matchEvent(e, toggleHotkey)) {
        e.preventDefault();
        const kind = rec.status.kind;
        if (kind === 'idle') {
          void onStart();
        } else if (kind === 'recording') {
          void onStop();
        } else {
          // paused → resume
          void rec.resume().catch(() => {
            /* error surfaced via rec.error */
          });
        }
        return;
      }

      if (matchEvent(e, pauseHotkey)) {
        e.preventDefault();
        const kind = rec.status.kind;
        if (kind === 'recording') {
          void rec.pause().catch(() => {});
        } else if (kind === 'paused') {
          void rec.resume().catch(() => {});
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rec.status.kind, rec.busy, consentAt, toggleHotkey, pauseHotkey]);

  // Stats derivation
  const now = Date.now();
  const weekAgo = now - 7 * 24 * 60 * 60 * 1000;
  const totalCount = recentCalls.length;
  const weekCount = recentCalls.filter((c) => {
    const ts = new Date(c.started_at).getTime();
    return Number.isFinite(ts) && ts >= weekAgo;
  }).length;
  const totalHours =
    recentCalls.reduce((acc, c) => acc + (c.duration_sec ?? 0), 0) / 3600;
  const recentForList = recentCalls.slice(0, 3);

  // Recording state — RecStrip уже видна над page content (см. App.tsx).
  // Hero меняет copy в зависимости от kind, большая красная кнопка скрыта
  // когда не idle (нельзя стартовать вторую запись).
  const kind = rec.status.kind;
  const isIdle = kind === 'idle';
  const isPaused = kind === 'paused';

  const headline = isIdle
    ? t('home.readyHeadline')
    : isPaused
      ? t('home.readyHeadlinePaused')
      : t('home.readyHeadlineRecording');
  const subtitle = isPaused
    ? t('home.subtitlePaused')
    : t('home.subtitle');

  const error = localError ?? rec.error;

  return (
    <section className="idle-enter">
      <div className="eyebrow" style={{ marginBottom: 18 }}>
        {formatLocaleDate(new Date(), locale)}
      </div>
      <div className="display" style={{ marginBottom: 12 }}>
        {headline}
      </div>
      <p className="subtitle" style={{ maxWidth: 540, marginBottom: 38 }}>
        {subtitle}
      </p>

      {showEngineAnnouncement && (() => {
        const variantClass =
          bannerVariant === 'failures'
            ? 'engine-announcement--failures'
            : bannerVariant === 'quota'
              ? 'engine-announcement--quota'
              : '';
        const eyebrow =
          bannerVariant === 'failures'
            ? t('home.engineAnnouncementFailures.eyebrow')
            : bannerVariant === 'quota'
              ? t('home.engineAnnouncementQuota.eyebrow')
              : t('home.engineAnnouncementDefault.eyebrow');
        const title =
          bannerVariant === 'failures'
            ? t('home.engineAnnouncementFailures.title')
            : bannerVariant === 'quota'
              ? t('home.engineAnnouncementQuota.title')
              : t('home.engineAnnouncementDefault.title');
        return (
          <div
            className={['engine-announcement', variantClass].filter(Boolean).join(' ')}
            role="region"
            aria-label={t('home.engineAnnouncementAria')}
          >
            <div>
              <p
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: 9.5,
                  textTransform: 'uppercase',
                  letterSpacing: '0.1em',
                  color: 'var(--ink-3)',
                  margin: '0 0 6px',
                }}
              >
                {eyebrow}
              </p>
              <p
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontSize: 17,
                  letterSpacing: '-0.01em',
                  margin: '0 0 8px',
                }}
              >
                {title}
              </p>
              <div className="engine-announcement-ba">
                {bannerVariant === 'default' && (
                  <>
                    <div className="engine-announcement-ba-item">
                      <span className="engine-announcement-ba-label">
                        {t('home.engineAnnouncementDefault.beforeLabel')}
                      </span>
                      <span className="engine-announcement-ba-value">
                        {t('home.engineAnnouncementDefault.beforeValue')}
                      </span>
                    </div>
                    <span className="engine-announcement-ba-arrow" aria-hidden="true">→</span>
                    <div className="engine-announcement-ba-item">
                      <span className="engine-announcement-ba-label">
                        {t('home.engineAnnouncementDefault.afterLabel')}
                      </span>
                      <span className="engine-announcement-ba-value">
                        {t('home.engineAnnouncementDefault.afterValue')}
                      </span>
                    </div>
                  </>
                )}
                {bannerVariant === 'failures' && (
                  <div className="engine-announcement-ba-item">
                    <span className="engine-announcement-ba-label">
                      {t('home.engineAnnouncementFailures.beforeLabel')}
                    </span>
                    <span className="engine-announcement-ba-value">
                      {t('home.engineAnnouncementFailures.beforeValue', {
                        count: String(bannerFailureCount),
                      })}
                    </span>
                  </div>
                )}
              </div>
              <div className="engine-announcement-actions">
                <button
                  type="button"
                  className="btn btn--primary btn--sm"
                  onClick={() => {
                    dismissEngineAnnouncement();
                    onOpenSettings?.();
                  }}
                >
                  {t('home.engineAnnouncementOpen')}
                </button>
                <button
                  type="button"
                  className="btn btn--quiet btn--sm"
                  onClick={dismissEngineAnnouncement}
                >
                  {t('home.engineAnnouncementDismiss')}
                </button>
              </div>
            </div>
          </div>
        );
      })()}

      {isIdle && (
        <div
          style={{
            display: 'flex',
            gap: 36,
            alignItems: 'center',
            marginBottom: 44,
          }}
        >
          <button
            type="button"
            className="rec-btn rec-btn--breathing"
            onClick={onStart}
            disabled={rec.busy}
            aria-label={rec.busy ? t('home.startingAria') : t('home.startAria')}
            title={t('home.hotkeyTitle', { chord: formatHotkey(toggleHotkey) })}
          />
          <div>
            <div className="small-caps" style={{ marginBottom: 4 }}>
              {formatHotkey(toggleHotkey)}
            </div>
            <div
              style={{
                fontFamily: 'var(--font-serif)',
                fontSize: 19,
                fontStyle: 'italic',
                color: 'var(--muted)',
                maxWidth: 260,
                lineHeight: 1.45,
              }}
            >
              {rec.busy ? t('home.starting') : t('home.hotkeyHint')}
            </div>
          </div>
        </div>
      )}

      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            marginBottom: 24,
            fontFamily: 'var(--font-sans)',
          }}
        >
          {error}
        </p>
      )}

      {recentCalls.length > 0 && (
        <div style={{ display: 'flex', marginBottom: 36 }}>
          <div className="stat">
            <span className="stat-value">{totalCount}</span>
            <span className="stat-label">{t('home.statTotal')}</span>
          </div>
          <div className="stat">
            <span className="stat-value">{weekCount}</span>
            <span className="stat-label">{t('home.statWeek')}</span>
          </div>
          <div className="stat">
            <span className="stat-value">
              {totalHours.toFixed(0)}
              <span style={{ fontSize: 18, marginLeft: 4 }}>{t('home.hoursAbbr')}</span>
            </span>
            <span className="stat-label">{t('home.statArchive')}</span>
          </div>
          {/* [B17] Always show 4 stats per artboard §2 — visual symmetry.
              При pendingSpeakers === 0 рендерим как muted '0'. */}
          <div className="stat">
            <span
              className="stat-value"
              style={{
                color: pendingSpeakers > 0 ? 'var(--accent)' : 'var(--ink)',
              }}
            >
              {pendingSpeakers}
            </span>
            <span className="stat-label">{t('home.statPending')}</span>
          </div>
        </div>
      )}

      {recentForList.length > 0 && (
        <div>
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 16,
              marginBottom: 14,
            }}
          >
            <span className="small-caps">{t('home.recentTitle')}</span>
            <div
              style={{ flex: 1, height: 1, background: 'var(--line-soft)' }}
            />
            {onOpenCall && recentCalls.length > 3 && (
              <button
                type="button"
                className="btn btn--quiet"
                style={{ padding: 0, fontSize: 13 }}
                onClick={() => onOpenCall(recentCalls[0]!.id)}
              >
                {t('home.allCalls')}
              </button>
            )}
          </div>
          {recentForList.map((c, idx) => (
            <button
              key={c.id}
              type="button"
              onClick={() => onOpenCall?.(c.id)}
              style={{
                display: 'grid',
                gridTemplateColumns: '100px 1fr auto',
                gap: 24,
                padding: '14px 0',
                width: '100%',
                background: 'none',
                border: 'none',
                borderTop: idx === 0 ? 'none' : '1px dotted var(--line)',
                textAlign: 'left',
                cursor: onOpenCall ? 'pointer' : 'default',
                color: 'inherit',
              }}
            >
              <div
                className="mono muted"
                style={{ fontSize: 12, letterSpacing: '0.04em' }}
              >
                {formatWhen(c.started_at, locale, t)}
              </div>
              <div>
                <div
                  style={{
                    fontFamily: 'var(--font-serif)',
                    fontSize: 16,
                    marginBottom: 4,
                    letterSpacing: '-0.01em',
                    color: 'var(--ink)',
                  }}
                >
                  {c.title ?? t('home.fallbackCallTitle', { short: c.id.slice(0, 8) })}
                </div>
                {c.failed_reason && (
                  <div
                    className="muted"
                    style={{
                      fontFamily: 'var(--font-serif)',
                      fontStyle: 'italic',
                      fontSize: 13,
                    }}
                  >
                    «{c.failed_reason}»
                  </div>
                )}
              </div>
              <div
                className="mono muted"
                style={{ fontSize: 12, alignSelf: 'center' }}
              >
                {formatDurationShort(c.duration_sec)}
              </div>
            </button>
          ))}
        </div>
      )}

      {showConsent && (
        <div
          ref={consentRef}
          className="modal-backdrop"
          role="dialog"
          aria-modal="true"
          aria-labelledby="consent-title"
        >
          <div className="index-card">
            <div className="eyebrow" style={{ marginBottom: 10 }}>
              {t('home.consentEyebrow')}
            </div>
            <h3
              id="consent-title"
              className="title"
              style={{ marginBottom: 14 }}
            >
              {t('home.consentTitle')}
            </h3>
            <p style={{ fontFamily: 'var(--font-serif)', fontSize: 16, lineHeight: 1.55 }}>
              {t('home.consentBody')}
            </p>
            <p className="muted" style={{ marginTop: 8 }}>
              {t('home.consentSubnote')}
            </p>
            <div style={{ display: 'flex', gap: 10, marginTop: 22 }}>
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
                onClick={onAcceptConsent}
                disabled={rec.busy}
              >
                {t('home.consentAccept')}
              </button>
            </div>
          </div>
        </div>
      )}

      {update && (
        <div className="card card--raised" style={{ marginTop: 28 }}>
          <p style={{ fontFamily: 'var(--font-sans)' }}>
            {t('home.updateAvailable', {
              version: update.version,
              current: update.current_version,
            })}
          </p>
          {update.notes && (
            <pre
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 12,
                color: 'var(--muted)',
                whiteSpace: 'pre-wrap',
                margin: '8px 0',
                padding: '8px 12px',
                background: 'var(--bg-2)',
                borderRadius: 6,
                maxHeight: '12rem',
                overflow: 'auto',
              }}
            >
              {update.notes}
            </pre>
          )}
          <div style={{ display: 'flex', gap: 10, marginTop: 8 }}>
            <button
              type="button"
              className="btn btn--primary"
              onClick={applyUpdate}
              disabled={installing}
            >
              {installing ? t('home.updateInstalling') : t('home.updateInstall')}
            </button>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setUpdate(null)}
              disabled={installing}
            >
              {t('common.later')}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

type TFn = ReturnType<typeof useI18n>['t'];

function formatLocaleDate(d: Date, locale: string): string {
  try {
    return d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
    });
  } catch {
    return d.toString();
  }
}

// "Сегодня · 11:24" / "Вчера · 16:02" / "15 мая · 10:00" — на ru/kk/en
// «Сегодня»/«Вчера» приходит через Intl.RelativeTimeFormat-free fallback,
// чтобы не зависеть от availability в jsdom.
function formatWhen(iso: string, locale: string, t: TFn): string {
  const date = new Date(iso);
  if (!Number.isFinite(date.getTime())) return iso;
  const bcp = bcp47(locale as Parameters<typeof bcp47>[0]);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfYesterday = new Date(startOfToday);
  startOfYesterday.setDate(startOfYesterday.getDate() - 1);
  const time = date.toLocaleTimeString(bcp, {
    hour: '2-digit',
    minute: '2-digit',
  });
  const todayLabel = locale === 'ru' ? 'Сегодня' : locale === 'kk' ? 'Бүгін' : 'Today';
  const yestLabel = locale === 'ru' ? 'Вчера' : locale === 'kk' ? 'Кеше' : 'Yesterday';
  // t-arg not used directly here — kept in signature so call sites stay
  // future-proof if we move labels into the dictionary.
  void t;
  if (date >= startOfToday) return `${todayLabel} · ${time}`;
  if (date >= startOfYesterday) return `${yestLabel} · ${time}`;
  const day = date.toLocaleDateString(bcp, {
    day: 'numeric',
    month: 'long',
  });
  return `${day} · ${time}`;
}

function formatDurationShort(sec: number | null): string {
  if (sec == null) return '—';
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}
