import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { listCalls, type Call } from '../api/recording';
import { Badge, CallRowSkeleton, Empty, InputField, SelectField, Toolbar } from '../ui';

type StatusFilter = 'all' | 'recording' | 'processing' | 'ready' | 'failed';

function matchesQuery(c: Call, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  // Поиск по title, провайдеру, lang, failed_reason, и первым 8 символам id.
  const haystack = [
    c.title ?? '',
    c.provider ?? '',
    c.lang_detected ?? '',
    c.failed_reason ?? '',
    c.id.slice(0, 8),
  ]
    .join(' ')
    .toLowerCase();
  return haystack.includes(needle);
}

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
  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');

  const refresh = () => {
    listCalls()
      .then(setCalls)
      .catch((e: unknown) => setError(humanError(e)));
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
  if (!calls) {
    return (
      <section className="calls">
        <Toolbar title="Звонки" />
        <ul className="calls-list" aria-busy="true">
          {Array.from({ length: 5 }, (_, i) => (
            <li key={i}>
              <CallRowSkeleton />
            </li>
          ))}
        </ul>
      </section>
    );
  }

  const filtered = calls
    .filter((c) => statusFilter === 'all' || c.status === statusFilter)
    .filter((c) => matchesQuery(c, query.trim()));

  return (
    <section className="calls">
      <Toolbar
        title="Звонки"
        actions={
          calls.length > 0 ? (
            <Badge tone="neutral">
              {filtered.length}
              {filtered.length !== calls.length ? ` / ${calls.length}` : ''}
            </Badge>
          ) : undefined
        }
      />
      {calls.length === 0 ? (
        <Empty
          title="Звонков пока нет"
          description="Начни запись с главной — сюда подтянется."
        />
      ) : (
        <>
          <div className="calls-filters">
            <InputField
              label=""
              type="search"
              placeholder="Поиск по названию звонка…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Поиск звонков"
            />
            <SelectField
              label=""
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
            >
              <option value="all">Все статусы</option>
              <option value="ready">Готовые</option>
              <option value="processing">В работе</option>
              <option value="recording">Идёт запись</option>
              <option value="failed">Ошибки</option>
            </SelectField>
          </div>
          {filtered.length === 0 ? (
            <Empty
              title="Ничего не нашлось"
              description="Сбрось фильтры или измени запрос."
            />
          ) : (
        <ul className="calls-list">
          {filtered.map((c) => (
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
                    {formatDuration(c.duration_sec)}
                    {c.lang_detected && ` · ${c.lang_detected.toUpperCase()}`}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
          )}
        </>
      )}
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
