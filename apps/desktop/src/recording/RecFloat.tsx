// [W4] Floating recording pill rendered in the `recording-widget` Tauri
// window. 280×52 transparent, always-on-top, no decorations. Acts as a
// pocket-sized RecStrip:
//   - eq indicator + paused-state styling reuse `.rec-eq`/data-paused
//   - elapsed timer in signal red
//   - pause/resume + stop buttons (RecMiniButton)
//   - click anywhere outside the buttons → restore main window
//
// Stop is wired so that the main window receives the standard
// `pipeline:finished` signal (HomePage flow) — the widget asks Rust to
// restore the main window, the user's session continues on CallDetailPage.

import { invoke } from '@tauri-apps/api/core';
import type { MouseEvent as ReactMouseEvent } from 'react';

import { useI18n } from '../i18n';

import { RecEq } from './RecEq';
import { RecMiniButton } from './RecMiniButton';
import { formatElapsed, useRecording } from './RecordingContext';

const STOP_INTERACTIVE_SELECTOR = 'button, [data-rec-no-restore]';

async function restoreMain(): Promise<void> {
  try {
    await invoke('restore_main_window');
  } catch (e) {
    // Restore is best-effort — if it fails we still want the widget to hide
    // (it'll re-show on the next minimize). Surface to the console only.
    console.warn('restore_main_window failed', e);
  }
}

export function RecFloat() {
  const { t } = useI18n();
  const rec = useRecording();

  // Even when idle we still render an empty drag region so the user can move
  // the widget if it appears for a moment before status syncs. Real content is
  // gated by `kind !== 'idle'`.
  const isActive = rec.status.kind !== 'idle';
  const isPaused = rec.status.kind === 'paused';

  const onClickBody = (e: ReactMouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement | null;
    if (target?.closest(STOP_INTERACTIVE_SELECTOR)) return;
    void restoreMain();
  };

  const onTogglePause = () => {
    if (rec.busy) return;
    if (isPaused) void rec.resume();
    else void rec.pause();
  };

  const onStop = () => {
    if (rec.busy) return;
    void (async () => {
      try {
        await rec.stop();
      } catch (e) {
        // Surface via rec.error; the main window will pick this up too.
        console.warn('stop from widget failed', e);
      }
      // Whether stop succeeded or not, bring the main window back so the user
      // sees their session — and the floating widget tucks itself away.
      await restoreMain();
    })();
  };

  return (
    <div
      className="rec-float"
      role={isActive ? 'status' : 'presentation'}
      aria-live="polite"
      data-tauri-drag-region
      data-paused={isPaused ? 'true' : 'false'}
      data-active={isActive ? 'true' : 'false'}
      onClick={onClickBody}
    >
      <div className="rec-float-eq" data-tauri-drag-region>
        <RecEq paused={isPaused} />
      </div>
      <div className="rec-float-body" data-tauri-drag-region>
        <span className="rec-float-timer">
          {formatElapsed(rec.elapsedSec)}
        </span>
        <span className="rec-float-label">
          {isPaused
            ? t('recording.stripPaused')
            : t('recording.stripRecording')}
        </span>
      </div>
      <div className="rec-float-actions" data-rec-no-restore>
        <RecMiniButton
          variant={isPaused ? 'play' : 'pause'}
          onClick={onTogglePause}
          disabled={rec.busy || !isActive}
          ariaLabel={
            isPaused ? t('recording.resumeAction') : t('recording.pauseAction')
          }
        />
        <RecMiniButton
          variant="stop"
          onClick={onStop}
          disabled={rec.busy || !isActive}
          ariaLabel={t('recording.stopAction')}
        />
      </div>
    </div>
  );
}
