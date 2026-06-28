// [B18.1b] Recording dock — Wotold v2. Was a top strip (.rec-strip); now a
// footer dock (.composer-dock + .composer--rec) per ~/Downloads/Wotold v2
// wk-app.jsx RecDock. Fixed to the bottom of the content area, offset by the
// rail width (rail-mini when collapsed). Mount-conditional: null when idle.
//
// Wiring preserved 1-to-1: useRecording status/elapsed/busy, audio-reactive
// RecEq (mic+system RMS), pause/resume + stop. New: «свернуть в виджет» (pip)
// minimises the main window → Rust emits main-window:minimized → floating
// widget window appears (App.tsx listener).
//
// A11y: role="status" + aria-live="polite"; explicit aria-labels on controls.

import { useEffect, useMemo, useState, type CSSProperties } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { listCalls, type Call } from '../api/recording';
import { useAudioLevel } from '../hooks/useAudioLevel';
import { useI18n } from '../i18n';
import { EngineChip, type EngineKind } from '../components/EngineChip';
import { Icon } from '../ui/Icon';

import { RecEq } from './RecEq';
import { RecMiniButton } from './RecMiniButton';
import { formatElapsed, useRecording } from './RecordingContext';

function activeCallId(
  status: ReturnType<typeof useRecording>['status'],
): string | null {
  if (status.kind === 'idle') return null;
  return status.callId;
}

interface RecStripProps {
  activeEngine?: EngineKind | null;
  /** Rail collapsed → dock offsets by rail-mini instead of rail-w. */
  collapsed?: boolean;
}

export function RecStrip({ activeEngine, collapsed = false }: RecStripProps = {}) {
  const { t } = useI18n();
  const rec = useRecording();
  const callId = activeCallId(rec.status);
  const isPaused = rec.status.kind === 'paused';
  const [title, setTitle] = useState<string | null>(null);

  // Best-effort title lookup (list_calls is cached; cheap). Decorative only.
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
        /* silent — title is decorative */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [callId]);

  const label = useMemo(
    () => (isPaused ? t('recording.stripPaused') : t('recording.stripRecording')),
    [isPaused, t],
  );

  // Live audio levels — max(mic, system) per index so bars react to either source.
  const isRecordingActive = rec.status.kind === 'recording';
  const audio = useAudioLevel(isRecordingActive);
  const levels = useMemo(
    () => audio.mic.map((m, i) => Math.max(m, audio.system[i] ?? 0)),
    [audio.mic, audio.system],
  );

  if (rec.status.kind === 'idle') return null;

  const onTogglePause = () => {
    if (rec.busy) return;
    if (isPaused) void rec.resume();
    else void rec.pause();
  };

  const onStop = () => {
    if (rec.busy) return;
    void rec.stop().catch(() => {
      /* error already surfaced via rec.error */
    });
  };

  const onMinimize = () => {
    // Minimise the main window; Rust emits main-window:minimized → floating
    // widget window is shown (App.tsx listener while recording).
    void getCurrentWindow()
      .minimize()
      .catch((e) => console.warn('minimize main window failed', e));
  };

  return (
    <div
      className="composer-dock"
      style={
        {
          position: 'fixed',
          left: collapsed ? 'var(--rail-mini)' : 'var(--rail-w)',
          right: 0,
          bottom: 0,
          zIndex: 40,
        } as CSSProperties
      }
    >
      <div
        className="composer composer--rec"
        role="status"
        aria-live="polite"
        data-paused={isPaused ? 'true' : 'false'}
        style={{ maxWidth: 'none' }}
      >
        <span
          className={isPaused ? 'dot' : 'dot dot--pulse'}
          style={{ background: 'var(--danger)' }}
          aria-hidden
        />
        <span style={{ color: 'var(--danger)', fontWeight: 600, fontSize: 13 }}>
          {label}
        </span>
        {activeEngine && <EngineChip kind={activeEngine} variant="recording" />}
        <span className="mono" style={{ fontSize: 16, fontWeight: 600 }}>
          {formatElapsed(rec.elapsedSec)}
        </span>
        {!isPaused && (
          <span className="mono" style={{ fontSize: 12, color: 'var(--text-faint)' }}>
            {t('recording.segment', { n: Math.floor(rec.elapsedSec / 600) + 1 })}
          </span>
        )}
        <RecEq paused={isPaused} levels={levels} />
        {title && (
          <span className="u-muted u-trunc" style={{ fontSize: 12, maxWidth: 220 }}>
            {title}
          </span>
        )}

        <div style={{ flex: 1 }} />

        <button
          type="button"
          className="iconbtn"
          onClick={onMinimize}
          aria-label={t('rail.collapse')}
          title={t('rail.collapse')}
        >
          <Icon name="pip" size={16} />
        </button>
        <RecMiniButton
          variant={isPaused ? 'play' : 'pause'}
          onClick={onTogglePause}
          disabled={rec.busy}
          ariaLabel={isPaused ? t('recording.resumeAction') : t('recording.pauseAction')}
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
