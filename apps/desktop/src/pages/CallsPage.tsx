import { useEffect, useState } from 'react';

import { listCalls, type Call } from '../api/recording';

interface CallsPageProps {
  onOpen: (callId: string) => void;
}

export function CallsPage({ onOpen }: CallsPageProps) {
  const [calls, setCalls] = useState<Call[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listCalls()
      .then(setCalls)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!calls) return <p className="hint">Загрузка…</p>;

  return (
    <section className="calls-list">
      <h2>Звонки</h2>
      {calls.length === 0 ? (
        <p className="hint">Звонков пока нет. Начни запись с главной.</p>
      ) : (
        <ul>
          {calls.map((c) => (
            <li key={c.id} className={`call call-${c.status}`}>
              <button
                type="button"
                className="call-open"
                onClick={() => onOpen(c.id)}
                title="Открыть детали"
              >
                <div
                  className="call-status"
                  aria-label={c.status}
                  title={statusTooltip(c.status)}
                >
                  {statusIcon(c.status)}
                </div>
                <div className="call-meta">
                  <div className="call-when">{formatStarted(c.started_at)}</div>
                  <div className="call-detail">
                    {formatDuration(c.duration_sec)} · {c.path_label}
                    {c.provider && ` · ${c.provider}`}
                    {c.lang_detected && ` · ${c.lang_detected}`}
                  </div>
                </div>
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
