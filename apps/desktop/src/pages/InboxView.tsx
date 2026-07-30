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

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { humanError } from '../api/errors';
import {
  listCalls,
  type Call,
  type CallProgressEvent,
} from '../api/recording';
import { listActiveCallIds } from '../api/calls';
import { listCallSpeakersBatch, type CallSpeakerView } from '../api/speakers';
import {
  Button,
  CallRowSkeleton,
  Empty,
  IconBtn,
  ViewHead,
  useToast,
} from '../ui';
import { Icon } from '../ui/Icon';
import { formatElapsed } from '../recording/RecordingContext';
import { LiveRecEq } from '../recording/LiveRecEq';
import { useI18n } from '../i18n';
import { TableRow } from './inboxBits';
import { useInboxRowActions } from './useInboxRowActions';
import { FacetButton, OmniBar } from './inboxOmni';
import { ViewSwitcher, type InboxViewMode } from './InboxViewSwitcher';
import { InboxCards, InboxMonth, InboxWeek } from './InboxCalendarViews';
import {
  FACETS_EMPTY,
  confirmedParticipants,
  declinePlural,
  facetCount,
  groupByMonth,
  matchesFacets,
  type Facets,
  type FacetDef,
} from './inboxData';

type TFn = ReturnType<typeof useI18n>['t'];


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
  /** [B20.4] Keep-alive: компонент всегда mounted, false = скрыт (display:none).
   *  Состояние (вид/поиск/фасеты/offset'ы/скролл) переживает навигацию. */
  active?: boolean;
}

export function InboxView({
  onOpen,
  onRecord,
  recording = false,
  paused = false,
  elapsed = 0,
  onPause,
  active = true,
}: InboxViewProps) {
  const { locale, t } = useI18n();
  const toast = useToast();
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
      .then((rows) => {
        setCalls(rows);
        // [TD-26] Успех обязан снимать ошибку. Раньше error только ставился,
        // а рендер держит error-ветку первой — один transient
        // «database is locked» показывал текст ошибки вместо списка до
        // перезапуска приложения.
        setError(null);
      })
      .catch((e: unknown) => setError(humanError(e, t)));
  };

  // [B19.7, B20.5] Row-menu actions (reprocess/export/delete) — общий hook,
  // используются таблицей и ПКМ-меню календарных видов.
  const { onRowReprocess, onRowExport, onRowDelete } = useInboxRowActions({
    t,
    toast,
    refresh,
    markActive: (callId) => setActiveIds((prev) => new Set(prev).add(callId)),
  });

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
  // [TD-46] Один батч-вызов вместо вызова на каждый готовый звонок: инбокс
  // смонтирован через keep-alive и стрелял этой пачкой даже пока
  // пользователь смотрит настройки.
  useEffect(() => {
    if (!calls || calls.length === 0) return;
    let cancelled = false;
    void (async () => {
      const ready = calls.filter((c) => c.status === 'ready');
      if (ready.length === 0) {
        if (!cancelled) {
          setSpeakerInitials(new Map());
          setCallPersons(new Map());
        }
        return;
      }
      let byCall: Record<string, CallSpeakerView[]>;
      try {
        byCall = await listCallSpeakersBatch(ready.map((c) => c.id));
      } catch {
        // Аватары — украшение строки списка: их отсутствие не повод рушить
        // инбокс. Прежний Promise.allSettled глотал ошибки поштучно, здесь
        // отваливается вся пачка сразу.
        return;
      }
      if (cancelled) return;
      const next = new Map<string, string[]>();
      const persons = new Map<string, string[]>();
      for (const call of ready) {
        const speakers = byCall[call.id];
        if (!speakers) continue;
        // [B29.1] Дедуп по контакту: несколько голосов одного человека — один аватар.
        const { initials: out, names } = confirmedParticipants(speakers);
        next.set(call.id, out);
        if (names.length > 0) persons.set(call.id, names);
      }
      setSpeakerInitials(next);
      setCallPersons(persons);
    })();
    return () => {
      cancelled = true;
    };
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

  // [TD-26] Сортировка была нарисована, но не существовала: колонки имели
  // иконку и cursor:pointer, а ни onClick, ни состояния не было. Класс
  // `.th-sort` есть в каноне uikit, то есть аффорданса задумана — доводим её
  // до рабочего состояния, а не убираем.
  //
  // Группировка по месяцам осмысленна только при сортировке по дате: она и
  // есть дата. При сортировке по длительности группы выключаются, иначе
  // «самый длинный» означало бы «самый длинный внутри своего месяца».
  const [sort, setSort] = useState<{ key: 'date' | 'duration'; dir: 'asc' | 'desc' }>({
    key: 'date',
    dir: 'desc',
  });
  const toggleSort = (key: 'date' | 'duration') =>
    setSort((prev) => (prev.key === key ? { key, dir: prev.dir === 'desc' ? 'asc' : 'desc' } : { key, dir: 'desc' }));
  const ariaSort = (key: 'date' | 'duration'): 'ascending' | 'descending' | 'none' =>
    sort.key === key ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none';

  const sorted = useMemo(() => {
    const rows = [...filtered];
    const sign = sort.dir === 'asc' ? 1 : -1;
    rows.sort((a, b) =>
      sort.key === 'duration'
        ? sign * ((a.duration_sec ?? 0) - (b.duration_sec ?? 0))
        : sign * (new Date(a.started_at).getTime() - new Date(b.started_at).getTime()),
    );
    return rows;
  }, [filtered, sort]);

  const pluralForms: [string, string, string] = [
    t('calls.callsForm1'),
    t('calls.callsForm2'),
    t('calls.callsForm5'),
  ];
  const nActive = facetCount(facets) + (text ? 1 : 0);

  // [B20.4] Keep-alive scroll restore: display:none сбрасывает scrollTop в
  // WebKit → пишем позицию непрерывно (onScroll) и восстанавливаем при show.
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const savedScroll = useRef(0);
  useLayoutEffect(() => {
    // scrollTop-присваивание вместо scrollTo() — jsdom-safe + мгновенно.
    if (active && scrollRef.current) scrollRef.current.scrollTop = savedScroll.current;
  }, [active]);
  // Реактивация: подхватить изменения, случившиеся вне event-потока
  // (напр. удаление звонка из kebab на CallDetailPage). Update in place —
  // старые строки остаются на экране, без флика.
  const wasActive = useRef(active);
  useEffect(() => {
    if (active && !wasActive.current) refresh();
    wasActive.current = active;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  return (
    // [B18.9-fix] Shared shell: bleed past .app-main 34/44 padding + fill the
    // viewport so the .view-head navbar spans flush (rail→right edge) and the
    // table scrolls in its own region below — same pattern as Contacts/Settings.
    <div
      className="main page-bleed"
      style={{ display: active ? undefined : 'none' }}
    >
      <ViewHead icon="inbox" title={t('nav.calls')} count={calls?.length} countTone="line">
        {/* [B34.2] Обёртка по содержимому: раньше поле и фильтр делили общий
            бюджет 480px, и на окне 980px поиск выходил заметно уже, чем на
            контактах с ассистентом. Теперь ширина поля одна на все страницы
            (--search-w), а фильтр к ней не приплюсовывается. */}
        <div
          style={{
            display: 'flex',
            gap: 6,
            flex: '0 0 auto',
            marginLeft: 10,
          }}
        >
          <div style={{ flex: '0 0 var(--search-w)', minWidth: 0 }}>
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
                aria-label={t('recording.stopAction')}
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
      <div
        className="scroll"
        style={{ flex: '1 1 auto', minHeight: 0 }}
        ref={scrollRef}
        onScroll={(e) => {
          savedScroll.current = e.currentTarget.scrollTop;
        }}
      >
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
            onReprocess={onRowReprocess}
            onExport={onRowExport}
            onDelete={onRowDelete}
            activeIds={activeIds}
          />
        ) : view === 'week' ? (
          <InboxWeek
            calls={filtered}
            onOpen={onOpen}
            speakerInitials={speakerInitials}
            locale={locale}
            t={t}
            onReprocess={onRowReprocess}
            onExport={onRowExport}
            onDelete={onRowDelete}
            activeIds={activeIds}
          />
        ) : view === 'month' ? (
          <InboxMonth
            calls={filtered}
            onOpen={onOpen}
            speakerInitials={speakerInitials}
            locale={locale}
            t={t}
            onReprocess={onRowReprocess}
            onExport={onRowExport}
            onDelete={onRowDelete}
            activeIds={activeIds}
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
              <span role="columnheader" aria-sort={ariaSort('duration')}>
                <button type="button" className="th-sort" onClick={() => toggleSort('duration')}>
                  {t('inbox.colDuration')}
                  <Icon name="sort" size={11} />
                </button>
              </span>
              <span role="columnheader" aria-sort={ariaSort('date')}>
                <button type="button" className="th-sort" onClick={() => toggleSort('date')}>
                  {t('inbox.colDate')}
                  <Icon name="sort" size={11} />
                </button>
              </span>
              <span />
            </div>
            {(sort.key === 'date'
              ? groupByMonth(sorted, locale)
              : [{ label: '', calls: sorted }]
            ).map((g) => (
              <div key={g.label || 'flat'}>
                {g.label && <div className="tbl-group">{g.label}</div>}
                {g.calls.map((c) => (
                  <TableRow
                    key={c.id}
                    call={c}
                    onOpen={onOpen}
                    onReprocess={onRowReprocess}
                    onExport={onRowExport}
                    onDelete={onRowDelete}
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
