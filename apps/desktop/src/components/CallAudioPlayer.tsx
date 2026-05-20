// [B16 UX P0] Audio player для CallDetailPage — критический для recording app.
// Использует Tauri asset protocol (convertFileSrc) для безопасного access
// к WAV-файлам внутри $APPDATA/calls/**/*.wav (scope в tauri.conf.json).

import { useEffect, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import { Button } from '../ui';
import { humanError } from '../api/errors';

interface Props {
  callId: string;
}

type Track = 'mic' | 'system';

export function CallAudioPlayer({ callId }: Props) {
  const [active, setActive] = useState<Track>('system');
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [missingMic, setMissingMic] = useState(false);
  const [missingSystem, setMissingSystem] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    setSrc(null);
    getCallAudioPath(callId, active)
      .then((path) => {
        if (!cancelled) {
          setSrc(convertFileSrc(path));
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        if (active === 'mic') setMissingMic(true);
        if (active === 'system') setMissingSystem(true);
        setError(humanError(e));
        setSrc(null);
      });
    return () => {
      cancelled = true;
    };
  }, [callId, active]);

  // Если active-track отсутствует — попробуем переключиться на другую.
  useEffect(() => {
    if (active === 'system' && missingSystem && !missingMic) setActive('mic');
    if (active === 'mic' && missingMic && !missingSystem) setActive('system');
  }, [active, missingMic, missingSystem]);

  if (missingMic && missingSystem) {
    return null;
  }

  return (
    <div
      className="card"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 14,
        marginBottom: 22,
        flexWrap: 'wrap',
      }}
    >
      <div
        role="group"
        aria-label="Дорожка"
        style={{ display: 'flex', gap: 6, flexShrink: 0 }}
      >
        <Button
          variant={active === 'system' ? 'primary' : 'ghost'}
          size="sm"
          onClick={() => setActive('system')}
          disabled={missingSystem}
          title="Звук собеседника (системный аудио)"
        >
          Собеседник
        </Button>
        <Button
          variant={active === 'mic' ? 'primary' : 'ghost'}
          size="sm"
          onClick={() => setActive('mic')}
          disabled={missingMic}
          title="Звук с твоего микрофона"
        >
          Я
        </Button>
      </div>
      {src ? (
        // eslint-disable-next-line jsx-a11y/media-has-caption
        <audio
          src={src}
          controls
          preload="metadata"
          style={{ flex: 1, minWidth: 240 }}
        />
      ) : (
        <p className="muted" style={{ margin: 0 }}>
          {error ? `Не удалось загрузить аудио: ${error}` : 'Загружаем…'}
        </p>
      )}
    </div>
  );
}
