// [S5] In-app banner that listens to backend `recording:suggested` event
// (см. audio/call_detect.rs). Mount-conditional: returns null когда не было
// suggestion'а или юзер уже стартовал/dismiss'нул. Auto-dismiss через 30s.
//
// Поднимается параллельно с native macOS notification (S4) — нативка
// нужна для случая когда окно свернуто; banner — когда окно активно
// и юзер уже здесь.
//
// A11y:
//   - role="status" + aria-live="polite" — анонс без воровства фокуса.
//   - Buttons имеют явные aria-label'ы.

import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { useI18n } from '../i18n';

import { useRecording } from './RecordingContext';

interface SuggestPayload {
  bundle_id: string;
  app_name: string;
  reason: string;
}

const AUTO_DISMISS_MS = 30_000;

export function SuggestBanner() {
  const { t } = useI18n();
  const rec = useRecording();
  const [pending, setPending] = useState<SuggestPayload | null>(null);

  // ── Subscribe to backend event. Drop пока recording активен (probe и так
  //    подавит emit на backend, но это double-defence на случай гонок).
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    void (async () => {
      unlisten = await listen<SuggestPayload>(
        'recording:suggested',
        (event) => {
          if (rec.status.kind !== 'idle') return;
          setPending(event.payload);
        },
      );
    })();
    return () => {
      unlisten?.();
    };
    // status.kind берётся из rec.status напрямую при callback'е, поэтому
    // useEffect deps пуст — listener живёт всё время.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Auto-dismiss banner через 30s если юзер не нажал ничего.
  useEffect(() => {
    if (!pending) return;
    const id = window.setTimeout(() => setPending(null), AUTO_DISMISS_MS);
    return () => window.clearTimeout(id);
  }, [pending]);

  // ── Если recording стартовал (любым способом — hotkey / native notification
  //    / banner button) — снимаем banner.
  useEffect(() => {
    if (pending && rec.status.kind !== 'idle') {
      setPending(null);
    }
  }, [pending, rec.status.kind]);

  if (!pending) return null;

  const onStart = async () => {
    setPending(null);
    if (rec.busy || rec.status.kind !== 'idle') return;
    try {
      await rec.start();
    } catch {
      /* Ошибка surface'ит через rec.error; banner свою работу сделал. */
    }
  };

  const onDismiss = () => setPending(null);

  return (
    <div
      className="suggest-banner"
      role="status"
      aria-live="polite"
      data-testid="suggest-banner"
    >
      <div className="suggest-banner-body">
        <span className="suggest-banner-title">
          {t('recording.suggestTitle', { app: pending.app_name })}
        </span>
        <span className="suggest-banner-text">
          {t('recording.suggestBody')}
        </span>
      </div>
      <div className="suggest-banner-actions">
        <button
          type="button"
          className="btn btn--primary btn--sm"
          onClick={() => void onStart()}
          disabled={rec.busy}
        >
          {t('recording.suggestStart')}
        </button>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          onClick={onDismiss}
        >
          {t('recording.suggestDismiss')}
        </button>
      </div>
    </div>
  );
}
