// [B18.2a] InboxView — Wotold v2 unified call list. Replaces interim CallsPage
// at the `inbox` route. Header = Icon + title + count + OmniBar (text + facet
// tokens) + ViewSwitcher; body = month-grouped v2 rows + virtualization.
//
// Data/pipeline layer ported 1-to-1 from CallsPage (live pipeline events,
// speaker-initials aggregation, deriveCallState). Cards/week/month views,
// person facet and the row ⋯-menu land in B18.2b.

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { humanError } from '../api/errors';
import {
  listCalls,
  type Call,
  type CallProgressEvent,
} from '../api/recording';
import { listActiveCallIds } from '../api/calls';
import { listCallSpeakers } from '../api/speakers';
import { EngineChip } from '../components/EngineChip';
import { List, type RowComponentProps } from 'react-window';
import { CallRowSkeleton, Empty, Segmented, type SegOption } from '../ui';
import { Icon, type IconName } from '../ui/Icon';
import { useI18n, type TranslationKey } from '../i18n';
import { CallStateTag, ProgressRail } from '../components/call-state';
import { pipelineStepKey, type CallState } from '../types/callState';
import { AvatarGroup, StatusCell } from './inboxBits';
import { InboxCards, InboxMonth, InboxWeek } from './InboxCalendarViews';
import {
  FACETS_EMPTY,
  ROW_HEIGHT,
  VIRTUALIZATION_THRESHOLD,
  VIRTUAL_LIST_HEIGHT,
  callHasRecap,
  declinePlural,
  deriveCallState,
  facetCount,
  formatDuration,
  formatMegabytes,
  groupByMonth,
  inferSpeakers,
  initials,
  matchesFacets,
  toggleFacet,
  type Facets,
} from './inboxData';

type TFn = ReturnType<typeof useI18n>['t'];
type InboxViewMode = 'list' | 'cards' | 'week' | 'month';

interface FacetDef {
  key: keyof Facets;
  label: string;
  icon: IconName;
  values: { v: string; label: string }[];
}

function facetDefs(t: TFn, persons: string[]): FacetDef[] {
  const defs: FacetDef[] = [
    {
      key: 'status',
      label: t('inbox.facetStatus'),
      icon: 'bolt',
      values: [
        { v: 'ready', label: t('inbox.statusReady') },
        { v: 'processing', label: t('inbox.statusProcessing') },
        { v: 'error', label: t('inbox.statusError') },
      ],
    },
    {
      key: 'recap',
      label: t('inbox.facetRecap'),
      icon: 'sparkle',
      values: [
        { v: 'yes', label: t('inbox.recapYes') },
        { v: 'no', label: t('inbox.recapNo') },
      ],
    },
    {
      key: 'period',
      label: t('inbox.facetPeriod'),
      icon: 'calendar',
      values: [
        { v: 'today', label: t('inbox.periodToday') },
        { v: 'week', label: t('inbox.periodWeek') },
      ],
    },
  ];
  // [B18.7b] Person facet — values are confirmed-contact display names across
  // ready calls (dynamic; only shown once at least one confirmed contact exists).
  if (persons.length > 0) {
    defs.push({
      key: 'person',
      label: t('inbox.facetPerson'),
      icon: 'users',
      values: persons.map((n) => ({ v: n, label: n })),
    });
  }
  return defs;
}

// StatusCell / AvatarGroup / statusColor moved to ./inboxBits (shared with
// the calendar views). CallState is still used by renderSecondary below.

// ── Omni-bar (text + facet tokens + suggestions) ──

interface OmniBarProps {
  facets: Facets;
  setFacets: (next: Facets) => void;
  text: string;
  setText: (v: string) => void;
  defs: FacetDef[];
  t: TFn;
}

function OmniBar({ facets, setFacets, text, setText, defs, t }: OmniBarProps) {
  const [draft, setDraft] = useState('');
  const [focus, setFocus] = useState(false);

  const labelOf = (k: keyof Facets, v: string) =>
    defs.find((d) => d.key === k)?.values.find((x) => x.v === v)?.label ?? v;
  const iconOf = (k: keyof Facets) => defs.find((d) => d.key === k)?.icon ?? 'bolt';

  const tokens: { k: keyof Facets; v: string }[] = [];
  (Object.keys(facets) as (keyof Facets)[]).forEach((k) =>
    (facets[k] as string[]).forEach((v) => tokens.push({ k, v })),
  );

  const allTok = defs.flatMap((d) =>
    d.values.map((val) => ({ k: d.key, v: val.v, label: val.label, fl: d.label, icon: d.icon })),
  );
  const q = draft.trim().toLowerCase();
  const sugg = (q
    ? allTok.filter((x) => x.label.toLowerCase().includes(q) || x.fl.toLowerCase().includes(q))
    : allTok
  )
    .filter((x) => !(facets[x.k] as string[]).includes(x.v))
    .slice(0, 5);

  const add = (tok: { k: keyof Facets; v: string }) => {
    setFacets(toggleFacet(facets, tok.k, tok.v));
    setDraft('');
  };
  const rm = (k: keyof Facets, v: string) => setFacets(toggleFacet(facets, k, v));
  const hasAny = tokens.length > 0 || !!text;

  return (
    <div className="omni" data-focus={focus ? 'true' : undefined} style={{ flex: 1, minWidth: 0 }}>
      <Icon name="search" size={15} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />
      <div className="omni-row">
        {tokens.map((tok) => (
          <span
            key={tok.k + tok.v}
            className="chip chip--accent"
            style={{ gap: 4, flex: '0 0 auto' }}
          >
            <Icon name={iconOf(tok.k)} size={11} />
            {labelOf(tok.k, tok.v)}
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                rm(tok.k, tok.v);
              }}
              style={{ display: 'inline-flex', color: 'inherit' }}
              aria-label={t('inbox.clearAll')}
            >
              <Icon name="x" size={11} />
            </button>
          </span>
        ))}
        {text && (
          <span className="chip chip--line" style={{ gap: 4, flex: '0 0 auto' }}>
            «{text}»
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                setText('');
              }}
              style={{ display: 'inline-flex', color: 'inherit' }}
              aria-label={t('inbox.clearAll')}
            >
              <Icon name="x" size={11} />
            </button>
          </span>
        )}
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={hasAny ? '' : t('inbox.searchPlaceholder')}
          aria-label={t('inbox.searchPlaceholder')}
          onFocus={() => setFocus(true)}
          onBlur={() => setTimeout(() => setFocus(false), 160)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (q && sugg[0]) add(sugg[0]);
              else if (draft.trim()) {
                setText(draft.trim());
                setDraft('');
              }
            }
            if (e.key === 'Backspace' && !draft) {
              if (text) setText('');
              else if (tokens.length) {
                const last = tokens[tokens.length - 1]!;
                rm(last.k, last.v);
              }
            }
          }}
        />
      </div>
      {hasAny && (
        <button
          type="button"
          className="iconbtn"
          data-size="sm"
          onMouseDown={(e) => {
            e.preventDefault();
            setFacets({ ...FACETS_EMPTY });
            setText('');
          }}
          aria-label={t('inbox.clearAll')}
          style={{ flex: '0 0 auto' }}
        >
          <Icon name="x" size={14} />
        </button>
      )}
      {focus && sugg.length > 0 && (
        <div className="menu" style={{ left: 0, right: 0, top: 'calc(100% + 5px)', width: 'auto' }}>
          <div className="menu-label">{q ? t('inbox.addFilter') : t('inbox.quickFilters')}</div>
          {sugg.map((s) => (
            <button
              key={s.k + s.v}
              type="button"
              className="menu-item"
              onMouseDown={(e) => {
                e.preventDefault();
                add({ k: s.k, v: s.v });
              }}
            >
              <span className="mi-ico">
                <Icon name={s.icon} size={15} />
              </span>
              <span style={{ flex: 1 }}>
                <span className="u-faint">{s.fl}: </span>
                {s.label}
              </span>
            </button>
          ))}
          {q && (
            <button
              type="button"
              className="menu-item"
              onMouseDown={(e) => {
                e.preventDefault();
                setText(draft.trim());
                setDraft('');
              }}
            >
              <span className="mi-ico">
                <Icon name="search" size={15} />
              </span>
              <span>{t('inbox.searchInTitles', { q: draft.trim() })}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ── View switcher (icon segmented) ──

const VIEW_DEFS: [InboxViewMode, IconName, TranslationKey][] = [
  ['list', 'list', 'inbox.viewList'],
  ['cards', 'grid', 'inbox.viewCards'],
  ['week', 'calendarWeek', 'inbox.viewWeek'],
  ['month', 'calendar', 'inbox.viewMonth'],
];

function ViewSwitcher({
  view,
  setView,
  t,
}: {
  view: InboxViewMode;
  setView: (v: InboxViewMode) => void;
  t: TFn;
}) {
  const options: SegOption<InboxViewMode>[] = VIEW_DEFS.map(([v, icon, key]) => ({
    value: v,
    label: t(key),
    icon,
  }));
  return (
    <Segmented<InboxViewMode>
      options={options}
      value={view}
      onChange={setView}
      ariaLabel={t('inbox.viewLabel')}
    />
  );
}

// ── Secondary row (state-dependent) ──

function secondaryText(text: string, color?: string): ReactNode {
  return (
    <div
      className="u-trunc"
      title={text}
      style={{ fontSize: 12, color: color ?? 'var(--text-3)', minWidth: 0 }}
    >
      {text}
    </div>
  );
}

function renderSecondary(call: Call, state: CallState, t: TFn): ReactNode {
  if (state === 'ready') return null;
  if (state === 'error') {
    const raw = call.failed_reason?.trim() ?? '';
    const shortMsg = raw.split(/[—.\n]/)[0]?.trim() || t('callState.errorFallback');
    return (
      <div
        className="u-trunc"
        title={raw || shortMsg}
        style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, fontSize: 12 }}
      >
        <span className="u-trunc" style={{ color: 'var(--text-3)' }}>
          {shortMsg} · {t('callState.audioSaved')}
        </span>
        <span className="mono" style={{ fontSize: 11, color: 'var(--accent-text)', flexShrink: 0 }}>
          {t('callState.moreDetails')}
        </span>
      </div>
    );
  }
  if (state === 'processing') {
    const eta = call.pipeline_eta_sec;
    return eta != null ? secondaryText(t('calls.secondaryEta', { sec: eta })) : null;
  }
  if (state === 'uploading') {
    const bytes = call.upload_bytes;
    if (bytes != null && bytes > 0) {
      return secondaryText(formatMegabytes(bytes));
    }
    return null;
  }
  if (state === 'queued') return secondaryText(t('calls.secondaryQueued'));
  if (state === 'live') return secondaryText(t('calls.secondaryLive'), 'var(--danger)');
  return null;
}

// ── Call row ──

interface CallRowProps {
  call: Call;
  onOpen: (id: string) => void;
  hasBorder: boolean;
  speakers?: string[];
  isActive?: boolean;
  t: TFn;
}

function CallRow({ call, onOpen, hasBorder, speakers, isActive, t }: CallRowProps) {
  const list = speakers && speakers.length > 0 ? speakers : inferSpeakers(call);
  const uiState = deriveCallState(call);
  const busy = call.status === 'ready' && isActive === true;
  const showTag = uiState !== 'ready' || busy;
  const showRail = uiState === 'uploading' || uiState === 'processing' || busy;
  const title = call.title ?? t('calls.fallbackCallTitle', { short: call.id.slice(0, 8) });
  const secondary = busy ? secondaryText(t('calls.secondaryBusy')) : renderSecondary(call, uiState, t);

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
      className="lrow"
      style={{
        display: 'grid',
        gridTemplateColumns: '14px minmax(0, 1fr) auto auto',
        gap: 12,
        padding: '11px 8px',
        width: '100%',
        borderRadius: 'var(--r-sm)',
        borderTop: hasBorder ? '1px solid var(--border)' : 'none',
        alignItems: 'center',
        textAlign: 'left',
        cursor: 'pointer',
      }}
    >
      <span style={{ display: 'inline-flex', justifyContent: 'center', paddingTop: 2 }}>
        <StatusCell call={call} busy={busy} />
      </span>
      <div style={{ minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
          <span
            className="u-trunc"
            title={title}
            style={{ fontWeight: 550, fontSize: 13.5, flex: '0 1 auto', minWidth: 0 }}
          >
            {title}
          </span>
          {callHasRecap(call) && !showTag && (
            <Icon name="sparkle" size={13} style={{ color: 'var(--text-faint)', flexShrink: 0 }} />
          )}
          {showTag && (
            <CallStateTag
              state={busy ? 'processing' : uiState}
              labelOverride={
                busy
                  ? t('callState.busyGeneric')
                  : uiState === 'processing' || uiState === 'uploading'
                    ? t(pipelineStepKey(call.pipeline_step))
                    : undefined
              }
            />
          )}
          {!showTag && call.processing_via && (
            <EngineChip kind={call.processing_via} variant="inline" />
          )}
        </div>
        {secondary && <div style={{ marginTop: 3, minWidth: 0 }}>{secondary}</div>}
        {showRail && (
          <div style={{ marginTop: 6 }}>
            <ProgressRail
              indeterminate
              ariaLabel={busy ? t('callState.busyGeneric') : t(`callState.${uiState}`)}
            />
          </div>
        )}
      </div>
      <AvatarGroup list={list} />
      <div
        className="mono u-faint"
        style={{ fontSize: 12, textAlign: 'right', letterSpacing: '0.02em', minWidth: 52 }}
      >
        {formatDuration(call.duration_sec)}
      </div>
    </div>
  );
}

interface VirtualRowProps {
  calls: Call[];
  onOpen: (id: string) => void;
  speakerInitials: Map<string, string[]>;
  activeIds: Set<string>;
  t: TFn;
}

function VirtualCallRow({
  index,
  style,
  calls,
  onOpen,
  speakerInitials,
  activeIds,
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
        isActive={activeIds.has(c.id)}
        t={t}
      />
    </div>
  );
}

// ── Main ──

interface PipelineFinishedEvent {
  call_id: string;
  status: 'ready' | 'failed';
  failed_reason: string | null;
}

interface InboxViewProps {
  onOpen: (callId: string) => void;
}

export function InboxView({ onOpen }: InboxViewProps) {
  const { locale, t } = useI18n();
  const [calls, setCalls] = useState<Call[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState('');
  const [facets, setFacets] = useState<Facets>(FACETS_EMPTY);
  const [view, setView] = useState<InboxViewMode>('list');
  const [speakerInitials, setSpeakerInitials] = useState<Map<string, string[]>>(new Map());
  // [B18.7b] Confirmed-contact display names per call → powers the person facet.
  const [callPersons, setCallPersons] = useState<Map<string, string[]>>(new Map());
  const [activeIds, setActiveIds] = useState<Set<string>>(new Set());

  const refresh = () => {
    listCalls()
      .then(setCalls)
      .catch((e: unknown) => setError(humanError(e)));
  };

  useEffect(() => {
    refresh();
    listActiveCallIds()
      .then((ids) => setActiveIds(new Set(ids)))
      .catch((e: unknown) => console.warn('listActiveCallIds:', e));

    const removeActive = (callId: string) =>
      setActiveIds((prev) => {
        if (!prev.has(callId)) return prev;
        const next = new Set(prev);
        next.delete(callId);
        return next;
      });

    const unlisteners: UnlistenFn[] = [];
    const track = (p: Promise<UnlistenFn>, label: string) => {
      p.then((fn) => unlisteners.push(fn)).catch((e: unknown) =>
        console.warn(`${label} listener:`, e),
      );
    };

    track(
      listen<{ call_id: string }>('pipeline:started', (e) => {
        setActiveIds((prev) => new Set(prev).add(e.payload.call_id));
      }),
      'pipeline:started',
    );
    track(
      listen<PipelineFinishedEvent>('pipeline:finished', (e) => {
        removeActive(e.payload.call_id);
        refresh();
      }),
      'pipeline:finished',
    );
    track(
      listen<{ call_id: string }>('pipeline:cancelled', (e) => {
        removeActive(e.payload.call_id);
        refresh();
      }),
      'pipeline:cancelled',
    );
    track(
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
      }),
      'call:progress',
    );
    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // Aggregate confirmed speakers per ready call (one-shot after list).
  useEffect(() => {
    if (!calls || calls.length === 0) return;
    void (async () => {
      const ready = calls.filter((c) => c.status === 'ready');
      const results = await Promise.allSettled(ready.map((c) => listCallSpeakers(c.id)));
      const next = new Map<string, string[]>();
      const persons = new Map<string, string[]>();
      results.forEach((r, i) => {
        if (r.status !== 'fulfilled') return;
        const callId = ready[i]!.id;
        const out: string[] = [];
        const names: string[] = [];
        for (const s of r.value) {
          if (s.confirmed && s.contact_display_name) {
            out.push(initials(s.contact_display_name));
            names.push(s.contact_display_name);
          }
        }
        next.set(callId, out);
        if (names.length > 0) persons.set(callId, names);
      });
      setSpeakerInitials(next);
      setCallPersons(persons);
    })();
  }, [calls]);

  // [B18.7b] Distinct confirmed-contact names → person facet values.
  const allPersons = useMemo(() => {
    const set = new Set<string>();
    callPersons.forEach((names) => names.forEach((n) => set.add(n)));
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [callPersons]);

  const defs = useMemo(() => facetDefs(t, allPersons), [t, allPersons]);

  const filtered = useMemo(
    () => (calls ?? []).filter((c) => matchesFacets(c, facets, text.trim(), callPersons)),
    [calls, facets, text, callPersons],
  );

  const pluralForms: [string, string, string] = [
    t('calls.callsForm1'),
    t('calls.callsForm2'),
    t('calls.callsForm5'),
  ];
  const nActive = facetCount(facets) + (text ? 1 : 0);

  return (
    <section>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 14,
          marginBottom: 22,
          flexWrap: 'wrap',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
          <Icon name="inbox" size={20} style={{ color: 'var(--text-2)' }} />
          <span style={{ fontSize: 18, fontWeight: 650 }}>{t('nav.calls')}</span>
          {calls && (
            <span className="chip" data-size="sm">
              {nActive > 0 ? `${filtered.length} / ${calls.length}` : calls.length}
            </span>
          )}
        </div>
        <OmniBar
          facets={facets}
          setFacets={setFacets}
          text={text}
          setText={setText}
          defs={defs}
          t={t}
        />
        <ViewSwitcher view={view} setView={setView} t={t} />
      </div>

      {error ? (
        <p role="alert" style={{ color: 'var(--danger)', fontFamily: 'var(--font)' }}>
          {error}
        </p>
      ) : !calls ? (
        <ul style={{ listStyle: 'none', padding: 0 }} aria-busy="true">
          {Array.from({ length: 5 }, (_, i) => (
            <li key={i}>
              <CallRowSkeleton />
            </li>
          ))}
        </ul>
      ) : view === 'cards' ? (
        <InboxCards calls={filtered} onOpen={onOpen} speakerInitials={speakerInitials} locale={locale} t={t} />
      ) : view === 'week' ? (
        <InboxWeek calls={filtered} onOpen={onOpen} speakerInitials={speakerInitials} locale={locale} t={t} />
      ) : view === 'month' ? (
        <InboxMonth calls={filtered} onOpen={onOpen} speakerInitials={speakerInitials} locale={locale} t={t} />
      ) : calls.length === 0 ? (
        <Empty title={t('calls.emptyTitle')} description={t('calls.emptyBody')} />
      ) : filtered.length === 0 ? (
        <Empty title={t('calls.notFoundTitle')} description={t('calls.notFoundBody')} />
      ) : filtered.length >= VIRTUALIZATION_THRESHOLD ? (
        <List
          rowComponent={VirtualCallRow}
          rowCount={filtered.length}
          rowHeight={ROW_HEIGHT}
          rowProps={{ calls: filtered, onOpen, speakerInitials, activeIds, t }}
          defaultHeight={VIRTUAL_LIST_HEIGHT}
        />
      ) : (
        groupByMonth(filtered, locale).map((g) => (
          <div key={g.label} style={{ marginBottom: 28 }}>
            <div className="sec-label" style={{ marginBottom: 4 }}>
              {g.label}
            </div>
            {g.calls.map((c, idx) => (
              <CallRow
                key={c.id}
                call={c}
                onOpen={onOpen}
                hasBorder={idx > 0}
                speakers={speakerInitials.get(c.id)}
                isActive={activeIds.has(c.id)}
                t={t}
              />
            ))}
          </div>
        ))
      )}
      {calls && calls.length > 0 && (
        <div className="small-caps" style={{ marginTop: 8, color: 'var(--text-faint)' }}>
          {nActive > 0
            ? t('calls.filteredOf', {
                filtered: filtered.length,
                total: calls.length,
                plural: declinePlural(calls.length, pluralForms),
              })
            : t('calls.countOf', {
                n: calls.length,
                plural: declinePlural(calls.length, pluralForms),
              })}
        </div>
      )}
    </section>
  );
}
