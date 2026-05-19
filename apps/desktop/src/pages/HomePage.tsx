import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import {
  getRecordingState,
  startRecording,
  stopRecording,
  type Call,
  type RecordingState,
} from '../api/recording';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

export function HomePage() {
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);

  const [recording, setRecording] = useState<RecordingState | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastCall, setLastCall] = useState<Call | null>(null);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    invoke<string>('get_device_id')
      .then(setDeviceId)
      .catch((e: unknown) => setError(String(e)));

    invoke<AvailableUpdate | null>('check_for_update')
      .then((u) => {
        if (u) setUpdate(u);
      })
      .catch((e: unknown) => console.warn('updater check failed', e));

    getRecordingState()
      .then(setRecording)
      .catch(() => {});
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

  const onStart = async () => {
    setBusy(true);
    setError(null);
    setLastCall(null);
    try {
      const call = await startRecording();
      setRecording({ call_id: call.id, started_at: call.started_at });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
      setError(String(e));
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
      setError(String(e));
    }
  };

  return (
    <>
      <h1>Wotold</h1>
      <p className="device-id">device: {deviceId ?? '…'}</p>

      <div className="record-area">
        {!recording && (
          <button
            type="button"
            className="record-start"
            onClick={onStart}
            disabled={busy}
          >
            <span className="record-dot" />
            {busy ? 'Запускаем…' : 'Начать запись'}
          </button>
        )}
        {recording && (
          <div className="record-active">
            <div className="record-indicator">
              <span className="record-dot pulse" />
              <span>Запись · {formatElapsed(elapsed)}</span>
            </div>
            <button
              type="button"
              className="record-stop"
              onClick={onStop}
              disabled={busy}
            >
              {busy ? 'Останавливаем…' : 'Стоп'}
            </button>
          </div>
        )}
        {error && <p className="error">{error}</p>}
        {lastCall && !recording && (
          <p className="record-saved">
            ✓ Звонок сохранён: <code>{lastCall.id.slice(0, 8)}…</code> ·{' '}
            {lastCall.duration_sec ?? 0} сек
          </p>
        )}
      </div>

      {update && (
        <aside className="update-prompt">
          <p>
            Доступна версия <strong>{update.version}</strong> (сейчас{' '}
            {update.current_version}).
          </p>
          {update.notes && <pre className="update-notes">{update.notes}</pre>}
          <div className="update-actions">
            <button type="button" onClick={applyUpdate} disabled={installing}>
              {installing ? 'Устанавливаем…' : 'Обновить сейчас'}
            </button>
            <button type="button" onClick={() => setUpdate(null)} disabled={installing}>
              Позже
            </button>
          </div>
        </aside>
      )}
    </>
  );
}

function formatElapsed(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}
