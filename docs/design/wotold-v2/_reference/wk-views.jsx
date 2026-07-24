/* eslint-disable */
// WOTOLD · Inbox views — Список · Карточки · Неделя · Месяц (Apple-style)
const { useState: uSv } = React;

const VIEW_OPTS = [
  { value: 'list', label: 'Список', icon: 'list' },
  { value: 'cards', label: 'Карточки', icon: 'grid' },
  { value: 'week', label: 'Неделя', icon: 'calendarWeek' },
  { value: 'month', label: 'Месяц', icon: 'calendar' },
];

// frame chrome with the view switcher
function ViewFrame({ active, children, note }) {
  return (
    <div style={{ background: 'var(--bg)', borderRadius: 'var(--r-md)', padding: 16, height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
        <div className="seg">
          {VIEW_OPTS.map((o) => (
            <button key={o.value} data-active={o.value === active}><Icon name={o.icon} size={14} />{o.label}</button>
          ))}
        </div>
      </div>
      {note && <p className="u-faint" style={{ fontSize: 11.5, margin: '4px 2px 12px' }}>{note}</p>}
      <div style={{ flex: 1, minHeight: 0 }}>{children}</div>
    </div>
  );
}

// ── Список ──
function ViewList() {
  return (
    <ViewFrame active="list" note="Плотная таблица. Лучшее для сканирования и массовых действий.">
      <div>
        {WK_CALLS.map((c) => (
          <div key={c.id} className="lrow" style={{ padding: '8px', cursor: 'pointer' }}>
            <StatusDot status={c.status} />
            <span className="u-trunc" style={{ flex: 1, minWidth: 0, fontWeight: 550, fontSize: 13 }}>{c.title}</span>
            <AvatarGroup items={partsOf(c)} size={18} max={3} on="bg" />
            <span className="u-faint mono" style={{ fontSize: 11, width: 38, textAlign: 'right' }}>{fmtDur(c.dur)}</span>
            <span className="u-faint" style={{ fontSize: 11, width: 64, textAlign: 'right' }}>{fmtDay(c.when)}</span>
          </div>
        ))}
      </div>
    </ViewFrame>
  );
}

// ── Карточки ──
function ViewCards() {
  return (
    <ViewFrame active="cards" note="Визуально, с превью участников. Лучшее для просмотра богатых звонков.">
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
        {WK_CALLS.slice(0, 6).map((c) => (
          <div key={c.id} className="panel panel--raised" style={{ padding: 12, cursor: 'pointer', display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
              <StatusDot status={c.status} />
              <span className="u-trunc" style={{ flex: 1, fontWeight: 600, fontSize: 13 }}>{c.title}</span>
              {c.recap && <Icon name="sparkle" size={13} style={{ color: 'var(--text-faint)' }} />}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <AvatarGroup items={partsOf(c)} size={22} max={4} on="bg" />
              <span className="u-faint mono" style={{ fontSize: 11 }}>{fmtDur(c.dur)}</span>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6 }}>
              <span className="u-faint" style={{ fontSize: 11 }}>{fmtDay(c.when)}</span>
              {c.status === 'processing' && <Chip size="sm" tone="accent">обработка</Chip>}
              {c.status === 'error' && <Chip size="sm" tone="danger">ошибка</Chip>}
            </div>
          </div>
        ))}
      </div>
    </ViewFrame>
  );
}

// ── Неделя (Apple) ──
const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
function ViewWeek() {
  // week of Jun 15–21 2026 (Mon–Sun), where the data lives
  const days = Array.from({ length: 7 }, (_, i) => new Date(2026, 5, 15 + i));
  const today = 21;
  const callsOn = (d) => WK_CALLS.filter((c) => { const dt = new Date(c.when); return dt.getMonth() === 5 && dt.getDate() === d.getDate(); })
    .sort((a, b) => new Date(a.when) - new Date(b.when));
  return (
    <ViewFrame active="week" note="Звонки по дням недели. Видно ритм рабочей недели — как в Apple Calendar.">
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', gap: 6, height: '100%' }}>
        {days.map((d) => {
          const isToday = d.getDate() === today;
          return (
            <div key={d.getDate()} style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
              <div style={{ textAlign: 'center', paddingBottom: 8, borderBottom: '1px solid var(--border)', marginBottom: 8 }}>
                <div className="u-faint" style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: '.04em', fontWeight: 700 }}>{WD[d.getDay() === 0 ? 6 : d.getDay() - 1]}</div>
                <div style={{ width: 26, height: 26, margin: '4px auto 0', borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontWeight: 600, fontSize: 13, background: isToday ? 'var(--accent)' : 'transparent', color: isToday ? 'var(--on-accent)' : 'var(--text)' }}>{d.getDate()}</div>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                {callsOn(d).map((c) => (
                  <div key={c.id} style={{ borderRadius: 6, padding: '5px 6px', cursor: 'pointer', background: 'var(--panel)', borderLeft: `2.5px solid ${STATUS_COLOR[c.status]}`, boxShadow: 'var(--shadow-sm)' }}>
                    <div className="mono u-faint" style={{ fontSize: 9.5 }}>{fmtTime(c.when)}</div>
                    <div className="u-trunc" style={{ fontSize: 11, fontWeight: 550, lineHeight: 1.25 }}>{c.title}</div>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </ViewFrame>
  );
}

// ── Месяц (Apple) ──
function ViewMonth() {
  const year = 2026, month = 5; // June
  const first = new Date(year, month, 1);
  const startWd = (first.getDay() + 6) % 7; // Mon=0
  const daysIn = new Date(year, month + 1, 0).getDate();
  const today = 21;
  const cells = [];
  for (let i = 0; i < startWd; i++) cells.push(null);
  for (let d = 1; d <= daysIn; d++) cells.push(d);
  while (cells.length % 7) cells.push(null);
  const callsOn = (d) => WK_CALLS.filter((c) => { const dt = new Date(c.when); return dt.getMonth() === month && dt.getDate() === d; });

  return (
    <ViewFrame active="month" note="Месяц целиком. Лучшее для планирования и поиска «когда был тот звонок».">
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', marginBottom: 4 }}>
        {WD.map((w) => <div key={w} className="u-faint" style={{ fontSize: 10, fontWeight: 700, textTransform: 'uppercase', textAlign: 'center', padding: '2px 0' }}>{w}</div>)}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', gridAutoRows: '1fr', gap: 1, background: 'var(--border)', border: '1px solid var(--border)', borderRadius: 8, overflow: 'hidden', height: 'calc(100% - 20px)' }}>
        {cells.map((d, i) => {
          const list = d ? callsOn(d) : [];
          const isToday = d === today;
          return (
            <div key={i} style={{ background: 'var(--panel)', padding: 4, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2, opacity: d ? 1 : .4 }}>
              {d && <div style={{ alignSelf: 'flex-start', minWidth: 18, height: 18, padding: '0 4px', borderRadius: 9, display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 11, fontWeight: 600, background: isToday ? 'var(--accent)' : 'transparent', color: isToday ? 'var(--on-accent)' : 'var(--text-2)' }}>{d}</div>}
              {list.slice(0, 2).map((c) => (
                <div key={c.id} className="u-trunc" style={{ fontSize: 9.5, fontWeight: 550, padding: '1px 4px', borderRadius: 4, cursor: 'pointer',
                  background: 'var(--accent-soft)', color: 'var(--accent-text)', borderLeft: `2px solid ${STATUS_COLOR[c.status]}` }}>{c.title}</div>
              ))}
              {list.length > 2 && <div className="u-faint" style={{ fontSize: 9, paddingLeft: 4 }}>+{list.length - 2}</div>}
            </div>
          );
        })}
      </div>
    </ViewFrame>
  );
}

Object.assign(window, { ViewList, ViewCards, ViewWeek, ViewMonth, VIEW_OPTS });
