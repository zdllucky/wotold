// [B17] CallsPage — exact match per docs/design/atelier-v2/_reference/atelier-2.jsx §4.
// Document-style list:
//   - Header row: title 36, bottom-line search input flex, filter pills inline
//   - "94 звонка · 38 ч" small-caps subtotal
//   - Month groups: 120px sticky gutter (month) + flex calls
//   - Each call: grid 64/1fr/200/70 → date · title+preview · stacked avatars · duration
//   - Dotted dividers (1px dotted var(--line))
//
// Virtualization сохранена для 200+ списков (flat layout, без groups).

import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { listCalls, type Call } from '../api/recording';
import { listCallSpeakers } from '../api/speakers';
import { List, type RowComponentProps } from 'react-window';
import { CallRowSkeleton, Empty } from '../ui';

const VIRTUALIZATION_THRESHOLD = 200;
const ROW_HEIGHT = 78;
const VIRTUAL_LIST_HEIGHT = 600;

// SP palette inline — speaker thread colors per --sp-1..5 tokens.
const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

function declinePlural(n: number, forms: [string, string, string]): string {
  const abs = Math.abs(n) % 100;
  const tail = abs % 10;
  if (abs >= 11 && abs <= 14) return forms[2];
  if (tail === 1) return forms[0];
  if (tail >= 2 && tail <= 4) return forms[1];
  return forms[2];
}

type StatusFilter = 'all' | 'today' | 'week';

interface MonthGroup {
  label: string;
  calls: Call[];
}

function monthLabel(d: Date): string {
  return d.toLocaleDateString('ru-RU', { month: 'long', year: 'numeric' });
}

function groupByMonth(calls: Call[]): MonthGroup[] {
  const map = new Map<string, MonthGroup>();
  for (const c of calls) {
    const dt = new Date(c.started_at);
    if (!Number.isFinite(dt.getTime())) continue;
    const key = `${dt.getFullYear()}-${dt.getMonth()}`;
    let g = map.get(key);
    if (!g) {
      g = { label: capitalize(monthLabel(dt)), calls: [] };
      map.set(key, g);
    }
    g.calls.push(c);
  }
  // Map is insertion-ordered (calls already sorted desc by started_at).
  return Array.from(map.values());
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
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

function withinFilter(c: Call, f: StatusFilter): boolean {
  if (f === 'all') return true;
  const t = new Date(c.started_at).getTime();
  if (!Number.isFinite(t)) return false;
  const now = Date.now();
  if (f === 'today') {
    const startOfToday = new Date();
    startOfToday.setHours(0, 0, 0, 0);
    return t >= startOfToday.getTime();
  }
  if (f === 'week') {
    return t >= now - 7 * 24 * 3600 * 1000;
  }
  return true;
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
  const [filter, setFilter] = useState<StatusFilter>('all');
  // [B17] Aggregate: per-call confirmed speakers initials.
  const [speakerInitials, setSpeakerInitials] = useState<
    Map<string, string[]>
  >(new Map());

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

  // [B17] Aggregate confirmed speakers per call. One-shot после list, heavy
  // (N запросов на ready-звонки) — но кэш до full reload.
  useEffect(() => {
    if (!calls || calls.length === 0) return;
    void (async () => {
      const ready = calls.filter((c) => c.status === 'ready');
      const results = await Promise.allSettled(
        ready.map((c) => listCallSpeakers(c.id)),
      );
      const next = new Map<string, string[]>();
      results.forEach((r, i) => {
        if (r.status !== 'fulfilled') return;
        const callId = ready[i]!.id;
        const initialsList: string[] = [];
        for (const s of r.value) {
          if (s.confirmed && s.contact_display_name) {
            initialsList.push(initials(s.contact_display_name));
          }
        }
        next.set(callId, initialsList);
      });
      setSpeakerInitials(next);
    })();
  }, [calls]);

  if (error) {
    return (
      <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
        {error}
      </p>
    );
  }
  if (!calls) {
    return (
      <section>
        <div className="title" style={{ fontSize: 36, marginBottom: 20 }}>
          Звонки
        </div>
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
    .filter((c) => withinFilter(c, filter))
    .filter((c) => matchesQuery(c, query.trim()));

  const totalDurationSec = calls.reduce(
    (acc, c) => acc + (c.duration_sec ?? 0),
    0,
  );
  const totalHours = totalDurationSec / 3600;

  const FILTERS: Array<{ id: StatusFilter; label: string }> = [
    { id: 'all', label: 'Все' },
    { id: 'today', label: 'Сегодня' },
    { id: 'week', label: 'Неделя' },
  ];

  return (
    <section>
      {/* Header — title + bottom-line search + filter pills */}
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: 24,
          marginBottom: 26,
        }}
      >
        <div className="title" style={{ fontSize: 36 }}>
          Звонки
        </div>
        <div
          style={{
            flex: 1,
            borderBottom: '1px solid var(--line)',
            paddingBottom: 6,
          }}
        >
          <input
            className="input"
            placeholder="Найти в расшифровках…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Поиск звонков"
            style={{ borderBottom: 'none', fontSize: 15, padding: 0 }}
          />
        </div>
        <div style={{ display: 'flex', gap: 6 }}>
          {FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              className={`btn ${filter === f.id ? 'btn--ghost' : 'btn--quiet'}`}
              style={{ padding: '6px 12px', fontSize: 12 }}
              onClick={() => setFilter(f.id)}
              aria-pressed={filter === f.id}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {calls.length > 0 && (
        <div className="small-caps" style={{ marginBottom: 18 }}>
          {filtered.length !== calls.length
            ? `${filtered.length} из ${calls.length} ${declinePlural(calls.length, ['звонка', 'звонков', 'звонков'])}`
            : `${calls.length} ${declinePlural(calls.length, ['звонок', 'звонка', 'звонков'])}`}
          {totalDurationSec > 0 && ` · ${totalHours.toFixed(0)} ч`}
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
          rowProps={{ calls: filtered, onOpen, speakerInitials }}
          defaultHeight={VIRTUAL_LIST_HEIGHT}
        />
      ) : (
        <>
          {groupByMonth(filtered).map((g) => (
            <div key={g.label} style={{ marginBottom: 32 }}>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '120px 1fr',
                  gap: 32,
                }}
              >
                <div>
                  <div
                    className="small-caps"
                    style={{
                      paddingTop: 14,
                      position: 'sticky',
                      top: 0,
                    }}
                  >
                    {g.label}
                  </div>
                </div>
                <div>
                  {g.calls.map((c, idx) => (
                    <CallRow
                      key={c.id}
                      call={c}
                      onOpen={onOpen}
                      hasBorder={idx > 0}
                      speakers={speakerInitials.get(c.id)}
                    />
                  ))}
                </div>
              </div>
            </div>
          ))}
        </>
      )}
    </section>
  );
}

interface CallRowProps {
  call: Call;
  onOpen: (id: string) => void;
  hasBorder: boolean;
  /** Initials of confirmed speakers (computed parent-side). Falls back
   *  to deterministic hash placeholder if missing. */
  speakers?: string[];
}

function CallRow({ call, onOpen, hasBorder, speakers }: CallRowProps) {
  const list = speakers && speakers.length > 0 ? speakers : inferSpeakers(call);
  return (
    <button
      type="button"
      onClick={() => onOpen(call.id)}
      title={statusTooltip(call.status, call.failed_reason)}
      style={{
        display: 'grid',
        gridTemplateColumns: '64px 1fr 200px 70px',
        gap: 20,
        padding: '16px 0',
        width: '100%',
        background: 'none',
        border: 'none',
        borderTop: hasBorder ? '1px dotted var(--line)' : 'none',
        alignItems: 'baseline',
        textAlign: 'left',
        cursor: 'pointer',
        color: 'inherit',
      }}
    >
      <div
        className="mono muted"
        style={{
          fontSize: 11,
          letterSpacing: '0.04em',
          paddingTop: 4,
        }}
      >
        {formatDay(call.started_at)}
      </div>
      <div>
        <div
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 17,
            marginBottom: 4,
            letterSpacing: '-0.01em',
            color: 'var(--ink)',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexWrap: 'wrap',
          }}
        >
          {call.title ?? `Звонок ${call.id.slice(0, 8)}`}
          {call.status === 'processing' && (
            <span
              className="mono"
              style={{
                fontSize: 9,
                background: 'var(--accent-soft)',
                color: 'var(--accent)',
                padding: '2px 6px',
                borderRadius: 3,
                letterSpacing: '0.12em',
                textTransform: 'uppercase',
              }}
            >
              распознаём
            </span>
          )}
          {call.status === 'failed' && (
            <span
              className="mono"
              style={{
                fontSize: 9,
                background: 'var(--signal-soft)',
                color: 'var(--signal)',
                padding: '2px 6px',
                borderRadius: 3,
                letterSpacing: '0.12em',
                textTransform: 'uppercase',
              }}
            >
              ошибка
            </span>
          )}
          {call.status === 'recording' && (
            <span
              className="mono"
              style={{
                fontSize: 9,
                background: 'var(--signal-soft)',
                color: 'var(--signal)',
                padding: '2px 6px',
                borderRadius: 3,
                letterSpacing: '0.12em',
                textTransform: 'uppercase',
              }}
            >
              ● запись
            </span>
          )}
        </div>
        {call.failed_reason && call.status === 'failed' && (
          <div
            className="muted"
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontSize: 14,
              lineHeight: 1.4,
            }}
          >
            «{call.failed_reason}»
          </div>
        )}
      </div>
      <div
        style={{
          display: 'flex',
          gap: 4,
          flexWrap: 'wrap',
          alignItems: 'center',
        }}
      >
        {list.slice(0, 3).map((s, i) => (
          <span
            key={i}
            className="sp-avatar"
            style={{
              background: SP_COLORS[i % SP_COLORS.length],
              width: 24,
              height: 24,
              marginLeft: i === 0 ? 0 : -8,
              border: '2px solid var(--bg)',
              fontSize: 9,
            }}
          >
            {s}
          </span>
        ))}
        {list.length > 3 && (
          <span
            className="mono muted"
            style={{ fontSize: 11, marginLeft: 4 }}
          >
            +{list.length - 3}
          </span>
        )}
      </div>
      <div
        className="mono muted"
        style={{
          fontSize: 12,
          textAlign: 'right',
          letterSpacing: '0.04em',
        }}
      >
        {formatDuration(call.duration_sec)}
      </div>
    </button>
  );
}

interface VirtualRowProps {
  calls: Call[];
  onOpen: (id: string) => void;
  speakerInitials: Map<string, string[]>;
}

function VirtualCallRow({
  index,
  style,
  calls,
  onOpen,
  speakerInitials,
}: RowComponentProps<VirtualRowProps>) {
  const c = calls[index]!;
  return (
    <div style={style}>
      <CallRow
        call={c}
        onOpen={onOpen}
        hasBorder={index > 0}
        speakers={speakerInitials.get(c.id)}
      />
    </div>
  );
}

// [B17] Fallback if speakerInitials map не загружен (pending или failed).
// Deterministic hash gives stable placeholder per call id — без «прыжков»
// между перерендерами.
function inferSpeakers(call: Call): string[] {
  const sec = call.duration_sec ?? 0;
  const guess = sec < 300 ? 1 : sec < 1800 ? 2 : 3;
  const hash = [...call.id].reduce(
    (acc, ch) => (acc * 31 + ch.charCodeAt(0)) | 0,
    0,
  );
  const letters = 'АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЩЮЯ';
  const out: string[] = [];
  for (let i = 0; i < guess; i++) {
    const a = letters[Math.abs(hash + i * 7) % letters.length];
    const b = letters[Math.abs(hash + i * 13 + 5) % letters.length];
    out.push(`${a}${b}`);
  }
  return out;
}

function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
  );
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
    return date.getDate().toString().padStart(2, '0');
  } catch {
    return iso;
  }
}

function formatDuration(sec: number | null): string {
  if (sec == null) return '—';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}
