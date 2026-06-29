// [B18.2a / B18.9] InboxView — Wotold v2 unified call list. Replaces interim
// CallsPage at the `inbox` route. Header = shared <ViewHead> (icon + title +
// count) carrying the OmniBar + FacetButton + icon-only ViewSwitcher + record
// action; the list view is the v2 database `.tbl` table (month-grouped via
// `.tbl-group`). Cards/week/month views are unchanged.
//
// Data/pipeline layer ported 1-to-1 from CallsPage (live pipeline events,
// speaker-initials aggregation, deriveCallState). The list view renders the
// table directly (react-window virtualization dropped — the prototype `.tbl`
// has a sticky head + group headers that don't fit a flat virtual list).

import { useEffect, useMemo, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { humanError } from '../api/errors';
import {
  listCalls,
  type Call,
  type CallProgressEvent,
} from '../api/recording';
import { listActiveCallIds } from '../api/calls';
import { listCallSpeakers } from '../api/speakers';
import {
  Button,
  CallRowSkeleton,
  Dropdown,
  Empty,
  IconBtn,
  MenuItem,
  MenuLabel,
  MenuSep,
  Segmented,
  ViewHead,
  type SegOption,
} from '../ui';
import { Icon, type IconName } from '../ui/Icon';
import { formatElapsed } from '../recording/RecordingContext';
import { LiveRecEq } from '../recording/LiveRecEq';
import { useI18n, type TranslationKey } from '../i18n';
import { TableRow } from './inboxBits';
import { InboxCards, InboxMonth, InboxWeek } from './InboxCalendarViews';
import {
  FACETS_EMPTY,
  declinePlural,
  facetCount,
  groupByMonth,
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

// StatusCell / AvatarGroup / the `.trow` TableRow live in ./inboxBits (shared
// with the calendar views and the list table below).

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
    <div
      className="omni"
      data-focus={focus ? 'true' : undefined}
      data-tauri-drag-region="false"
      style={{ flex: 1, minWidth: 0 }}
    >
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

// ── Facet button (dropdown checkboxes — same facet defs as the omni-bar) ──

interface FacetButtonProps {
  facets: Facets;
  setFacets: (next: Facets) => void;
  defs: FacetDef[];
  t: TFn;
}

function FacetButton({ facets, setFacets, defs, t }: FacetButtonProps) {
  const count = facetCount(facets);
  return (
    <Dropdown
      width={232}
      trigger={({ toggle }) => (
        <button
          type="button"
          className="btn btn--default"
          onClick={toggle}
          style={
            count > 0 ? { borderColor: 'var(--accent)', color: 'var(--accent-text)' } : undefined
          }
        >
          <Icon name="filter" size={14} />
          {t('inbox.filter')}
          {count > 0 ? ` · ${count}` : ''}
        </button>
      )}
    >
      {defs.map((def, i) => (
        <div key={def.key}>
          {i > 0 && <MenuSep />}
          <MenuLabel>{def.label}</MenuLabel>
          {def.values.map((val) => {
            const on = (facets[def.key] as string[]).includes(val.v);
            return (
              <button
                key={val.v}
                type="button"
                className="menu-item"
                data-active={on ? 'true' : undefined}
                onClick={(e) => {
                  e.stopPropagation();
                  setFacets(toggleFacet(facets, def.key, val.v));
                }}
              >
                <span className="chk" data-done={on ? 'true' : undefined} style={{ width: 15, height: 15 }}>
                  <Icon name="check" size={11} />
                </span>
                <span style={{ flex: 1 }}>{val.label}</span>
              </button>
            );
          })}
        </div>
      ))}
      {count > 0 && (
        <>
          <MenuSep />
          <MenuItem icon="x" onClick={() => setFacets({ ...FACETS_EMPTY })}>
            {t('inbox.clearAll')}
          </MenuItem>
        </>
      )}
    </Dropdown>
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
      iconOnly
      ariaLabel={t('inbox.viewLabel')}
    />
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
  /**
   * Optional record-action wiring for the header bar (App-level). When
   * `onRecord` is omitted the record control is not rendered at all — the
   * InboxView remains usable standalone (and in tests) without recording.
   */
  onRecord?: () => void;
  recording?: boolean;
  paused?: boolean;
  elapsed?: number;
  onPause?: () => void;
}

export function InboxView({
  onOpen,
  onRecord,
  recording = false,
  paused = false,
  elapsed = 0,
  onPause,
}: InboxViewProps) {
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
    // [B18.9-fix] Shared shell: bleed past .app-main 34/44 padding + fill the
    // viewport so the .view-head navbar spans flush (rail→right edge) and the
    // table scrolls in its own region below — same pattern as Contacts/Settings.
    <div className="main" style={{ margin: '-34px -44px', height: '100vh' }}>
      <ViewHead icon="inbox" title={t('nav.calls')} count={calls?.length} countTone="line">
        <div
          style={{
            display: 'flex',
            gap: 6,
            flex: '1 1 auto',
            maxWidth: 480,
            marginLeft: 10,
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <OmniBar
              facets={facets}
              setFacets={setFacets}
              text={text}
              setText={setText}
              defs={defs}
              t={t}
            />
          </div>
          <FacetButton facets={facets} setFacets={setFacets} defs={defs} t={t} />
        </div>
        <ViewSwitcher view={view} setView={setView} t={t} />
        <div style={{ flex: 1 }} />
        {onRecord &&
          (recording ? (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <IconBtn
                icon={paused ? 'play' : 'pause'}
                label={paused ? t('recording.resumeAction') : t('recording.pauseAction')}
                onClick={onPause}
              />
              <button
                type="button"
                className="btn btn--danger"
                onClick={onRecord}
                style={{ gap: 8 }}
              >
                <LiveRecEq paused={paused} inherit />
                <span className="mono" style={{ fontWeight: 600 }}>
                  {formatElapsed(elapsed)}
                </span>
                <Icon name="stop" size={14} />
              </button>
            </div>
          ) : (
            <Button variant="primary" leading={<Icon name="mic" size={16} />} onClick={onRecord}>
              {t('inbox.recordShort')}
            </Button>
          ))}
      </ViewHead>

      {/* Body — the `.tbl` table is flush (its head/rows self-pad via wk.css);
          the calendar views pad themselves; the skeleton / error / footer get
          their own `.pad` so only the table touches the bar edges. */}
      <div className="scroll" style={{ flex: '1 1 auto', minHeight: 0 }}>
        {error ? (
          <p
            role="alert"
            className="pad"
            style={{ color: 'var(--danger)', fontFamily: 'var(--font)' }}
          >
            {error}
          </p>
        ) : !calls ? (
          <ul className="pad" style={{ listStyle: 'none' }} aria-busy="true">
            {Array.from({ length: 5 }, (_, i) => (
              <li key={i}>
                <CallRowSkeleton />
              </li>
            ))}
          </ul>
        ) : view === 'cards' ? (
          <InboxCards
            calls={filtered}
            onOpen={onOpen}
            speakerInitials={speakerInitials}
            locale={locale}
            t={t}
          />
        ) : view === 'week' ? (
          <InboxWeek
            calls={filtered}
            onOpen={onOpen}
            speakerInitials={speakerInitials}
            locale={locale}
            t={t}
          />
        ) : view === 'month' ? (
          <InboxMonth
            calls={filtered}
            onOpen={onOpen}
            speakerInitials={speakerInitials}
            locale={locale}
            t={t}
          />
        ) : calls.length === 0 ? (
          <Empty title={t('calls.emptyTitle')} description={t('calls.emptyBody')} />
        ) : filtered.length === 0 ? (
          <Empty title={t('calls.notFoundTitle')} description={t('calls.notFoundBody')} />
        ) : (
          <div className="tbl">
            <div className="tbl-head">
              <span />
              <span>{t('inbox.colName')}</span>
              <span>{t('inbox.colParticipants')}</span>
              <span className="th-sort">
                {t('inbox.colDuration')}
                <Icon name="sort" size={11} />
              </span>
              <span className="th-sort">
                {t('inbox.colDate')}
                <Icon name="sort" size={11} />
              </span>
              <span />
            </div>
            {groupByMonth(filtered, locale).map((g) => (
              <div key={g.label}>
                <div className="tbl-group">{g.label}</div>
                {g.calls.map((c) => (
                  <TableRow
                    key={c.id}
                    call={c}
                    onOpen={onOpen}
                    speakers={speakerInitials.get(c.id)}
                    isActive={activeIds.has(c.id)}
                    locale={locale}
                    t={t}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
        {calls && calls.length > 0 && (
          <div
            className="small-caps"
            style={{ padding: '8px var(--s4) var(--s4)', color: 'var(--text-faint)' }}
          >
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
      </div>
    </div>
  );
}
