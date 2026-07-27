/* eslint-disable */
// WOTOLD · integrated Inbox — omni-bar + facet button (same tokens),
// view switcher, and Карточки/Неделя/Месяц renderers for the main area.
const { useState: uI, useRef: uIr, useEffect: uIe } = React;

const I_STATUS_COLOR = { ready: 'var(--ok)', processing: 'var(--accent)', error: 'var(--danger)' };

// facets — engine intentionally omitted (local-first; processing location de-emphasized)
const I_FACETS = [
  { key: 'status', label: 'Статус', icon: 'bolt', values: [{ v: 'ready', l: 'Готово' }, { v: 'processing', l: 'Обработка' }, { v: 'error', l: 'Ошибка' }] },
  { key: 'recap', label: 'Рекап', icon: 'sparkle', values: [{ v: 'yes', l: 'С рекапом' }, { v: 'no', l: 'Без рекапа' }] },
  { key: 'person', label: 'Участник', icon: 'user', values: WK_CONTACTS.map((c) => ({ v: c.sp, l: c.name.split(' ')[0] })) },
  { key: 'period', label: 'Период', icon: 'calendar', values: [{ v: 'today', l: 'Сегодня' }, { v: 'week', l: 'Эта неделя' }] },
];
const I_EMPTY = { status: [], recap: [], person: [], period: [] };
const iFacetLabel = (k) => I_FACETS.find((f) => f.key === k).label;
const iFacetIcon = (k) => I_FACETS.find((f) => f.key === k).icon;
const iValueLabel = (k, v) => { const f = I_FACETS.find((x) => x.key === k); const it = f.values.find((x) => x.v === v); return it ? it.l : v; };
const iCount = (f) => Object.values(f).reduce((n, a) => n + a.length, 0);
const iToggle = (f, k, v) => ({ ...f, [k]: f[k].includes(v) ? f[k].filter((x) => x !== v) : [...f[k], v] });
function iMatch(c, f, text) {
  if (f.status.length && !f.status.includes(c.status)) return false;
  if (f.recap.length && !f.recap.includes(c.recap ? 'yes' : 'no')) return false;
  if (f.person.length && !f.person.some((p) => c.parts.includes(p))) return false;
  if (f.period.length) {
    const rd = relDay(c.when); let ok = false;
    if (f.period.includes('today') && rd === 'Сегодня') ok = true;
    if (f.period.includes('week') && ['Сегодня', 'Вчера', 'На этой неделе'].includes(rd)) ok = true;
    if (!ok) return false;
  }
  if (text && !c.title.toLowerCase().includes(text.toLowerCase())) return false;
  return true;
}

// ── Omni-bar (default) — text + typed filter tokens ──
function OmniBar({ f, setF, text, setText }) {
  const [draft, setDraft] = uI('');
  const [focus, setFocus] = uI(false);
  const tokens = [];
  Object.entries(f).forEach(([k, arr]) => arr.forEach((v) => tokens.push({ k, v })));

  const allTok = [];
  I_FACETS.forEach((fc) => fc.values.forEach((val) => allTok.push({ k: fc.key, v: val.v, label: val.l, fl: fc.label, icon: fc.icon })));
  const q = draft.trim().toLowerCase();
  const sugg = (q ? allTok.filter((t) => t.label.toLowerCase().includes(q) || t.fl.toLowerCase().includes(q)) : allTok)
    .filter((t) => !f[t.k].includes(t.v)).slice(0, 5);
  const add = (t) => { setF((p) => iToggle(p, t.k, t.v)); setDraft(''); };
  const rm = (k, v) => setF((p) => iToggle(p, k, v));

  return (
    <div className="omni" data-focus={focus ? 'true' : undefined}>
      <Icon name="search" size={15} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />
      <div className="omni-row">
        {tokens.map((t) => (
          <span key={t.k + t.v} className="chip chip--accent" style={{ gap: 4, flex: '0 0 auto' }}>
            <Icon name={iFacetIcon(t.k)} size={11} />{iValueLabel(t.k, t.v)}
            <button onMouseDown={(e) => { e.preventDefault(); rm(t.k, t.v); }} style={{ display: 'inline-flex', color: 'inherit' }}><Icon name="x" size={11} /></button>
          </span>
        ))}
        {text && <span className="chip chip--line" style={{ gap: 4, flex: '0 0 auto' }}>«{text}»
          <button onMouseDown={(e) => { e.preventDefault(); setText(''); }} style={{ display: 'inline-flex', color: 'inherit' }}><Icon name="x" size={11} /></button></span>}
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder={tokens.length || text ? '' : 'Поиск или фильтр…'}
          onFocus={() => setFocus(true)} onBlur={() => setTimeout(() => setFocus(false), 160)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { if (q && sugg[0]) add(sugg[0]); else if (draft.trim()) { setText(draft.trim()); setDraft(''); } }
            if (e.key === 'Backspace' && !draft) { if (text) setText(''); else if (tokens.length) rm(tokens[tokens.length - 1].k, tokens[tokens.length - 1].v); }
          }} />
      </div>
      {(tokens.length || text) > 0 && <button className="iconbtn" data-size="sm" onMouseDown={(e) => { e.preventDefault(); setF({ ...I_EMPTY }); setText(''); }} aria-label="Сбросить" style={{ flex: '0 0 auto' }}><Icon name="x" size={14} /></button>}
      {focus && sugg.length > 0 && (
        <div className="menu" style={{ left: 0, right: 0, top: 'calc(100% + 5px)', width: 'auto' }}>
          <MenuLabel>{q ? 'Добавить фильтр' : 'Быстрые фильтры'}</MenuLabel>
          {sugg.map((t) => (
            <button key={t.k + t.v} className="menu-item" onMouseDown={(e) => { e.preventDefault(); add(t); }}>
              <span className="mi-ico"><Icon name={t.icon} size={15} /></span>
              <span style={{ flex: 1 }}><span className="u-faint">{t.fl}: </span>{t.label}</span>
            </button>
          ))}
          {q && <button className="menu-item" onMouseDown={(e) => { e.preventDefault(); setText(draft.trim()); setDraft(''); }}>
            <span className="mi-ico"><Icon name="search" size={15} /></span><span>Искать «{draft.trim()}» в названиях</span>
          </button>}
        </div>
      )}
    </div>
  );
}

// ── Facet button — same tokens via dropdown checkboxes ──
function FacetButton({ f, setF }) {
  const n = iCount(f);
  return (
    <Dropdown width={232} trigger={({ toggle }) => (
      <button className="btn btn--default" onClick={toggle} style={n ? { borderColor: 'var(--accent)', color: 'var(--accent-text)' } : null}>
        <Icon name="filter" size={14} />Фильтр{n ? ` · ${n}` : ''}
      </button>
    )}>
      {I_FACETS.map((fc, i) => (
        <React.Fragment key={fc.key}>
          {i > 0 && <MenuSep />}
          <MenuLabel>{fc.label}</MenuLabel>
          {fc.values.map((val) => {
            const on = f[fc.key].includes(val.v);
            return (
              <button key={val.v} className="menu-item" data-active={on} onClick={(e) => { e.stopPropagation(); setF((p) => iToggle(p, fc.key, val.v)); }}>
                <span className="chk" data-done={on} style={{ width: 15, height: 15 }}><Icon name="check" size={11} /></span>
                <span style={{ flex: 1 }}>{val.l}</span>
              </button>
            );
          })}
        </React.Fragment>
      ))}
      {n > 0 && <><MenuSep /><button className="menu-item" onClick={() => setF({ ...I_EMPTY })}><span className="mi-ico"><Icon name="x" size={15} /></span><span>Сбросить всё</span></button></>}
    </Dropdown>
  );
}

// ── View switcher (icon segmented) ──
const I_VIEWS = [['list', 'list', 'Список'], ['cards', 'grid', 'Карточки'], ['week', 'calendarWeek', 'Неделя'], ['month', 'calendar', 'Месяц']];
function ViewSwitcher({ view, setView }) {
  return (
    <div className="seg" role="tablist" aria-label="Представление">
      {I_VIEWS.map(([v, ic, label]) => (
        <button key={v} title={label} data-active={view === v} aria-label={label} aria-selected={view === v}
          onClick={() => setView(v)} style={{ padding: '0 9px' }}><Icon name={ic} size={15} /></button>
      ))}
    </div>
  );
}

// ── Карточки ──
function InboxCards({ calls, onOpen }) {
  if (!calls.length) return <Empty icon="search" title="Ничего не найдено" desc="Измените запрос или фильтры." />;
  return (
    <div style={{ padding: 'var(--s5)', display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(252px, 1fr))', gap: 12, alignContent: 'start' }}>
      {calls.map((c) => (
        <button key={c.id} className="panel panel--raised" onClick={() => onOpen(c.id)}
          style={{ padding: 13, cursor: 'pointer', display: 'flex', flexDirection: 'column', gap: 11, textAlign: 'left' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <StatusCell call={c} />
            <span className="u-trunc" style={{ flex: 1, fontWeight: 600, fontSize: 14 }}>{c.title}</span>
            {c.recap && <Icon name="sparkle" size={13} style={{ color: 'var(--text-faint)' }} />}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <AvatarGroup items={c.parts.map(av)} size={24} max={4} on="bg" />
            <span className="u-faint mono" style={{ fontSize: 12 }}>{fmtDur(c.dur)}</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6, borderTop: '1px solid var(--border)', paddingTop: 9 }}>
            <span className="u-faint" style={{ fontSize: 12 }}>{fmtDay(c.when)} · {fmtTime(c.when)}</span>
            {c.status === 'processing' && <Chip size="sm" tone="accent">обработка</Chip>}
            {c.status === 'error' && <Chip size="sm" tone="danger">ошибка</Chip>}
          </div>
        </button>
      ))}
    </div>
  );
}

// ── Неделя (Apple-style) ──
const I_WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
const I_MON_NOM = ['Январь', 'Февраль', 'Март', 'Апрель', 'Май', 'Июнь', 'Июль', 'Август', 'Сентябрь', 'Октябрь', 'Ноябрь', 'Декабрь'];
const I_MON_GEN = ['января', 'февраля', 'марта', 'апреля', 'мая', 'июня', 'июля', 'августа', 'сентября', 'октября', 'ноября', 'декабря'];
const I_MON_SHORT = ['Янв', 'Фев', 'Мар', 'Апр', 'Май', 'Июн', 'Июл', 'Авг', 'Сен', 'Окт', 'Ноя', 'Дек'];
const I_TODAY = new Date(2026, 5, 21);
const sameDay = (a, b) => a.toDateString() === b.toDateString();
const mondayOf = (d) => { const m = new Date(d); m.setDate(d.getDate() - ((d.getDay() + 6) % 7)); m.setHours(0, 0, 0, 0); return m; };

// shared calendar header: ‹ · period selector (month/year) · › · Сегодня
function CalHeader({ label, onPrev, onNext, onToday, curYear, curMonth, onPickMonth }) {
  const [py, setPy] = uI(curYear);
  uIe(() => { setPy(curYear); }, [curYear, curMonth]);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 14 }}>
      <IconBtn icon="chevronLeft" size="sm" tip="Назад" label="Назад" onClick={onPrev} />
      <Dropdown width={244} trigger={({ toggle }) => (
        <button className="btn btn--ghost" onClick={toggle} style={{ fontWeight: 650, fontSize: 15, gap: 6, height: 30 }}>
          {label}<Icon name="chevronDown" size={14} style={{ color: 'var(--text-faint)' }} />
        </button>
      )}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '4px 6px 8px' }} onClick={(e) => e.stopPropagation()}>
          <IconBtn icon="chevronLeft" size="sm" label="Год назад" onClick={() => setPy((y) => y - 1)} />
          <span className="mono" style={{ fontWeight: 600 }}>{py}</span>
          <IconBtn icon="chevronRight" size="sm" label="Год вперёд" onClick={() => setPy((y) => y + 1)} />
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 4, padding: '0 6px 6px' }}>
          {I_MON_SHORT.map((m, i) => {
            const on = py === curYear && i === curMonth;
            return (
              <button key={m} onClick={() => onPickMonth(py, i)} style={{
                height: 30, borderRadius: 'var(--r-sm)', fontSize: 12.5, fontWeight: 550, border: 'none',
                background: on ? 'var(--accent)' : 'transparent', color: on ? 'var(--on-accent)' : 'var(--text-2)' }}
                onMouseEnter={(e) => { if (!on) e.currentTarget.style.background = 'var(--hover)'; }}
                onMouseLeave={(e) => { if (!on) e.currentTarget.style.background = 'transparent'; }}>{m}</button>
            );
          })}
        </div>
      </Dropdown>
      <IconBtn icon="chevronRight" size="sm" tip="Вперёд" label="Вперёд" onClick={onNext} />
      <div style={{ flex: 1 }} />
      <Btn variant="default" size="sm" icon="calendar" onClick={onToday}>Сегодня</Btn>
    </div>
  );
}

// ── Неделя (Apple-style) ──
function InboxWeek({ calls, onOpen }) {
  const [off, setOff] = uI(0);
  const base = mondayOf(I_TODAY);
  const start = new Date(base); start.setDate(base.getDate() + off * 7);
  const days = Array.from({ length: 7 }, (_, i) => { const d = new Date(start); d.setDate(start.getDate() + i); return d; });
  const last = days[6];
  const on = (d) => calls.filter((c) => sameDay(new Date(c.when), d)).sort((a, b) => new Date(a.when) - new Date(b.when));
  const label = start.getMonth() === last.getMonth()
    ? `${start.getDate()}–${last.getDate()} ${I_MON_GEN[start.getMonth()]} ${last.getFullYear()}`
    : `${start.getDate()} ${I_MON_GEN[start.getMonth()]} – ${last.getDate()} ${I_MON_GEN[last.getMonth()]}`;
  const pickMonth = (y, m) => { const wk = mondayOf(new Date(y, m, 1)); setOff(Math.round((wk - base) / 604800000)); };
  return (
    <div style={{ padding: 'var(--s5)', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <CalHeader label={label} onPrev={() => setOff((o) => o - 1)} onNext={() => setOff((o) => o + 1)} onToday={() => setOff(0)}
        curYear={start.getFullYear()} curMonth={start.getMonth()} onPickMonth={pickMonth} />
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', gap: 8, flex: 1, minHeight: 0 }}>
        {days.map((d) => {
          const isT = sameDay(d, I_TODAY);
          return (
            <div key={d.toISOString()} style={{ display: 'flex', flexDirection: 'column', minWidth: 0, borderRadius: 'var(--r)', background: isT ? 'var(--accent-soft)' : 'transparent', padding: 4 }}>
              <div style={{ textAlign: 'center', paddingBottom: 8, borderBottom: '1px solid var(--border)', marginBottom: 8 }}>
                <div className="u-faint" style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: '.04em', fontWeight: 700 }}>{I_WD[d.getDay() === 0 ? 6 : d.getDay() - 1]}</div>
                <div style={{ width: 28, height: 28, margin: '5px auto 0', borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center', fontWeight: 600, fontSize: 14, background: isT ? 'var(--accent)' : 'transparent', color: isT ? 'var(--on-accent)' : 'var(--text)' }}>{d.getDate()}</div>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
                {on(d).map((c) => (
                  <button key={c.id} onClick={() => onOpen(c.id)} style={{ textAlign: 'left', borderRadius: 7, padding: '6px 7px', cursor: 'pointer', background: 'var(--panel)', borderLeft: `2.5px solid ${I_STATUS_COLOR[c.status]}`, boxShadow: 'var(--shadow-sm)' }}>
                    <div className="mono u-faint" style={{ fontSize: 10 }}>{fmtTime(c.when)}</div>
                    <div className="u-trunc" style={{ fontSize: 12, fontWeight: 550, lineHeight: 1.3 }}>{c.title}</div>
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

// ── Месяц (Apple-style) ──
function InboxMonth({ calls, onOpen }) {
  const [off, setOff] = uI(0);
  const cur = new Date(2026, 5 + off, 1);
  const year = cur.getFullYear(), month = cur.getMonth();
  const startWd = (cur.getDay() + 6) % 7;
  const daysIn = new Date(year, month + 1, 0).getDate();
  const cells = [];
  for (let i = 0; i < startWd; i++) cells.push(null);
  for (let d = 1; d <= daysIn; d++) cells.push(d);
  while (cells.length % 7) cells.push(null);
  const on = (d) => calls.filter((c) => { const dt = new Date(c.when); return dt.getFullYear() === year && dt.getMonth() === month && dt.getDate() === d; });
  const pickMonth = (y, m) => setOff((y - 2026) * 12 + (m - 5));
  return (
    <div style={{ padding: 'var(--s5)', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <CalHeader label={`${I_MON_NOM[month]} ${year}`} onPrev={() => setOff((o) => o - 1)} onNext={() => setOff((o) => o + 1)} onToday={() => setOff(0)}
        curYear={year} curMonth={month} onPickMonth={pickMonth} />
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', marginBottom: 6 }}>
        {I_WD.map((w) => <div key={w} className="u-faint" style={{ fontSize: 10.5, fontWeight: 700, textTransform: 'uppercase', textAlign: 'center' }}>{w}</div>)}
      </div>
      <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: 'repeat(7,1fr)', gridAutoRows: '1fr', gap: 1, background: 'var(--border)', border: '1px solid var(--border)', borderRadius: 10, overflow: 'hidden' }}>
        {cells.map((d, i) => {
          const list = d ? on(d) : [];
          const isT = d && sameDay(new Date(year, month, d), I_TODAY);
          return (
            <div key={i} style={{ background: 'var(--panel)', padding: 5, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 3, opacity: d ? 1 : .4 }}>
              {d && <div style={{ alignSelf: 'flex-start', minWidth: 20, height: 20, padding: '0 5px', borderRadius: 10, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 11.5, fontWeight: 600, background: isT ? 'var(--accent)' : 'transparent', color: isT ? 'var(--on-accent)' : 'var(--text-2)' }}>{d}</div>}
              {list.slice(0, 3).map((c) => (
                <button key={c.id} onClick={() => onOpen(c.id)} className="u-trunc" style={{ textAlign: 'left', fontSize: 10.5, fontWeight: 550, padding: '2px 5px', borderRadius: 4, cursor: 'pointer', background: 'var(--accent-soft)', color: 'var(--accent-text)', borderLeft: `2px solid ${I_STATUS_COLOR[c.status]}` }}>{c.title}</button>
              ))}
              {list.length > 3 && <div className="u-faint" style={{ fontSize: 10, paddingLeft: 5 }}>+{list.length - 3}</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

Object.assign(window, { I_FACETS, I_EMPTY, iMatch, iCount, OmniBar, FacetButton, ViewSwitcher, InboxCards, InboxWeek, InboxMonth });
