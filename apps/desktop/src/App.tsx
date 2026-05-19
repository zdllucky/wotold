import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

export function App() {
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    invoke<string>('get_device_id')
      .then(setDeviceId)
      .catch((e: unknown) => setError(String(e)));

    // M11.4: неблокирующая фоновая проверка апдейтов при старте.
    invoke<AvailableUpdate | null>('check_for_update')
      .then((u) => {
        if (u) setUpdate(u);
      })
      .catch((e: unknown) => {
        // Тихо — отсутствие сети не должно ломать запуск.
        console.warn('updater check failed', e);
      });
  }, []);

  const applyUpdate = async () => {
    setInstalling(true);
    try {
      await invoke('apply_update');
      // apply_update перезапустит процесс, сюда не доберёмся.
    } catch (e) {
      setInstalling(false);
      setError(String(e));
    }
  };

  return (
    <main className="app">
      <h1>Wotold</h1>
      <p className="device-id">device: {deviceId ?? '…'}</p>
      {error && <p className="error">{error}</p>}

      {update && (
        <aside className="update-prompt">
          <p>
            Доступна версия <strong>{update.version}</strong> (сейчас {update.current_version}).
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

      <p className="hint">Этап 1 каркас + Этап 11 авто-обновление. Запись — Этап 2.</p>
    </main>
  );
}
