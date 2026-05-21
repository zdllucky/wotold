// [W3] Recording state lifted to an App-level provider so any surface
// (HomePage, RecStrip, future RecFloat window) reads the same source of truth.
//
// The provider:
//  - reconstructs status on mount via `getRecordingState` (W2 backend);
//  - exposes `start/pause/resume/stop` that delegate to api/recording.ts;
//  - ticks `elapsedSec` every 250ms, freezing the value during pauses;
//  - surfaces backend errors via `error` for the UI to show as banner/toast.
//
// HomePage hotkey handler keeps its own copy of `recording` for now — W5 will
// migrate HomePage onto this hook. We intentionally do NOT subscribe to
// `pipeline:started/finished` here; those are a separate concern owned by
// AppShell's activity badge.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import {
  getRecordingState,
  pauseRecording,
  resumeRecording,
  startRecording,
  stopRecording,
  type RecordingState as BackendRecordingState,
} from '../api/recording';

export type RecordingStatus =
  | { kind: 'idle' }
  | {
      kind: 'recording';
      callId: string;
      startedAt: string;
      pausedTotalMs: number;
    }
  | {
      kind: 'paused';
      callId: string;
      startedAt: string;
      /** RFC3339, when the current pause window began. */
      pausedAt: string;
      /** Already-accumulated pause time BEFORE the current pause window. */
      pausedTotalMs: number;
    };

export interface RecordingApi {
  status: RecordingStatus;
  elapsedSec: number;
  busy: boolean;
  error: string | null;
  clearError: () => void;
  start: () => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  stop: () => Promise<{ callId: string }>;
}

const RecordingContext = createContext<RecordingApi | null>(null);

// 250ms tick — matches AudioScrubber cadence; visually smooth for the timer
// without spinning React every frame.
const TICK_MS = 250;

function statusFromBackend(
  state: BackendRecordingState | null,
): RecordingStatus {
  if (!state) return { kind: 'idle' };
  if (state.paused_at) {
    return {
      kind: 'paused',
      callId: state.call_id,
      startedAt: state.started_at,
      pausedAt: state.paused_at,
      pausedTotalMs: state.paused_total_ms,
    };
  }
  return {
    kind: 'recording',
    callId: state.call_id,
    startedAt: state.started_at,
    pausedTotalMs: state.paused_total_ms,
  };
}

function computeElapsedSec(status: RecordingStatus, nowMs: number): number {
  if (status.kind === 'idle') return 0;
  const startedMs = new Date(status.startedAt).getTime();
  if (Number.isNaN(startedMs)) return 0;

  if (status.kind === 'paused') {
    const pausedAtMs = new Date(status.pausedAt).getTime();
    // Elapsed freezes at the moment the user paused.
    const raw = pausedAtMs - startedMs - status.pausedTotalMs;
    return Math.max(0, Math.floor(raw / 1000));
  }

  // recording: subtract previously accumulated paused total.
  const raw = nowMs - startedMs - status.pausedTotalMs;
  return Math.max(0, Math.floor(raw / 1000));
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  try {
    return JSON.stringify(e);
  } catch {
    return 'Unknown error';
  }
}

interface RecordingProviderProps {
  children: ReactNode;
}

export function RecordingProvider({ children }: RecordingProviderProps) {
  const [status, setStatus] = useState<RecordingStatus>({ kind: 'idle' });
  const [elapsedSec, setElapsedSec] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Ref mirror of `status` so the tick callback never closes over a stale value
  // when start/stop fire within the same render frame.
  const statusRef = useRef<RecordingStatus>(status);
  statusRef.current = status;

  // ── Reconcile on mount. If the backend says we're already recording (e.g.
  //    page reload mid-call), reconstruct the provider's local state.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const state = await getRecordingState();
        if (cancelled) return;
        const next = statusFromBackend(state);
        setStatus(next);
        setElapsedSec(computeElapsedSec(next, Date.now()));
      } catch (e) {
        if (cancelled) return;
        // Non-fatal: stay idle, surface as recoverable error so UI can warn.
        setError(errorMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Tick. Recompute elapsed every TICK_MS. When paused, the formula above
  //    returns a constant so React skips a re-render via setState equality.
  useEffect(() => {
    if (statusRef.current.kind === 'idle') {
      setElapsedSec(0);
      return;
    }
    const id = window.setInterval(() => {
      const next = computeElapsedSec(statusRef.current, Date.now());
      setElapsedSec((prev) => (prev === next ? prev : next));
    }, TICK_MS);
    return () => window.clearInterval(id);
  }, [status.kind]);

  const start = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      const call = await startRecording();
      // startRecording returns a fully-populated Call; we only need the
      // started_at + id to seed status. paused_total_ms is 0 on a fresh start.
      setStatus({
        kind: 'recording',
        callId: call.id,
        startedAt: call.started_at,
        pausedTotalMs: 0,
      });
      setElapsedSec(0);
    } catch (e) {
      setError(errorMessage(e));
      throw e;
    } finally {
      setBusy(false);
    }
  }, []);

  const pause = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      const state = await pauseRecording();
      const next = statusFromBackend(state);
      setStatus(next);
      // Snap the displayed timer to the freeze-point synchronously so the UI
      // doesn't briefly show the still-incrementing recording value before
      // the next tick repaints with the paused formula.
      setElapsedSec(computeElapsedSec(next, Date.now()));
    } catch (e) {
      setError(errorMessage(e));
      throw e;
    } finally {
      setBusy(false);
    }
  }, []);

  const resume = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      const state = await resumeRecording();
      const next = statusFromBackend(state);
      setStatus(next);
      setElapsedSec(computeElapsedSec(next, Date.now()));
    } catch (e) {
      setError(errorMessage(e));
      throw e;
    } finally {
      setBusy(false);
    }
  }, []);

  const stop = useCallback(async (): Promise<{ callId: string }> => {
    setError(null);
    setBusy(true);
    try {
      const call = await stopRecording();
      setStatus({ kind: 'idle' });
      setElapsedSec(0);
      return { callId: call.id };
    } catch (e) {
      setError(errorMessage(e));
      throw e;
    } finally {
      setBusy(false);
    }
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const value = useMemo<RecordingApi>(
    () => ({
      status,
      elapsedSec,
      busy,
      error,
      clearError,
      start,
      pause,
      resume,
      stop,
    }),
    [status, elapsedSec, busy, error, clearError, start, pause, resume, stop],
  );

  return (
    <RecordingContext.Provider value={value}>
      {children}
    </RecordingContext.Provider>
  );
}

export function useRecording(): RecordingApi {
  const ctx = useContext(RecordingContext);
  if (!ctx) {
    throw new Error('useRecording must be inside RecordingProvider');
  }
  return ctx;
}

/** Format an elapsed time as `mm:ss` or `h:mm:ss` when ≥ 1h. */
export function formatElapsed(elapsedSec: number): string {
  const sec = Math.max(0, Math.floor(elapsedSec));
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  const pad = (n: number) => n.toString().padStart(2, '0');
  if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
  return `${pad(m)}:${pad(s)}`;
}
