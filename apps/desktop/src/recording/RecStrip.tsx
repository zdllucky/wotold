// [W3] Persistent recording strip shown at the top of `app-main`.
//
// Mount-conditional: returns null when status.kind === 'idle'. When the user
// is recording or paused, RecStrip shows the equalizer indicator, a copy
// label, the elapsed timer, optional call title (best-effort lookup via
// list_calls), and pause/resume + stop controls.
//
// Click on the strip body itself is a no-op for now — W4 will wire it to
// "open the floating recording pane" (a second Tauri window).
//
// Accessibility:
//   - The wrapper carries `role="status"` + `aria-live="polite"` so screen
//     readers announce the state change without stealing focus.
//   - Buttons have explicit `aria-label`s (no icon text).

import { useEffect, useMemo, useState } from 'react';

import { listCalls, type Call } from '../api/recording';
import { useI18n } from '../i18n';

import { RecEq } from './RecEq';
import { RecMiniButton } from './RecMiniButton';
import { formatElapsed, useRecording } from './RecordingContext';

function activeCallId(
  status: ReturnType<typeof useRecording>['status'],
): string | null {
  if (status.kind === 'idle') return null;
  return status.callId;
}

export function RecStrip() {
  const { t } = useI18n();
  const rec = useRecording();
  const callId = activeCallId(rec.status);
  const isPaused = rec.status.kind === 'paused';
  const [title, setTitle] = useState<string | null>(null);

  // Best-effort title lookup. We avoid `get_call` because that triggers full
  // detail page load logic; `list_calls` is already cached on App mount and
  // is cheap. We only re-query if callId changes — title rarely changes during
  // a recording (LLM generates it later).
  useEffect(() => {
    if (!callId) {
      setTitle(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const calls = await listCalls();
        if (cancelled) return;
        const c = calls.find((x: Call) => x.id === callId) ?? null;
        setTitle(c?.title ?? null);
      } catch {
        // Silent — title is purely decorative on the strip.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [callId]);

  const label = useMemo(
    () =>
      isPaused ? t('recording.stripPaused') : t('recording.stripRecording'),
    [isPaused, t],
  );

  if (rec.status.kind === 'idle') return null;

  const onTogglePause = () => {
    if (rec.busy) return;
    if (isPaused) void rec.resume();
    else void rec.pause();
  };

  const onStop = () => {
    if (rec.busy) return;
    // Result is consumed by W5 (HomePage) — for now we just trigger the call.
    void rec.stop().catch(() => {
      /* error already surfaced via rec.error */
    });
  };

  return (
    <div
      className="rec-strip"
      role="status"
      aria-live="polite"
      data-paused={isPaused ? 'true' : 'false'}
    >
      <RecEq paused={isPaused} />

      <div className="rec-strip-meta">
        <div className="rec-strip-meta-row">
          <span className="rec-strip-label">{label}</span>
          <span className="rec-strip-timer">
            {formatElapsed(rec.elapsedSec)}
          </span>
        </div>
        {title && <span className="rec-strip-title">{title}</span>}
      </div>

      <div className="rec-strip-actions">
        <RecMiniButton
          variant={isPaused ? 'play' : 'pause'}
          onClick={onTogglePause}
          disabled={rec.busy}
          ariaLabel={
            isPaused
              ? t('recording.resumeAction')
              : t('recording.pauseAction')
          }
        />
        <RecMiniButton
          variant="stop"
          onClick={onStop}
          disabled={rec.busy}
          ariaLabel={t('recording.stopAction')}
        />
      </div>
    </div>
  );
}
