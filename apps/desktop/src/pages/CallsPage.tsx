import { useEffect, useState } from 'react';

import { listCalls, type Call } from '../api/recording';

export function CallsPage() {
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
              <div className="call-status" aria-label={c.status}>
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
            </li>
          ))}
        </ul>
      )}
      <p className="hint">
        FTS-поиск по транскрипту и детальный экран — после подключения транскрипции (#22, #31).
      </p>
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
