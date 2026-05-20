// [B17] CallsPage — Atelier v2 редизайн per docs/design/atelier-v2/MIGRATION.md §3.
// Date-grouped serif list, sticky bucket headers как small-caps gutter.
// Virtualization выше threshold сохранена — там grouping fallback на flat list.

import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { listCalls, type Call } from '../api/recording';
import { List, type RowComponentProps } from 'react-window';
import { CallRowSkeleton, Empty } from '../ui';

// [B16] Virtualization порог. Ниже — grouping headers (UX), выше — flat list.
const VIRTUALIZATION_THRESHOLD = 200;
const ROW_HEIGHT = 64;
const VIRTUAL_LIST_HEIGHT = 600;

function declinePlural(n: number, forms: [string, string, string]): string {
  const abs = Math.abs(n) % 100;
  const tail = abs % 10;
  if (abs >= 11 && abs <= 14) return forms[2];
  if (tail === 1) return forms[0];
  if (tail >= 2 && tail <= 4) return forms[1];
  return forms[2];
}

type StatusFilter = 'all' | 'recording' | 'processing' | 'ready' | 'failed';

type DateBucket = 'today' | 'yesterday' | 'this_week' | 'older';

function bucketFor(call: Call): DateBucket {
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const callDate = new Date(call.started_at);
  if (callDate >= startOfToday) return 'today';
  const startOfYesterday = new Date(startOfToday);
  startOfYesterday.setDate(startOfYesterday.getDate() - 1);
  if (callDate >= startOfYesterday) return 'yesterday';
  const startOfWeek = new Date(startOfToday);
  startOfWeek.setDate(startOfWeek.getDate() - 7);
  if (callDate >= startOfWeek) return 'this_week';
  return 'older';
}

function bucketLabel(b: DateBucket, sample?: Call): string {
  switch (b) {
    case 'today':
      return 'Сегодня';
    case 'yesterday':
      return 'Вчера';
    case 'this_week':
      return 'На этой неделе';
    case 'older':
      if (sample) {
        try {
          return new Date(sample.started_at).toLocaleDateString('ru-RU', {
            month: 'long',
            year: 'numeric',
          });
        } catch {
          /* fallthrough */
        }
      }
      return 'Раньше';
  }
}

function groupByBucket(calls: Call[]): Array<{ bucket: DateBucket; calls: Call[] }> {
  const order: DateBucket[] = ['today', 'yesterday', 'this_week', 'older'];
  const groups = new Map<DateBucket, Call[]>();
  for (const c of calls) {
    const b = bucketFor(c);
    const arr = groups.get(b) ?? [];
    arr.push(c);
    groups.set(b, arr);
  }
  return order
    .filter((b) => groups.has(b))
    .map((b) => ({ bucket: b, calls: groups.get(b)! }));
}

function matchesQuery(c: Call, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
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
    let unlisten: UnlistenFn | undefined;
    listen<PipelineFinishedEvent>('pipeline:finished', () => {
      refresh();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e: unknown) => {
        console.warn('pipeline event listener:', e);
      });
    return () => {
      unlisten?.();
    };
  }, []);

  if (error) {
    return (
      <p style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>{error}</p>
    );
  }
  if (!calls) {
    return (
      <section>
        <h1 className="title" style={{ fontSize: 36, marginBottom: 20 }}>
          Звонки
        </h1>
        <ul style={{ listStyle: 'none', padding: 0 }} aria-busy="true">
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

  const totalDurationSec = calls.reduce((acc, c) => acc + (c.duration_sec ?? 0), 0);
  const totalHours = (totalDurationSec / 3600).toFixed(1);

  const filterOptions: Array<{ id: StatusFilter; label: string }> = [
    { id: 'all', label: 'Все' },
    { id: 'ready', label: 'Готовые' },
    { id: 'processing', label: 'В работе' },
    { id: 'recording', label: 'Идёт' },
    { id: 'failed', label: 'Ошибки' },
  ];

  return (
    <section>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: 24,
          marginBottom: 18,
          flexWrap: 'wrap',
        }}
      >
        <h1 className="title" style={{ fontSize: 36, margin: 0 }}>
          Звонки
        </h1>
        <div style={{ flex: 1, minWidth: 200 }}>
          <input
            className="input"
            type="search"
            placeholder="Найти в звонках…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Поиск звонков"
          />
        </div>
      </div>

      {calls.length > 0 && (
        <div
          className="small-caps"
          style={{
            marginBottom: 24,
            display: 'flex',
            gap: 18,
            alignItems: 'baseline',
            flexWrap: 'wrap',
          }}
        >
          <span>
            {filtered.length !== calls.length
              ? `${filtered.length} из ${calls.length}`
              : `${calls.length} ${declinePlural(calls.length, ['звонок', 'звонка', 'звонков'])}`}
          </span>
          {totalDurationSec > 0 && <span>· {totalHours} ч</span>}
          <span style={{ flex: 1, minWidth: 16 }} />
          {filterOptions.map((opt) => (
            <button
              key={opt.id}
              type="button"
              onClick={() => setStatusFilter(opt.id)}
              className={`btn btn--sm ${statusFilter === opt.id ? 'btn--primary' : 'btn--quiet'}`}
              aria-pressed={statusFilter === opt.id}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}

      {calls.length === 0 ? (
        <Empty
          title="Звонков пока нет"
          description="Начни запись на «Главной» — звонок появится здесь сразу после остановки."
        />
      ) : filtered.length === 0 ? (
        <Empty
          title="Ничего не нашлось"
          description="Сбрось фильтры или измени запрос."
        />
      ) : filtered.length >= VIRTUALIZATION_THRESHOLD ? (
        <List
          rowComponent={VirtualCallRow}
          rowCount={filtered.length}
          rowHeight={ROW_HEIGHT}
          rowProps={{ calls: filtered, onOpen }}
          defaultHeight={VIRTUAL_LIST_HEIGHT}
        />
      ) : (
        <div>
          {groupByBucket(filtered).map(({ bucket, calls: bucketCalls }) => (
            <div key={bucket} style={{ marginBottom: 32 }}>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '140px 1fr',
                  gap: 32,
                }}
              >
                <div
                  className="small-caps"
                  style={{ paddingTop: 16, alignSelf: 'start' }}
                >
                  {bucketLabel(bucket, bucketCalls[0])}
                </div>
                <div>
                  {bucketCalls.map((c, idx) => (
                    <button
                      key={c.id}
                      type="button"
                      onClick={() => onOpen(c.id)}
                      title={statusTooltip(c.status, c.failed_reason)}
                      style={{
                        display: 'grid',
                        gridTemplateColumns: '64px 1fr 110px 60px',
                        gap: 18,
                        padding: '14px 0',
                        borderTop: idx === 0 ? 'none' : '1px solid var(--line-soft)',
                        alignItems: 'baseline',
                        width: '100%',
                        background: 'none',
                        border: idx === 0 ? 'none' : undefined,
                        borderTopStyle: idx === 0 ? 'none' : 'solid',
                        textAlign: 'left',
                        cursor: 'pointer',
                      }}
                    >
                      <div className="mono muted" style={{ fontSize: 12 }}>
                        {formatDay(c.started_at)}
                      </div>
                      <div>
                        <div
                          style={{
                            fontFamily: 'var(--font-serif)',
                            fontSize: 17,
                            color: 'var(--ink)',
                            marginBottom: 2,
                          }}
                        >
                          {c.title ?? `Звонок ${c.id.slice(0, 8)}`}
                        </div>
                        <div
                          className="small-caps"
                          style={{ fontSize: 10.5, marginTop: 4 }}
                        >
                          {statusBadge(c.status)}
                          {c.lang_detected ? ` · ${c.lang_detected.toUpperCase()}` : ''}
                          {c.provider ? ` · ${c.provider}` : ''}
                        </div>
                      </div>
                      <div className="mono muted" style={{ fontSize: 11 }}>
                        {formatStartedTime(c.started_at)}
                      </div>
                      <div
                        className="mono muted"
                        style={{ fontSize: 12, textAlign: 'right' }}
                      >
                        {formatDuration(c.duration_sec)}
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

interface VirtualRowProps {
  calls: Call[];
  onOpen: (id: string) => void;
}

function VirtualCallRow({ index, style, calls, onOpen }: RowComponentProps<VirtualRowProps>) {
  const c = calls[index]!;
  return (
    <div style={style}>
      <button
        type="button"
        onClick={() => onOpen(c.id)}
        title={statusTooltip(c.status, c.failed_reason)}
        style={{
          display: 'grid',
          gridTemplateColumns: '110px 1fr 110px 60px',
          gap: 18,
          padding: '14px 0',
          borderTop: '1px solid var(--line-soft)',
          alignItems: 'baseline',
          width: '100%',
          background: 'none',
          border: 'none',
          borderTopStyle: 'solid',
          textAlign: 'left',
          cursor: 'pointer',
        }}
      >
        <div className="mono muted" style={{ fontSize: 12 }}>
          {formatDay(c.started_at)}
        </div>
        <div>
          <div
            style={{
              fontFamily: 'var(--font-serif)',
              fontSize: 16,
              color: 'var(--ink)',
              marginBottom: 2,
            }}
          >
            {c.title ?? `Звонок ${c.id.slice(0, 8)}`}
          </div>
          <div className="small-caps" style={{ fontSize: 10.5, marginTop: 4 }}>
            {statusBadge(c.status)}
            {c.lang_detected ? ` · ${c.lang_detected.toUpperCase()}` : ''}
          </div>
        </div>
        <div className="mono muted" style={{ fontSize: 11 }}>
          {formatStartedTime(c.started_at)}
        </div>
        <div className="mono muted" style={{ fontSize: 12, textAlign: 'right' }}>
          {formatDuration(c.duration_sec)}
        </div>
      </button>
    </div>
  );
}

function statusBadge(status: string): string {
  switch (status) {
    case 'recording':
      return '● Идёт запись';
    case 'processing':
      return '~ Обрабатывается';
    case 'ready':
      return 'Готово';
    case 'failed':
      return '⚠ Ошибка';
    default:
      return status;
  }
}

function statusTooltip(status: string, failedReason: string | null): string {
  const base = (() => {
    switch (status) {
      case 'recording':
        return 'Идёт запись прямо сейчас.';
      case 'processing':
        return 'Запись завершена, идёт транскрипция через STT.';
      case 'ready':
        return 'Готово — есть transcript.md и raw_stt.json.';
      case 'failed':
        return 'Звонок не доведён до transcript. Аудио всё ещё на диске.';
      default:
        return status;
    }
  })();
  return status === 'failed' && failedReason ? `${base}\n\n${failedReason}` : base;
}

function formatDay(iso: string): string {
  try {
    const date = new Date(iso);
    return date.toLocaleDateString('ru-RU', {
      day: '2-digit',
      month: 'short',
    });
  } catch {
    return iso;
  }
}

function formatStartedTime(iso: string): string {
  try {
    const date = new Date(iso);
    return date.toLocaleTimeString('ru-RU', {
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
