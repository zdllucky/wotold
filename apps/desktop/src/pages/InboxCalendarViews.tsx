// [B18.2b] Inbox calendar/grid views — Карточки / Неделя / Месяц. Ports of
// ~/Downloads/Wotold v2/wk-inbox.jsx (InboxCards/InboxWeek/InboxMonth/CalHeader).
// Self-contained: render the already-filtered `calls` + speaker initials; no new
// APIs. Today = real new Date(). Month/weekday labels via Intl. CalHeader's
// month-picker Dropdown is deferred to B18.2c (kept to ‹ label › + Сегодня).

import { useMemo, useState } from 'react';
import { bcp47, useI18n } from '../i18n';
import type { Call } from '../api/recording';
import { Empty } from '../ui';
import { Icon } from '../ui/Icon';
import { callHasRecap, deriveCallState, formatDuration, inferSpeakers } from './inboxData';
import { AvatarGroup, StatusCell, statusColor } from './inboxBits';

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

// ── shared calendar header (simplified — no month-picker dropdown yet) ──
function CalHeader({
  label,
  onPrev,
  onNext,
  onToday,
  t,
}: {
  label: string;
  onPrev: () => void;
  onNext: () => void;
  onToday: () => void;
  t: TFn;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 14 }}>
      <button className="iconbtn" data-size="sm" aria-label="‹" onClick={onPrev}>
        <Icon name="chevronLeft" size={16} />
      </button>
      <span className="mono" style={{ fontWeight: 650, fontSize: 15 }}>
        {label}
      </span>
      <button className="iconbtn" data-size="sm" aria-label="›" onClick={onNext}>
        <Icon name="chevronRight" size={16} />
      </button>
      <div style={{ flex: 1 }} />
      <button className="btn btn--default" data-size="sm" onClick={onToday}>
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

// ── Неделя ──
export function InboxWeek({ calls, onOpen, locale, t }: ViewProps) {
  const [off, setOff] = useState(0);
  const today = useMemo(() => new Date(), []);
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

  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <CalHeader
        label={rangeLabel(start, days[6]!, locale)}
        onPrev={() => setOff((o) => o - 1)}
        onNext={() => setOff((o) => o + 1)}
        onToday={() => setOff(0)}
        t={t}
      />
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(7, 1fr)',
          gap: 8,
          minHeight: 'min(62vh, 540px)',
        }}
      >
        {days.map((d) => {
          const isT = sameDay(d, today);
          return (
            <div
              key={d.toISOString()}
              style={{
                display: 'flex',
                flexDirection: 'column',
                minWidth: 0,
                borderRadius: 'var(--r)',
                background: isT ? 'var(--accent-soft)' : 'transparent',
                padding: 4,
              }}
            >
              <div
                style={{
                  textAlign: 'center',
                  paddingBottom: 8,
                  borderBottom: '1px solid var(--border)',
                  marginBottom: 8,
                }}
              >
                <div
                  className="u-faint"
                  style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: '.04em', fontWeight: 700 }}
                >
                  {weekdayShort(d, locale)}
                </div>
                <div
                  style={{
                    width: 28,
                    height: 28,
                    margin: '5px auto 0',
                    borderRadius: '50%',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontWeight: 600,
                    fontSize: 14,
                    background: isT ? 'var(--accent)' : 'transparent',
                    color: isT ? 'var(--on-accent)' : 'var(--text)',
                  }}
                >
                  {d.getDate()}
                </div>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
                {onDay(d).map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    onClick={() => onOpen(c.id)}
                    style={{
                      textAlign: 'left',
                      borderRadius: 7,
                      padding: '6px 7px',
                      cursor: 'pointer',
                      background: 'var(--panel)',
                      borderLeft: `2.5px solid ${statusColor(deriveCallState(c))}`,
                      boxShadow: 'var(--shadow-sm)',
                    }}
                  >
                    <div className="mono u-faint" style={{ fontSize: 10 }}>
                      {fmtTime(c.started_at, locale)}
                    </div>
                    <div className="u-trunc" style={{ fontSize: 12, fontWeight: 550, lineHeight: 1.3 }}>
                      {c.title ?? c.id.slice(0, 8)}
                    </div>
                  </button>
                ))}
              </div>
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
  const today = useMemo(() => new Date(), []);
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
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <CalHeader
        label={monthYear(cur, locale)}
        onPrev={() => setOff((o) => o - 1)}
        onNext={() => setOff((o) => o + 1)}
        onToday={() => setOff(0)}
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
          borderRadius: 10,
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
