// [B17] HomePage — Atelier v2 exact match per docs/design/atelier-v2/_reference/
// atelier.jsx §2 (Home idle) + §3 (Recording active). Логика recording flow,
// consent, hotkey, updater сохранена 1-в-1 с предыдущей версии.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import { humanError } from '../api/errors';
import {
  getRecordingState,
  listCalls,
  startRecording,
  stopRecording,
  type Call,
  type RecordingState,
} from '../api/recording';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import { listCallSpeakers } from '../api/speakers';
import { DualWaveform } from '../components/DualWaveform';
import { useAudioLevel } from '../hooks/useAudioLevel';
import { useFocusTrap } from '../hooks/useFocusTrap';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

interface HomePageProps {
  /** Опциональный колбэк навигации в детали звонка. */
  onOpenCall?: (callId: string) => void;
}

export function HomePage({ onOpenCall }: HomePageProps = {}) {
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);

  const [recording, setRecording] = useState<RecordingState | null>(null);
  // [B17 V2.9] При остановке играем exit animation на overlay перед тем
  // как развалить state на idle layout. recordingExiting === true → overlay
  // ещё рендерится но с .recording-overlay--exiting класс; через 280мс
  // фактически setRecording(null) + переход в idle с .idle-enter.
  const [recordingExiting, setRecordingExiting] = useState(false);
  // [B14] Live levels из Swift sidecar (mic + system RMS @ 10Hz). Активен
  // только пока recording идёт.
  const audioLevels = useAudioLevel(recording !== null && !recordingExiting);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastCall, setLastCall] = useState<Call | null>(null);
  const [elapsed, setElapsed] = useState(0);
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

  useEffect(() => {
    invoke<AvailableUpdate | null>('check_for_update')
      .then((u) => {
        if (u) setUpdate(u);
      })
      .catch((e: unknown) => console.warn('updater check failed', e));

    getRecordingState()
      .then(setRecording)
      .catch((e: unknown) => console.warn('getRecordingState failed', e));

    getSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT)
      .then(setConsentAt)
      .catch((e: unknown) => console.warn('getSetting consent failed', e));

    listCalls()
      .then((calls) => setRecentCalls(calls.slice(0, 50)))
      .catch((e: unknown) => console.warn('listCalls (home) failed', e));
  }, []);

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

  useEffect(() => {
    if (!recording) {
      setElapsed(0);
      return;
    }
    const startMs = new Date(recording.started_at).getTime();
    const tick = () =>
      setElapsed(Math.max(0, Math.floor((Date.now() - startMs) / 1000)));
    tick();
    const id = window.setInterval(tick, 250);
    return () => window.clearInterval(id);
  }, [recording]);

  const startRecordingFlow = async () => {
    setBusy(true);
    setError(null);
    setLastCall(null);
    try {
      const call = await startRecording();
      setRecording({ call_id: call.id, started_at: call.started_at });
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
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
      setError(humanError(e));
    }
  };

  const onStop = async () => {
    setBusy(true);
    setError(null);
    // Trigger exit animation immediately for visual responsiveness, even
    // before backend completes. Если stopRecording fail — overlay уже
    // exiting но recording vill be reset anyway.
    setRecordingExiting(true);
    try {
      const call = await stopRecording();
      // Wait for exit animation to play before swapping layout.
      window.setTimeout(() => {
        setRecording(null);
        setRecordingExiting(false);
        setLastCall(call);
      }, 280);
    } catch (e) {
      setError(humanError(e));
      window.setTimeout(() => {
        setRecording(null);
        setRecordingExiting(false);
      }, 280);
    } finally {
      setBusy(false);
    }
  };

  const applyUpdate = async () => {
    setInstalling(true);
    try {
      await invoke('apply_update');
    } catch (e) {
      setInstalling(false);
      setError(humanError(e));
    }
  };

  // [B16] Hotkey ⌘⇧R: start/stop без клика.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isCmd = e.metaKey || e.ctrlKey;
      const target = (e.target as HTMLElement | null)?.tagName?.toLowerCase();
      if (target === 'input' || target === 'textarea' || target === 'select') return;
      if (
        isCmd &&
        e.shiftKey &&
        (e.key === 'r' || e.key === 'R' || e.key === 'к' || e.key === 'К')
      ) {
        e.preventDefault();
        if (busy) return;
        if (recording) {
          void onStop();
        } else {
          void onStart();
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording, busy, consentAt]);

  // Stats derivation
  const now = Date.now();
  const weekAgo = now - 7 * 24 * 60 * 60 * 1000;
  const totalCount = recentCalls.length;
  const weekCount = recentCalls.filter((c) => {
    const t = new Date(c.started_at).getTime();
    return Number.isFinite(t) && t >= weekAgo;
  }).length;
  const totalHours =
    recentCalls.reduce((acc, c) => acc + (c.duration_sec ?? 0), 0) / 3600;
  const recentForList = recentCalls.slice(0, 3);

  // ── RECORDING STATE — full-screen overlay per artboard §3 (no nav rail)
  if (recording) {
    return (
      <section
        className={`recording-overlay${recordingExiting ? ' recording-overlay--exiting' : ''}`}
        style={{
          position: 'fixed',
          inset: 0,
          zIndex: 50,
          background: 'var(--paper)',
          padding: '40px 56px',
          display: 'flex',
          flexDirection: 'column',
          gap: 32,
          overflowY: 'auto',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div>
            <div
              className="eyebrow"
              style={{ marginBottom: 10, color: 'var(--signal)' }}
            >
              ● Идёт запись · Локально
            </div>
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 92,
                fontWeight: 400,
                letterSpacing: '0.02em',
                color: 'var(--ink)',
                lineHeight: 1,
              }}
            >
              {(() => {
                const h = Math.floor(elapsed / 3600);
                const m = Math.floor((elapsed % 3600) / 60);
                const s = elapsed % 60;
                const hm = `${h.toString().padStart(2, '0')}:${m
                  .toString()
                  .padStart(2, '0')}`;
                const ss = s.toString().padStart(2, '0');
                return (
                  <>
                    {hm}
                    <span style={{ color: 'var(--signal)' }}>:{ss}</span>
                  </>
                );
              })()}
            </div>
          </div>
          <button
            type="button"
            className="rec-btn rec-btn--stop"
            onClick={onStop}
            disabled={busy}
            aria-label="Остановить запись"
          />
        </div>

        {error && (
          <p role="alert" style={{ color: 'var(--signal)' }}>
            {error}
          </p>
        )}

        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            gap: 18,
            justifyContent: 'center',
            minHeight: 280,
          }}
        >
          {/* [B17 V3.0] Объединённая stereo-split waveform — визуально одна
              дорожка, под капотом два канала. Mic вверх от центра (ink),
              System вниз (accent). Тишина = flat линия (никаких суррогатов). */}
          <div style={{ position: 'relative', minHeight: 220 }}>
            <DualWaveform mic={audioLevels.mic} system={audioLevels.system} />
          </div>

          {/* dB labels справа column для обоих каналов. */}
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              gap: 24,
              fontSize: 11,
            }}
          >
            <ChannelLabel
              label="Вы · микрофон"
              colorVar="var(--ink)"
              db={
                audioLevels.connected
                  ? formatDb(audioLevels.mic[audioLevels.mic.length - 1] ?? 0)
                  : '—'
              }
              connected={audioLevels.connected}
            />
            <ChannelLabel
              label="Собеседник · системный звук"
              colorVar="var(--accent)"
              db={
                audioLevels.connected
                  ? formatDb(
                      audioLevels.system[audioLevels.system.length - 1] ?? 0,
                    )
                  : '—'
              }
              connected={audioLevels.connected}
            />
          </div>
        </div>

        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            borderTop: '1px solid var(--line-soft)',
            paddingTop: 16,
          }}
        >
          <div
            className="mono muted"
            style={{ fontSize: 11, letterSpacing: '0.06em' }}
          >
            16 кГц моно · WAV · {formatHMS(elapsed, true)}
          </div>
          <div
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              color: 'var(--muted)',
              fontSize: 13,
            }}
          >
            Расшифровка начнётся автоматически
          </div>
        </div>
      </section>
    );
  }

  // ── IDLE STATE — home per artboard §2
  // [B17 V2.9] `idle-enter` triggers staggered fade-in анимацию children
  // (см. wotold.css). При первом mount + после остановки записи играет.
  return (
    <section className="idle-enter">
      <div className="eyebrow" style={{ marginBottom: 18 }}>
        {formatRuDate(new Date())}
      </div>
      <div className="display" style={{ marginBottom: 12 }}>
        Готов записывать.
      </div>
      <p className="subtitle" style={{ maxWidth: 540, marginBottom: 38 }}>
        Нажмите красный кружок когда начнёте звонок. Расшифровка приходит через
        10–30 секунд.
      </p>

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
          disabled={busy}
          aria-label={busy ? 'Запускаем' : 'Начать запись'}
          title="Горячая клавиша: ⌘⇧R"
        />
        <div>
          <div className="small-caps" style={{ marginBottom: 4 }}>
            ⌘ ⇧ R
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
            {busy ? 'Запускаем…' : 'Или просто нажмите горячую клавишу'}
          </div>
        </div>
      </div>

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

      {lastCall && (
        <div
          className="card"
          style={{ marginBottom: 28, display: 'flex', gap: 16, alignItems: 'center' }}
        >
          <div style={{ flex: 1 }}>
            <strong style={{ fontFamily: 'var(--font-sans)' }}>✓ Звонок сохранён</strong>
            <p className="muted" style={{ margin: '4px 0 0', fontSize: 13 }}>
              Длительность: {lastCall.duration_sec ?? 0} сек. Распознавание идёт в фоне —
              обычно занимает 10–30 секунд.
            </p>
          </div>
          {onOpenCall && (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => onOpenCall(lastCall.id)}
            >
              Открыть
            </button>
          )}
        </div>
      )}

      {recentCalls.length > 0 && (
        <div style={{ display: 'flex', marginBottom: 36 }}>
          <div className="stat">
            <span className="stat-value">{totalCount}</span>
            <span className="stat-label">Звонков · всего</span>
          </div>
          <div className="stat">
            <span className="stat-value">{weekCount}</span>
            <span className="stat-label">За неделю</span>
          </div>
          <div className="stat">
            <span className="stat-value">
              {totalHours.toFixed(0)}
              <span style={{ fontSize: 18, marginLeft: 4 }}>ч</span>
            </span>
            <span className="stat-label">В архиве</span>
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
            <span className="stat-label">Ждут подтверждения</span>
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
            <span className="small-caps">Недавно</span>
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
                Все звонки →
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
                {formatWhen(c.started_at)}
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
                  {c.title ?? `Звонок ${c.id.slice(0, 8)}`}
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
              Согласие на запись
            </div>
            <h3
              id="consent-title"
              className="title"
              style={{ marginBottom: 14 }}
            >
              Перед стартом
            </h3>
            <p style={{ fontFamily: 'var(--font-serif)', fontSize: 16, lineHeight: 1.55 }}>
              Wotold будет записывать звук с микрофона и системный аудиовыход. Перед
              началом убедись, что собеседник предупреждён и согласен на запись. По
              закону РФ/РК запись переговоров без уведомления другой стороны может
              быть нарушением.
            </p>
            <p className="muted" style={{ marginTop: 8 }}>
              Это окно появляется один раз. В дальнейшем будем доверять твоему решению.
            </p>
            <div style={{ display: 'flex', gap: 10, marginTop: 22 }}>
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => setShowConsent(false)}
                disabled={busy}
              >
                Отмена
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={onAcceptConsent}
                disabled={busy}
              >
                Согласен, начать
              </button>
            </div>
          </div>
        </div>
      )}

      {update && (
        <div className="card card--raised" style={{ marginTop: 28 }}>
          <p style={{ fontFamily: 'var(--font-sans)' }}>
            Доступна версия <strong>{update.version}</strong> (сейчас{' '}
            {update.current_version}).
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
              {installing ? 'Устанавливаем…' : 'Обновить сейчас'}
            </button>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setUpdate(null)}
              disabled={installing}
            >
              Позже
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function formatHMS(sec: number, padHours = false): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  const mm = m.toString().padStart(2, '0');
  const ss = s.toString().padStart(2, '0');
  if (h > 0 || padHours) return `${h.toString().padStart(2, '0')}:${mm}:${ss}`;
  return `${m}:${ss}`;
}

// [B14] RMS (0..1) → dBFS approximation. -∞ → "−∞", clamp -60 на min.
// 20·log10(rms). Real signal at -12 dB ≈ rms 0.25, -3 dB ≈ rms 0.71.
function formatDb(rms: number): string {
  if (rms <= 1e-6) return '−∞ dB';
  const db = 20 * Math.log10(rms);
  const clamped = Math.max(-60, Math.min(0, db));
  return `${clamped.toFixed(0)} dB`;
}

// Channel label aside DualWaveform — coloured dot + small-caps name + mono dB.
function ChannelLabel({
  label,
  colorVar,
  db,
  connected,
}: {
  label: string;
  colorVar: string;
  db: string;
  connected: boolean;
}) {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
        flex: 1,
        minWidth: 0,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span
          className="dot"
          style={{ background: colorVar, width: 8, height: 8, flexShrink: 0 }}
          aria-hidden
        />
        <span
          className="small-caps"
          style={{
            fontSize: 10,
            whiteSpace: 'nowrap',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
          }}
        >
          {label}
        </span>
      </div>
      <div
        className="mono"
        style={{
          fontSize: 13,
          letterSpacing: '0.04em',
          color: connected ? 'var(--ink)' : 'var(--subtle)',
        }}
      >
        {db}
      </div>
    </div>
  );
}

function formatRuDate(d: Date): string {
  try {
    return d.toLocaleDateString('ru-RU', {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
    });
  } catch {
    return d.toString();
  }
}

// "Сегодня · 11:24" / "Вчера · 16:02" / "15 мая · 10:00"
function formatWhen(iso: string): string {
  const date = new Date(iso);
  if (!Number.isFinite(date.getTime())) return iso;
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfYesterday = new Date(startOfToday);
  startOfYesterday.setDate(startOfYesterday.getDate() - 1);
  const time = date.toLocaleTimeString('ru-RU', {
    hour: '2-digit',
    minute: '2-digit',
  });
  if (date >= startOfToday) return `Сегодня · ${time}`;
  if (date >= startOfYesterday) return `Вчера · ${time}`;
  const day = date.toLocaleDateString('ru-RU', {
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
