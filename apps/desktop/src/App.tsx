import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export function App() {
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>('get_device_id')
      .then(setDeviceId)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="app">
      <h1>Wotold</h1>
      <p className="device-id">device: {deviceId ?? '…'}</p>
      {error && <p className="error">{error}</p>}
      <p className="hint">Этап 1 каркас. Запись — Этап 2.</p>
    </main>
  );
}
