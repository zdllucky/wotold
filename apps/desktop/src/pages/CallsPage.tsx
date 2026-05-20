import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { listCalls, type Call } from '../api/recording';
import { Empty, Toolbar } from '../ui';

interface CallsPageProps {
  onOpen: (callId: string) => void;
}

interface PipelineFinishedEvent {
  call_id: string;
  status: 'ready' | 'failed';
  failed_reason: string | null;
}

export function CallsPage({ onOpen }: CallsPageProps) {
  const [calls, setCalls] = useState<Call[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    listCalls()
      .then(setCalls)
      .catch((e: unknown) => setError(String(e)));
  };

  useEffect(() => {
    refresh();
    // [B5]: Tauri pipeline → 'pipeline:finished' → авто-refresh без manual reload.
    let unlisten: UnlistenFn | undefined;
    listen<PipelineFinishedEvent>('pipeline:finished', () => {
      refresh();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e: unknown) => {
        // Listener fail в dev-browser (где @tauri-apps/api/event не работает) — игнор.
        console.warn('pipeline event listener:', e);
      });
    return () => {
      unlisten?.();
    };
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!calls) return <p className="hint">Загрузка…</p>;

  return (
    <section className="calls">
      <Toolbar title="Звонки" />
      {calls.length === 0 ? (
        <Empty
          title="Звонков пока нет"
          description="Начни запись с главной — сюда подтянется."
        />
      ) : (
        <ul className="calls-list">
          {calls.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                className="call-row"
                data-status={c.status}
                onClick={() => onOpen(c.id)}
                title="Открыть детали"
              >
                <span
                  className="call-status-cell"
                  aria-label={c.status}
                  title={
                    c.status === 'failed' && c.failed_reason
                      ? `${statusTooltip(c.status)}\n\n${c.failed_reason}`
                      : statusTooltip(c.status)
                  }
                >
                  {statusIcon(c.status)}
                </span>
                <span className="call-meta">
                  <span className="call-when">{formatStarted(c.started_at)}</span>
                  <span className="call-detail-line">
                    {formatDuration(c.duration_sec)} · {c.path_label}
                    {c.provider && ` · ${c.provider}`}
                    {c.lang_detected && ` · ${c.lang_detected}`}
                  </span>
                </span>
                <code className="call-id">{c.id.slice(0, 8)}</code>
              </button>
            </li>
          ))}
        </ul>
      )}
      <p className="hint">FTS-поиск по транскрипту — backlog (#30 follow-up).</p>
    </section>
  );
}

function statusIcon(status: string): string {
  switch (status) {
    case 'recording':
      return '⏺';
    case 'processing':
      return '⚙';
    case 'ready':
      return '✓';
    case 'failed':
      return '✗';
    default:
      return '·';
  }
}

function statusTooltip(status: string): string {
  switch (status) {
    case 'recording':
      return 'Идёт запись прямо сейчас.';
    case 'processing':
      return 'Запись завершена, идёт транскрипция через STT.';
    case 'ready':
      return 'Готово — есть transcript.md и raw_stt.json.';
    case 'failed':
      return 'Звонок не доведён до transcript (краш записи / ошибка STT / зависание из прошлой сессии). Аудио всё ещё на диске.';
    default:
      return status;
  }
}

function formatStarted(iso: string): string {
  try {
    const date = new Date(iso);
    return date.toLocaleString('ru-RU', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function formatDuration(sec: number | null): string {
  if (sec == null) return '—';
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}
