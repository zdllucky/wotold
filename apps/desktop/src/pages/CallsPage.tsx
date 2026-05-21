// [B17] CallsPage — exact match per docs/design/atelier-v2/_reference/atelier-2.jsx §4.
// Document-style list:
//   - Header row: title 36, bottom-line search input flex, filter pills inline
//   - "94 звонка · 38 ч" small-caps subtotal
//   - Month groups: 120px sticky gutter (month) + flex calls
//   - Each call: grid 64/1fr/200/70 → date · title+preview · stacked avatars · duration
//   - Dotted dividers (1px dotted var(--line))
//
// Virtualization сохранена для 200+ списков (flat layout, без groups).

import { useEffect, useState, type ReactNode } from 'react';
import { humanError } from '../api/errors';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  listCalls,
  type Call,
  type CallProgressEvent,
} from '../api/recording';
import { listCallSpeakers } from '../api/speakers';
import { List, type RowComponentProps } from 'react-window';
import { CallRowSkeleton, Empty } from '../ui';
import { bcp47, useI18n } from '../i18n';
import { CallStateTag, ProgressRail } from '../components/call-state';
import { PIPELINE_STEP_KEYS, type CallState } from '../types/callState';

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

function monthLabel(d: Date, locale: string): string {
  return d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
    month: 'long',
    year: 'numeric',
  });
}

function groupByMonth(calls: Call[], locale: string): MonthGroup[] {
  const map = new Map<string, MonthGroup>();
  for (const c of calls) {
    const dt = new Date(c.started_at);
    if (!Number.isFinite(dt.getTime())) continue;
    const key = `${dt.getFullYear()}-${dt.getMonth()}`;
    let g = map.get(key);
    if (!g) {
      g = { label: capitalize(monthLabel(dt, locale)), calls: [] };
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

/** [V6.3] DB status → UI state. Pipeline step=1 (upload) рендерим как
 *  'uploading' — отдельная анимация ProgressRail, отдельный tag color. */
function deriveCallState(call: Call): CallState {
  if (call.status === 'recording') return 'live';
  if (call.status === 'failed') return 'error';
  if (call.status === 'ready') return 'ready';
  // status === 'processing'
  if (call.pipeline_step === 1) return 'uploading';
  return 'processing';
}

export function CallsPage({ onOpen }: CallsPageProps) {
  const { locale, t } = useI18n();
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
    let unlistenFinished: UnlistenFn | undefined;
    let unlistenProgress: UnlistenFn | undefined;
    listen<PipelineFinishedEvent>('pipeline:finished', () => {
      refresh();
    })
      .then((fn) => {
        unlistenFinished = fn;
      })
      .catch((e: unknown) => {
        console.warn('pipeline event listener:', e);
      });
    // [V6.3] Live per-step progress — обновляем only затронутый row
    // вместо полного refetch'а. DB-source-of-truth уже UPDATE'нут в pipeline
    // через set_call_progress; refresh() на каждый tick = overhead.
    listen<CallProgressEvent>('call:progress', (e) => {
      setCalls((prev) => {
        if (!prev) return prev;
        return prev.map((c) =>
          c.id === e.payload.call_id
            ? {
                ...c,
                pipeline_step: e.payload.step,
                pipeline_pct: e.payload.pct,
                pipeline_eta_sec: e.payload.eta_sec,
                upload_bytes: e.payload.upload_bytes,
              }
            : c,
        );
      });
    })
      .then((fn) => {
        unlistenProgress = fn;
      })
      .catch((e: unknown) => {
        console.warn('call:progress listener:', e);
      });
    return () => {
      unlistenFinished?.();
      unlistenProgress?.();
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
          {t('calls.title')}
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
    { id: 'all', label: t('calls.filterAll') },
    { id: 'today', label: t('calls.filterToday') },
    { id: 'week', label: t('calls.filterWeek') },
  ];

  const pluralForms: [string, string, string] = [
    t('calls.callsForm1'),
    t('calls.callsForm2'),
    t('calls.callsForm5'),
  ];

  // [V6.3] Active = recording | processing. Strip успокаивает юзера: можно
  // закрыть окно, прогресс не теряется (DB-state persists).
  const activeCount = calls.filter(
    (c) => c.status === 'recording' || c.status === 'processing',
  ).length;

  return (
    <section>
      {activeCount > 0 && (
        <div
          className="activity-strip"
          data-comment-anchor="calls-activity-strip"
        >
          <span className="stat-tag-dot" aria-hidden="true" />
          <span>
            {activeCount === 1
              ? t('calls.activityStripOne')
              : t('calls.activityStripMany', {
                  n: activeCount,
                  plural: declinePlural(activeCount, pluralForms),
                })}
          </span>
        </div>
      )}
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
          {t('calls.title')}
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
            placeholder={t('calls.search')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label={t('calls.searchAria')}
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
            ? t('calls.filteredOf', {
                filtered: filtered.length,
                total: calls.length,
                plural: declinePlural(calls.length, pluralForms),
              })
            : t('calls.countOf', {
                n: calls.length,
                plural: declinePlural(calls.length, pluralForms),
              })}
          {totalDurationSec > 0 && ` ${t('calls.hoursSuffix', { n: totalHours.toFixed(0) })}`}
        </div>
      )}

      {calls.length === 0 ? (
        <Empty title={t('calls.emptyTitle')} description={t('calls.emptyBody')} />
      ) : filtered.length === 0 ? (
        <Empty title={t('calls.notFoundTitle')} description={t('calls.notFoundBody')} />
      ) : filtered.length >= VIRTUALIZATION_THRESHOLD ? (
        <List
          rowComponent={VirtualCallRow}
          rowCount={filtered.length}
          rowHeight={ROW_HEIGHT}
          rowProps={{ calls: filtered, onOpen, speakerInitials, locale, t }}
          defaultHeight={VIRTUAL_LIST_HEIGHT}
        />
      ) : (
        <>
          {groupByMonth(filtered, locale).map((g) => (
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
                      locale={locale}
                      t={t}
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

type TFn = ReturnType<typeof useI18n>['t'];

interface CallRowProps {
  call: Call;
  onOpen: (id: string) => void;
  hasBorder: boolean;
  /** Initials of confirmed speakers (computed parent-side). Falls back
   *  to deterministic hash placeholder if missing. */
  speakers?: string[];
  locale: string;
  t: TFn;
}

function CallRow({ call, onOpen, hasBorder, speakers, t }: CallRowProps) {
  const list = speakers && speakers.length > 0 ? speakers : inferSpeakers(call);
  const uiState = deriveCallState(call);
  const showTag = uiState !== 'ready';
  const showRail = uiState === 'uploading' || uiState === 'processing';
  const title =
    call.title ?? t('calls.fallbackCallTitle', { short: call.id.slice(0, 8) });

  // [V6.8] Secondary info — единая строка под title, варианты по state.
  // failed: «<short reason> · аудио сохранено [подробнее →]» (без больших cards).
  // processing/uploading: текущий step label + ETA, под ним thin rail.
  // queued: «в очереди».
  // live: «идёт запись».
  // ready: пусто.
  const secondary = renderSecondary(call, uiState, t);

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onOpen(call.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen(call.id);
        }
      }}
      title={statusTooltip(call.status, call.failed_reason, t)}
      className="call-row"
      style={{
        // [V6.8] Responsive grid: date 36px / content min:0 1fr / avatars auto / duration auto.
        // min-width:0 на content-cell обязателен — иначе text-overflow:ellipsis не работает
        // (children по умолчанию min-content). gap:12 вместо 20 — узкие окна больше не ломаются.
        display: 'grid',
        gridTemplateColumns: '36px minmax(0, 1fr) auto auto',
        gap: 12,
        padding: '14px 0',
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
      {/* Content cell — min-width:0 чтобы child'ы могли ellipsis'иться */}
      <div style={{ minWidth: 0 }}>
        <div
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 17,
            letterSpacing: '-0.01em',
            color: 'var(--ink)',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            minWidth: 0,
          }}
        >
          <span
            title={title}
            style={{
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              flex: '0 1 auto',
            }}
          >
            {title}
          </span>
          {showTag && <CallStateTag state={uiState} />}
        </div>
        {secondary && (
          <div style={{ marginTop: 4, minWidth: 0 }}>{secondary}</div>
        )}
        {showRail && (
          <div style={{ marginTop: 6 }}>
            <ProgressRail
              indeterminate
              ariaLabel={t(`callState.${uiState}`)}
            />
          </div>
        )}
      </div>
      <div
        style={{
          display: 'flex',
          gap: 0,
          alignItems: 'center',
          flexShrink: 0,
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
            style={{ fontSize: 11, marginLeft: 6 }}
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
          flexShrink: 0,
          minWidth: 56,
        }}
      >
        {formatDuration(call.duration_sec)}
      </div>
    </div>
  );
}

/** [V6.8] Secondary-row content по state — единый компактный inline,
 *  не отдельные карды. ellipsis на длинных строках + tooltip на hover. */
function renderSecondary(
  call: Call,
  state: CallState,
  t: TFn,
): ReactNode {
  // ready — никакой второй строки (clean rest state)
  if (state === 'ready') return null;

  // failed — короткая первая фраза + «· аудио сохранено» + подробнее →
  if (state === 'error') {
    const raw = call.failed_reason?.trim() ?? '';
    const shortMsg = raw.split(/[—.\n]/)[0]?.trim() || t('callState.errorFallback');
    return (
      <div
        className="call-row-secondary call-row-secondary--error"
        title={raw || shortMsg}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          minWidth: 0,
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 13,
          color: 'var(--text-muted)',
        }}
      >
        <span
          style={{
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            flex: '0 1 auto',
          }}
        >
          {shortMsg} · {t('callState.audioSaved')}
        </span>
        <span
          className="mono"
          style={{
            fontSize: 11,
            color: 'var(--accent)',
            flexShrink: 0,
          }}
        >
          {t('callState.moreDetails')}
        </span>
      </div>
    );
  }

  // processing — текущий step label + ETA
  if (state === 'processing') {
    const step = clampStep(call.pipeline_step ?? 3);
    const stageKey = PIPELINE_STEP_KEYS[step - 1] ?? PIPELINE_STEP_KEYS[0];
    const eta = call.pipeline_eta_sec;
    const text =
      eta != null
        ? `${t(stageKey!)} · ${t('calls.secondaryEta', { sec: eta })}`
        : t(stageKey!);
    return secondaryText(text);
  }

  // uploading — «Загружаем аудио» + опц. «X / Y МБ»
  if (state === 'uploading') {
    const bytes = call.upload_bytes;
    const label = t('calls.secondaryUploading');
    if (bytes != null && bytes > 0) {
      return (
        <div
          className="call-row-secondary"
          style={{
            display: 'flex',
            alignItems: 'baseline',
            gap: 12,
            minWidth: 0,
            fontFamily: 'var(--font-serif)',
            fontStyle: 'italic',
            fontSize: 13,
            color: 'var(--text-muted)',
          }}
        >
          <span
            style={{
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              flex: '1 1 auto',
            }}
            title={label}
          >
            {label}
          </span>
          <span
            className="mono"
            style={{
              fontSize: 11,
              flexShrink: 0,
              color: 'var(--text-muted)',
            }}
          >
            {formatMegabytes(bytes)}
          </span>
        </div>
      );
    }
    return secondaryText(label);
  }

  // queued — «в очереди»
  if (state === 'queued') {
    return secondaryText(t('calls.secondaryQueued'));
  }

  // live — «идёт запись» (waveform на отдельной итерации, пока просто текст)
  if (state === 'live') {
    return secondaryText(t('calls.secondaryLive'), 'var(--signal)');
  }

  return null;
}

function secondaryText(text: string, color?: string): ReactNode {
  return (
    <div
      className="call-row-secondary"
      title={text}
      style={{
        fontFamily: 'var(--font-serif)',
        fontStyle: 'italic',
        fontSize: 13,
        color: color ?? 'var(--text-muted)',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        minWidth: 0,
      }}
    >
      {text}
    </div>
  );
}

function clampStep(step: number): 1 | 2 | 3 | 4 | 5 {
  const n = Math.min(Math.max(step | 0, 1), 5);
  return n as 1 | 2 | 3 | 4 | 5;
}

function formatMegabytes(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${(bytes / 1024).toFixed(0)} КБ`;
  return `${mb.toFixed(1)} МБ`;
}

interface VirtualRowProps {
  calls: Call[];
  onOpen: (id: string) => void;
  speakerInitials: Map<string, string[]>;
  locale: string;
  t: TFn;
}

function VirtualCallRow({
  index,
  style,
  calls,
  onOpen,
  speakerInitials,
  locale,
  t,
}: RowComponentProps<VirtualRowProps>) {
  const c = calls[index]!;
  return (
    <div style={style}>
      <CallRow
        call={c}
        onOpen={onOpen}
        hasBorder={index > 0}
        speakers={speakerInitials.get(c.id)}
        locale={locale}
        t={t}
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

function statusTooltip(status: string, failedReason: string | null, t: TFn): string {
  const base = (() => {
    switch (status) {
      case 'recording':
        return t('calls.tooltipRecording');
      case 'processing':
        return t('calls.tooltipProcessing');
      case 'ready':
        return t('calls.tooltipReady');
      case 'failed':
        return t('calls.tooltipFailed');
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
