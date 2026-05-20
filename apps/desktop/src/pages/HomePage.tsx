import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { invoke } from '@tauri-apps/api/core';

import {
  getRecordingState,
  startRecording,
  stopRecording,
  type Call,
  type RecordingState,
} from '../api/recording';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import { Button, Card, StatusDot } from '../ui';

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

  return (
    <section className="home">
      <header className="home-hero">
        <h1 className="home-title">Wotold</h1>
        <p className="home-subtitle text-muted">
          Жми «Запись» когда начнёшь звонок. Wotold расшифрует речь и
          пришлёт саммари — обычно через 10–30 секунд после остановки.
        </p>
      </header>

      <div className="record-area">
        {!recording && (
          <Button
            variant="record"
            size="lg"
            pill
            onClick={onStart}
            disabled={busy}
            busy={busy}
            leading={<StatusDot tone="neutral" size="lg" />}
          >
            {busy ? 'Запускаем…' : 'Начать запись'}
          </Button>
        )}
        {recording && (
          <div className="record-active">
            <div className="record-indicator">
              <StatusDot tone="danger" size="lg" pulse />
              <span>Запись · {formatElapsed(elapsed)}</span>
            </div>
            <Button
              variant="ghost"
              pill
              onClick={onStop}
              disabled={busy}
              busy={busy}
            >
              {busy ? 'Останавливаем…' : 'Стоп'}
            </Button>
          </div>
        )}
        {error && <p className="error">{error}</p>}
        {lastCall && !recording && (
          <div className="record-saved-card">
            <div>
              <strong>✓ Звонок сохранён</strong>
              <p className="text-muted" style={{ margin: 0, fontSize: 'var(--text-sm)' }}>
                Длительность: {lastCall.duration_sec ?? 0} сек. Распознавание
                идёт в фоне — обычно занимает 10–30 секунд.
              </p>
            </div>
            {onOpenCall && (
              <Button
                variant="primary"
                size="md"
                onClick={() => onOpenCall(lastCall.id)}
              >
                Открыть
              </Button>
            )}
          </div>
        )}
      </div>

      {showConsent && (
        <Card variant="raised" className="consent-card">
          <h3 className="consent-title">Согласие на запись</h3>
          <p>
            Wotold будет записывать звук с микрофона и системный аудиовыход. Перед началом
            убедись, что собеседник предупреждён и согласен на запись. По закону РФ/РК запись
            переговоров без уведомления другой стороны может быть нарушением (статьи о
            неприкосновенности частной жизни / тайне коммуникаций).
          </p>
          <p className="text-muted">
            Это окно появляется один раз. В дальнейшем будем доверять твоему решению.
          </p>
          <div className="form-actions">
            <Button variant="ghost" onClick={() => setShowConsent(false)} disabled={busy}>
              Отмена
            </Button>
            <Button variant="primary" onClick={onAcceptConsent} disabled={busy} busy={busy}>
              Согласен, начать
            </Button>
          </div>
        </Card>
      )}

      {update && (
        <Card className="update-prompt" variant="raised">
          <p>
            Доступна версия <strong>{update.version}</strong> (сейчас{' '}
            {update.current_version}).
          </p>
          {update.notes && <pre className="update-notes">{update.notes}</pre>}
          <div className="update-actions">
            <Button variant="primary" onClick={applyUpdate} disabled={installing} busy={installing}>
              {installing ? 'Устанавливаем…' : 'Обновить сейчас'}
            </Button>
            <Button variant="ghost" onClick={() => setUpdate(null)} disabled={installing}>
              Позже
            </Button>
          </div>
        </Card>
      )}
    </section>
  );
}

function formatElapsed(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}
