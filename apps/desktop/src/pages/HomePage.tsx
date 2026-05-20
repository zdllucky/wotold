import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import { humanError } from '../api/errors';
import { useFocusTrap } from '../hooks/useFocusTrap';
import {
  getRecordingState,
  listCalls,
  startRecording,
  stopRecording,
  type Call,
  type RecordingState,
} from '../api/recording';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

interface HomePageProps {
  /** Опциональный колбэк навигации в детали звонка. Используется при
   *  «Open» после остановки записи + auto-redirect через 5 сек. */
  onOpenCall?: (callId: string) => void;
}

export function HomePage({ onOpenCall }: HomePageProps = {}) {
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);

  const [recording, setRecording] = useState<RecordingState | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastCall, setLastCall] = useState<Call | null>(null);
  const [elapsed, setElapsed] = useState(0);
  // #39 (C1): recording consent. Сохраняется один раз — при первом «Начать запись».
  const [consentAt, setConsentAt] = useState<string | null>(null);
  const [showConsent, setShowConsent] = useState(false);
  const consentRef = useRef<HTMLDivElement>(null);
  useFocusTrap(consentRef, showConsent, {
    onClose: () => setShowConsent(false),
  });
  // [B16] HomePage hero stats — последний звонок, число за неделю, 3 recent для quick-open.
  const [recentCalls, setRecentCalls] = useState<Call[]>([]);

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

  useEffect(() => {
    if (!recording) {
      setElapsed(0);
      return;
    }
    const startMs = new Date(recording.started_at).getTime();
    const tick = () => setElapsed(Math.max(0, Math.floor((Date.now() - startMs) / 1000)));
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
    try {
      const call = await stopRecording();
      setRecording(null);
      setLastCall(call);
    } catch (e) {
      setError(humanError(e));
      setRecording(null);
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

  // [B16 audit P1] Hotkey ⌘⇧R: start / stop запись без клика. Слушаем
  // на window — главный экран всегда в focus при desktop-app. Игнорируем
  // если activeElement — input/textarea (юзер пишет где-то).
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

  // [B16] Stats для hero — derived из recentCalls.
  const now = Date.now();
  const weekAgo = now - 7 * 24 * 60 * 60 * 1000;
  const callsThisWeek = recentCalls.filter((c) => {
    const t = new Date(c.started_at).getTime();
    return Number.isFinite(t) && t >= weekAgo;
  }).length;
  const lastReady = recentCalls.find((c) => c.status === 'ready') ?? recentCalls[0] ?? null;
  const recentForList = recentCalls.slice(0, 3);

  return (
    <section>
      <header style={{ marginBottom: 38 }}>
        <div className="eyebrow" style={{ marginBottom: 16 }}>
          {new Date().toLocaleDateString('ru-RU', {
            weekday: 'long',
            day: 'numeric',
            month: 'long',
          })}
        </div>
        <h1 className="display" style={{ marginBottom: 12 }}>
          {recording ? 'Записываю…' : 'Готов записывать.'}
        </h1>
        <p className="subtitle" style={{ maxWidth: 540 }}>
          Нажмите красный кружок когда начнёте звонок. Wotold расшифрует речь и пришлёт
          саммари — обычно через 10–30 секунд после остановки.
        </p>
      </header>

      <div style={{ display: 'flex', gap: 36, alignItems: 'center', marginBottom: 44 }}>
        {!recording && (
          <>
            <button
              type="button"
              className="rec-btn"
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
          </>
        )}
        {recording && (
          <>
            <button
              type="button"
              className="rec-btn rec-btn--stop"
              onClick={onStop}
              disabled={busy}
              aria-label="Остановить запись"
            />
            <div>
              <div
                className="small-caps"
                style={{ color: 'var(--signal)', marginBottom: 4 }}
              >
                ● Идёт запись
              </div>
              <div
                className="mono"
                style={{ fontSize: 36, fontWeight: 500, color: 'var(--ink)' }}
              >
                {formatElapsed(elapsed)}
              </div>
            </div>
          </>
        )}
      </div>

      {error && (
        <p
          style={{
            color: 'var(--signal)',
            marginBottom: 24,
            fontFamily: 'var(--font-sans)',
          }}
        >
          {error}
        </p>
      )}

      {lastCall && !recording && (
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
        <div className="stat-row" style={{ marginBottom: 36 }}>
          <div className="stat">
            <span className="stat-value">{recentCalls.length}</span>
            <span className="stat-label">Звонков · всего</span>
          </div>
          <div className="stat">
            <span className="stat-value">{callsThisWeek}</span>
            <span className="stat-label">За неделю</span>
          </div>
          {lastReady && onOpenCall && (
            <button
              type="button"
              className="stat"
              onClick={() => onOpenCall(lastReady.id)}
              style={{
                background: 'none',
                border: 'none',
                borderLeft: '1px solid var(--line)',
                cursor: 'pointer',
                textAlign: 'left',
              }}
              title="Открыть последний звонок"
            >
              <span className="stat-value" style={{ fontSize: 19 }}>
                {formatRelative(lastReady.started_at)}
              </span>
              <span className="stat-label">Последний</span>
            </button>
          )}
        </div>
      )}

      {recentForList.length > 0 && !recording && (
        <section>
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 16,
              marginBottom: 14,
            }}
          >
            <span className="small-caps">Недавние</span>
            <div style={{ flex: 1, height: 1, background: 'var(--line-soft)' }} />
            {onOpenCall && recentCalls.length > 3 && (
              <button
                type="button"
                className="btn btn--quiet"
                style={{ fontSize: 13 }}
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
                gridTemplateColumns: '120px 1fr auto',
                gap: 24,
                padding: '14px 0',
                width: '100%',
                background: 'none',
                // Reset all borders FIRST, then set borderTop — порядок важен:
                // shorthand `border` сбрасывает borderTopColor/width даже если
                // borderTop задан выше. (Code-review HIGH fix.)
                border: 'none',
                borderTop:
                  idx === 0 ? 'none' : '1px solid var(--line-soft)',
                textAlign: 'left',
                cursor: onOpenCall ? 'pointer' : 'default',
              }}
            >
              <div className="mono muted" style={{ fontSize: 12 }}>
                {formatRelative(c.started_at)}
              </div>
              <div>
                <div
                  style={{
                    fontFamily: 'var(--font-serif)',
                    fontSize: 16,
                    color: 'var(--ink)',
                    marginBottom: 2,
                  }}
                >
                  {c.title ?? 'Без названия'}
                </div>
              </div>
              <div
                className="mono muted"
                style={{ fontSize: 12, alignSelf: 'center' }}
              >
                {c.duration_sec ?? 0}c
              </div>
            </button>
          ))}
        </section>
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
              началом убедись, что собеседник предупреждён и согласен на запись. По закону
              РФ/РК запись переговоров без уведомления другой стороны может быть
              нарушением (статьи о неприкосновенности частной жизни / тайне коммуникаций).
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

function formatElapsed(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function formatRelative(iso: string): string {
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return iso;
  const diffSec = Math.max(0, Math.floor((Date.now() - t) / 1000));
  if (diffSec < 60) return 'только что';
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)} мин назад`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)} ч назад`;
  if (diffSec < 7 * 86400) return `${Math.floor(diffSec / 86400)} д назад`;
  return new Date(iso).toLocaleDateString('ru-RU', {
    day: '2-digit',
    month: 'short',
  });
}
