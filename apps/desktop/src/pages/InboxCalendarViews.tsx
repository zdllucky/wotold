// [B18.2b] Inbox calendar/grid views — Карточки / Неделя / Месяц. Ports of
// ~/Downloads/Wotold v2/wk-inbox.jsx (InboxCards/InboxWeek/InboxMonth/CalHeader).
// Self-contained: render the already-filtered `calls` + speaker initials; no new
// APIs. Today = real new Date(). Month/weekday labels via Intl. [B19.2] CalHeader
// label is a Dropdown with year-nav + month-grid for quick jumps.

import { useMemo, useState } from 'react';
import { bcp47, useI18n } from '../i18n';
import type { Call } from '../api/recording';
import { Dropdown, Empty, IconBtn } from '../ui';
import { Icon } from '../ui/Icon';
import { callHasRecap, deriveCallState, formatDuration, inferSpeakers } from './inboxData';
import { AvatarGroup, StatusCell, statusColor } from './inboxBits';
import { callToDayEvent, hourRange, HOUR_PX, packDayEvents } from './weekGrid';

type TFn = ReturnType<typeof useI18n>['t'];

interface ViewProps {
  calls: Call[];
  onOpen: (id: string) => void;
  speakerInitials: Map<string, string[]>;
  locale: string;
  t: TFn;
}

// ── helpers ──
const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

function mondayOf(d: Date): Date {
  const m = new Date(d);
  m.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  m.setHours(0, 0, 0, 0);
  return m;
}

function bcp(locale: string) {
  return bcp47(locale as Parameters<typeof bcp47>[0]);
}

function fmtTime(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleTimeString(bcp(locale), { hour: '2-digit', minute: '2-digit' });
  } catch {
    return '';
  }
}

function fmtDayMonth(d: Date, locale: string): string {
  return d.toLocaleDateString(bcp(locale), { day: 'numeric', month: 'short' });
}

function weekdayShort(d: Date, locale: string): string {
  return d.toLocaleDateString(bcp(locale), { weekday: 'short' });
}

function monthYear(d: Date, locale: string): string {
  const s = d.toLocaleDateString(bcp(locale), { month: 'long', year: 'numeric' });
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function rangeLabel(start: Date, end: Date, locale: string): string {
  if (start.getMonth() === end.getMonth()) {
    return `${start.getDate()}–${fmtDayMonth(end, locale)} ${end.getFullYear()}`;
  }
  return `${fmtDayMonth(start, locale)} – ${fmtDayMonth(end, locale)}`;
}

function speakersOf(c: Call, map: Map<string, string[]>): string[] {
  const s = map.get(c.id);
  return s && s.length > 0 ? s : inferSpeakers(c);
}

// ── month-picker dropdown body (year nav + 3-col month grid) ──
function MonthPicker({
  curYear,
  curMonth,
  onPickMonth,
  locale,
  t,
}: {
  curYear: number;
  curMonth: number;
  onPickMonth: (year: number, month: number) => void;
  locale: string;
  t: TFn;
}) {
  // Local year-nav state. The Dropdown unmounts its body when closed, so this
  // re-inits to the active year on each open — no effect-based resync needed
  // (which would otherwise snap the user's in-picker year navigation back).
  const [py, setPy] = useState(curYear);
  const months = useMemo(
    () =>
      Array.from({ length: 12 }, (_, i) => {
        const s = new Date(2024, i, 1).toLocaleDateString(bcp(locale), { month: 'short' });
        return s.charAt(0).toUpperCase() + s.slice(1);
      }),
    [locale],
  );
  return (
    <>
      {/* Year nav — stopPropagation so the Dropdown stays open on ◀/▶. */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '4px 6px 8px',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <IconBtn
          icon="chevronLeft"
          label={t('inbox.yearPrev')}
          size="sm"
          onClick={() => setPy((y) => y - 1)}
        />
        <span className="mono" style={{ fontWeight: 600 }}>
          {py}
        </span>
        <IconBtn
          icon="chevronRight"
          label={t('inbox.yearNext')}
          size="sm"
          onClick={() => setPy((y) => y + 1)}
        />
      </div>
      <div
        style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 4, padding: '0 6px 6px' }}
      >
        {months.map((m, i) => {
          const on = py === curYear && i === curMonth;
          return (
            <button
              key={m}
              type="button"
              aria-pressed={on}
              onClick={() => onPickMonth(py, i)}
              style={{
                height: 30,
                borderRadius: 'var(--r-sm)',
                fontSize: 12.5,
                fontWeight: 550,
                border: 'none',
                cursor: 'pointer',
                background: on ? 'var(--accent)' : 'transparent',
                color: on ? 'var(--on-accent)' : 'var(--text-2)',
              }}
            >
              {m}
            </button>
          );
        })}
      </div>
    </>
  );
}

// ── shared calendar header (label = month-picker dropdown) ──
function CalHeader({
  label,
  onPrev,
  onNext,
  onToday,
  curYear,
  curMonth,
  onPickMonth,
  locale,
  t,
}: {
  label: string;
  onPrev: () => void;
  onNext: () => void;
  onToday: () => void;
  curYear: number;
  curMonth: number;
  onPickMonth: (year: number, month: number) => void;
  locale: string;
  t: TFn;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 14 }}>
      <IconBtn icon="chevronLeft" label={t('inbox.calPrev')} size="sm" iconSize={16} onClick={onPrev} />
      <Dropdown
        width={244}
        trigger={({ toggle }) => (
          <button
            type="button"
            className="btn btn--ghost"
            data-size="sm"
            onClick={toggle}
            style={{ fontWeight: 650, fontSize: 15, gap: 6 }}
          >
            <span className="mono">{label}</span>
            <Icon name="chevronDown" size={14} style={{ color: 'var(--text-faint)' }} />
          </button>
        )}
      >
        <MonthPicker
          curYear={curYear}
          curMonth={curMonth}
          onPickMonth={onPickMonth}
          locale={locale}
          t={t}
        />
      </Dropdown>
      <IconBtn icon="chevronRight" label={t('inbox.calNext')} size="sm" iconSize={16} onClick={onNext} />
      <div style={{ flex: 1 }} />
      <button type="button" className="btn btn--default" data-size="sm" onClick={onToday}>
        <Icon name="calendar" size={14} />
        {t('inbox.todayBtn')}
      </button>
    </div>
  );
}

// ── Карточки ──
export function InboxCards({ calls, onOpen, speakerInitials, locale, t }: ViewProps) {
  if (!calls.length) {
    return <Empty title={t('calls.notFoundTitle')} description={t('calls.notFoundBody')} />;
  }
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(252px, 1fr))',
        gap: 12,
        alignContent: 'start',
        padding: 'var(--s5)',
      }}
    >
      {calls.map((c) => (
        <button
          key={c.id}
          type="button"
          className="panel panel--raised"
          onClick={() => onOpen(c.id)}
          style={{ padding: 13, cursor: 'pointer', display: 'flex', flexDirection: 'column', gap: 11, textAlign: 'left' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <StatusCell call={c} />
            <span className="u-trunc" style={{ flex: 1, fontWeight: 600, fontSize: 14 }}>
              {c.title ?? c.id.slice(0, 8)}
            </span>
            {callHasRecap(c) && (
              <Icon name="sparkle" size={13} style={{ color: 'var(--text-faint)' }} />
            )}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <AvatarGroup list={speakersOf(c, speakerInitials)} />
            <span className="u-faint mono" style={{ fontSize: 12 }}>
              {formatDuration(c.duration_sec)}
            </span>
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 6,
              borderTop: '1px solid var(--border)',
              paddingTop: 9,
            }}
          >
            <span className="u-faint" style={{ fontSize: 12 }}>
              {fmtDayMonth(new Date(c.started_at), locale)} · {fmtTime(c.started_at, locale)}
            </span>
            {c.status === 'processing' && (
              <span className="chip chip--accent" data-size="sm">
                {t('inbox.statusProcessing')}
              </span>
            )}
            {c.status === 'failed' && (
              <span className="chip chip--danger" data-size="sm">
                {t('inbox.statusError')}
              </span>
            )}
          </div>
        </button>
      ))}
    </div>
  );
}

// ── Неделя — Outlook-style time-grid ─────────────────────────────────────
// [UI-fix C] Часовая ось слева, события позиционируются по времени начала,
// высота — по длительности (мин. слот 40 мин). Перекрытия делят ширину
// колонки поровну внутри кластера (weekGrid.packDayEvents). Sticky —
// только строка дней; высокая сетка скроллится родительским .scroll.
export function InboxWeek({ calls, onOpen, locale, t }: ViewProps) {
  const [off, setOff] = useState(0);
  const today = new Date(); // recomputed per render — stays correct past midnight
  const base = mondayOf(today);
  const start = new Date(base);
  start.setDate(base.getDate() + off * 7);
  const days = Array.from({ length: 7 }, (_, i) => {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    return d;
  });
  const onDay = (d: Date) =>
    calls
      .filter((c) => sameDay(new Date(c.started_at), d))
      .sort((a, b) => +new Date(a.started_at) - +new Date(b.started_at));

  // Единый часовой диапазон на всю видимую неделю — общий gutter.
  const weekCalls = days.map((d) => onDay(d));
  const allEvents = weekCalls.flat().map(callToDayEvent);
  const { startHour, endHour } = hourRange(allEvents);
  const gridH = (endHour - startHour) * HOUR_PX;
  const hours = Array.from({ length: endHour - startHour }, (_, i) => startHour + i);
  const fmtHour = (h: number) =>
    new Date(2024, 0, 1, h).toLocaleTimeString(bcp(locale), { hour: '2-digit', minute: '2-digit' });

  return (
    <div className="cal-week">
      <CalHeader
        label={rangeLabel(start, days[6]!, locale)}
        onPrev={() => setOff((o) => o - 1)}
        onNext={() => setOff((o) => o + 1)}
        onToday={() => setOff(0)}
        curYear={start.getFullYear()}
        curMonth={start.getMonth()}
        onPickMonth={(y, m) => {
          const wk = mondayOf(new Date(y, m, 1));
          setOff(Math.round((+wk - +base) / 604800000));
        }}
        locale={locale}
        t={t}
      />
      {/* Строка дней — sticky при скролле сетки. */}
      <div className="cal-week-head">
        <span aria-hidden="true" />
        {days.map((d) => {
          const isT = sameDay(d, today);
          return (
            <div key={d.toISOString()} className={`cal-week-day${isT ? ' is-today' : ''}`}>
              <div
                className="u-faint"
                style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: '.04em', fontWeight: 700 }}
              >
                {weekdayShort(d, locale)}
              </div>
              <div className="cal-week-daynum">{d.getDate()}</div>
            </div>
          );
        })}
      </div>
      <div className="cal-week-grid" style={{ height: gridH }}>
        {/* Часовой gutter — только визуальная шкала. */}
        <div className="cal-hour-gutter" aria-hidden="true">
          {hours.map((h) => (
            <span key={h} className="cal-hour-label" style={{ top: (h - startHour) * HOUR_PX }}>
              {fmtHour(h)}
            </span>
          ))}
        </div>
        {days.map((d, di) => {
          const isT = sameDay(d, today);
          const positioned = packDayEvents(weekCalls[di]!.map(callToDayEvent));
          const byId = new Map(positioned.map((p) => [p.id, p]));
          return (
            <div key={d.toISOString()} className={`cal-week-col${isT ? ' is-today' : ''}`}>
              {weekCalls[di]!.map((c) => {
                const p = byId.get(c.id);
                if (!p) return null;
                const top = ((p.startMin - startHour * 60) / 60) * HOUR_PX;
                const height = ((p.effEndMin - p.startMin) / 60) * HOUR_PX - 2;
                const laneW = 100 / p.laneCount;
                const compact = height < 44;
                return (
                  <button
                    key={c.id}
                    type="button"
                    className={`cal-event${compact ? ' cal-event--compact' : ''}`}
                    onClick={() => onOpen(c.id)}
                    style={{
                      top,
                      height,
                      left: `calc(${p.laneIdx * laneW}% + 2px)`,
                      width: `calc(${laneW}% - 4px)`,
                      borderLeftColor: statusColor(deriveCallState(c)),
                    }}
                  >
                    <span className="cal-event-time mono">{fmtTime(c.started_at, locale)}</span>
                    <span className="cal-event-title u-trunc">{c.title ?? c.id.slice(0, 8)}</span>
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Месяц ──
export function InboxMonth({ calls, onOpen, locale, t }: ViewProps) {
  const [off, setOff] = useState(0);
  const today = new Date(); // recomputed per render — stays correct past midnight
  const cur = new Date(today.getFullYear(), today.getMonth() + off, 1);
  const year = cur.getFullYear();
  const month = cur.getMonth();
  const startWd = (cur.getDay() + 6) % 7;
  const daysIn = new Date(year, month + 1, 0).getDate();
  const cells: (number | null)[] = [];
  for (let i = 0; i < startWd; i++) cells.push(null);
  for (let d = 1; d <= daysIn; d++) cells.push(d);
  while (cells.length % 7) cells.push(null);
  const onDay = (d: number) =>
    calls.filter((c) => {
      const dt = new Date(c.started_at);
      return dt.getFullYear() === year && dt.getMonth() === month && dt.getDate() === d;
    });
  // Monday-first weekday header (2024-01-01 is a Monday).
  const wdHeader = Array.from({ length: 7 }, (_, i) => weekdayShort(new Date(2024, 0, 1 + i), locale));

  return (
    // [UI-fix B] padding: паритет с InboxCards и прототипом wk-inbox.jsx —
    // порт потерял его, header/grid прижимались к краям.
    <div style={{ display: 'flex', flexDirection: 'column', padding: 'var(--s5)' }}>
      <CalHeader
        label={monthYear(cur, locale)}
        onPrev={() => setOff((o) => o - 1)}
        onNext={() => setOff((o) => o + 1)}
        onToday={() => setOff(0)}
        curYear={year}
        curMonth={month}
        onPickMonth={(y, m) => setOff((y - today.getFullYear()) * 12 + (m - today.getMonth()))}
        locale={locale}
        t={t}
      />
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7, 1fr)', marginBottom: 6 }}>
        {wdHeader.map((w, i) => (
          <div
            key={i}
            className="u-faint"
            style={{ fontSize: 10.5, fontWeight: 700, textTransform: 'uppercase', textAlign: 'center' }}
          >
            {w}
          </div>
        ))}
      </div>
      <div
        style={{
          minHeight: 'min(64vh, 560px)',
          display: 'grid',
          gridTemplateColumns: 'repeat(7, 1fr)',
          gridAutoRows: '1fr',
          gap: 1,
          background: 'var(--border)',
          border: '1px solid var(--border)',
          borderRadius: 'var(--r-md)',
          overflow: 'hidden',
        }}
      >
        {cells.map((d, i) => {
          const list = d ? onDay(d) : [];
          const isT = d != null && sameDay(new Date(year, month, d), today);
          return (
            <div
              key={i}
              style={{
                background: 'var(--panel)',
                padding: 5,
                minWidth: 0,
                display: 'flex',
                flexDirection: 'column',
                gap: 3,
                opacity: d ? 1 : 0.4,
              }}
            >
              {d && (
                <div
                  style={{
                    alignSelf: 'flex-start',
                    minWidth: 20,
                    height: 20,
                    padding: '0 5px',
                    borderRadius: 10,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: 11.5,
                    fontWeight: 600,
                    background: isT ? 'var(--accent)' : 'transparent',
                    color: isT ? 'var(--on-accent)' : 'var(--text-2)',
                  }}
                >
                  {d}
                </div>
              )}
              {list.slice(0, 3).map((c) => (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => onOpen(c.id)}
                  className="u-trunc"
                  style={{
                    textAlign: 'left',
                    fontSize: 10.5,
                    fontWeight: 550,
                    padding: '2px 5px',
                    borderRadius: 4,
                    cursor: 'pointer',
                    background: 'var(--accent-soft)',
                    color: 'var(--accent-text)',
                    borderLeft: `2px solid ${statusColor(deriveCallState(c))}`,
                  }}
                >
                  {c.title ?? c.id.slice(0, 8)}
                </button>
              ))}
              {list.length > 3 && (
                <div className="u-faint" style={{ fontSize: 10, paddingLeft: 5 }}>
                  +{list.length - 3}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
