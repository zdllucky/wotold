/* eslint-disable */
// WOTOLD · Inbox explorations — 3 подхода к поиску/фильтрам + 4 представления.
// Composes only from the UIKit. Each frame is self-contained & interactive.
const { useState: uSe } = React;

// ── shared mini bits ──
const STATUS_COLOR = { ready: 'var(--ok)', processing: 'var(--accent)', error: 'var(--danger)' };
function EngChip({ via, size }) { const e = WK_ENGINES[via]; return <Chip icon={e.icon} tone={e.tone} size={size}>{e.label}</Chip>; }
function StatusDot({ status }) {
  if (status === 'processing') return <Dot ring pulse color="var(--accent)" />;
  if (status === 'error') return <Dot color="var(--danger)" />;
  return <Dot color="var(--ok)" />;
}
const partsOf = (c) => c.parts.map(av);

// ── facets (shared by A & B & C-secondary) ──
const FACETS = [
  { key: 'status', label: 'Статус', icon: 'bolt', values: [{ v: 'ready', l: 'Готово' }, { v: 'processing', l: 'Обработка' }, { v: 'error', l: 'Ошибка' }] },
  { key: 'recap', label: 'Рекап', icon: 'sparkle', values: [{ v: 'yes', l: 'С рекапом' }, { v: 'no', l: 'Без рекапа' }] },
  { key: 'person', label: 'Участник', icon: 'user', values: WK_CONTACTS.map((c) => ({ v: c.sp, l: c.name.split(' ')[0] })) },
  { key: 'period', label: 'Период', icon: 'calendar', values: [{ v: 'today', l: 'Сегодня' }, { v: 'week', l: 'Эта неделя' }] },
];
const EMPTY = { status: [], recap: [], person: [], period: [] };
const facetLabel = (key) => FACETS.find((f) => f.key === key).label;
const valueLabel = (key, v) => { const f = FACETS.find((x) => x.key === key); const it = f.values.find((x) => x.v === v); return it ? it.l : v; };
const facetIcon = (key) => FACETS.find((f) => f.key === key).icon;

function matchCall(c, f, text) {
  if (f.status.length && !f.status.includes(c.status)) return false;
  if (f.recap.length && !f.recap.includes(c.recap ? 'yes' : 'no')) return false;
  if (f.person.length && !f.person.some((p) => c.parts.includes(p))) return false;
  if (f.period.length) {
    const rd = relDay(c.when);
    let ok = false;
    if (f.period.includes('today') && rd === 'Сегодня') ok = true;
    if (f.period.includes('week') && ['Сегодня', 'Вчера', 'На этой неделе'].includes(rd)) ok = true;
    if (!ok) return false;
  }
  if (text && !c.title.toLowerCase().includes(text.toLowerCase())) return false;
  return true;
}
const countActive = (f) => Object.values(f).reduce((n, a) => n + a.length, 0);
const toggleTok = (f, key, v) => ({ ...f, [key]: f[key].includes(v) ? f[key].filter((x) => x !== v) : [...f[key], v] });

// ── compact result list (shared) ──
function ResultList({ calls, empty }) {
  if (!calls.length) return <Empty icon="search" title={empty || 'Ничего не найдено'} desc="Измените запрос или фильтры." />;
  return (
    <div>
      {calls.map((c) => (
        <div key={c.id} className="lrow" style={{ cursor: 'pointer', padding: '8px 8px' }}>
          <StatusDot status={c.status} />
          <span className="u-trunc" style={{ flex: 1, minWidth: 0, fontWeight: 550, fontSize: 13 }}>{c.title}</span>
          <AvatarGroup items={partsOf(c)} size={18} max={3} on="bg" />
          <span className="u-faint mono" style={{ fontSize: 11, width: 38, textAlign: 'right' }}>{fmtDur(c.dur)}</span>
          <span className="u-faint" style={{ fontSize: 11, width: 64, textAlign: 'right' }}>{fmtDay(c.when)}</span>
        </div>
      ))}
    </div>
  );
}
const ResultCount = ({ n }) => <div className="u-faint" style={{ fontSize: 11, margin: '10px 2px 4px', textTransform: 'uppercase', letterSpacing: '.05em', fontWeight: 700 }}>{n} {n === 1 ? 'звонок' : n < 5 ? 'звонка' : 'звонков'}</div>;

// ═══════════ A · ОМНИ-СТРОКА ═══════════
function VariantOmni() {
  const [f, setF] = uSe({ ...EMPTY });
  const [text, setText] = uSe('');
  const [draft, setDraft] = uSe('');
  const [focus, setFocus] = uSe(false);
  const results = WK_CALLS.filter((c) => matchCall(c, f, text));

  const tokens = [];
  Object.entries(f).forEach(([k, arr]) => arr.forEach((v) => tokens.push({ k, v })));

  const allTok = [];
  FACETS.forEach((fc) => fc.values.forEach((val) => allTok.push({ k: fc.key, v: val.v, label: val.l, fl: fc.label, icon: fc.icon })));
  const q = draft.trim().toLowerCase();
  const suggestions = q
    ? allTok.filter((t) => !f[t.k].includes(t.v) && (t.label.toLowerCase().includes(q) || t.fl.toLowerCase().includes(q))).slice(0, 5)
    : allTok.filter((t) => !f[t.k].includes(t.v)).slice(0, 5);

  const addTok = (t) => { setF((p) => toggleTok(p, t.k, t.v)); setDraft(''); };
  const rmTok = (k, v) => setF((p) => toggleTok(p, k, v));

  return (
    <ExploreFrame title="Омни-строка" tag="для скорости"
      desc="Один ввод делает всё: текст ищет по названиям, а подсказки превращаются в фильтр-токены. Клавиатура-first.">
      <div className="panel" style={{ padding: 6, position: 'relative', borderColor: focus ? 'var(--accent)' : 'var(--border-strong)', boxShadow: focus ? '0 0 0 3px var(--accent-soft)' : 'none' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
          <Icon name="search" size={15} style={{ color: 'var(--text-faint)', margin: '0 2px' }} />
          {tokens.map((t) => (
            <span key={t.k + t.v} className="chip chip--accent" style={{ gap: 4 }}>
              <Icon name={facetIcon(t.k)} size={11} />{facetLabel(t.k)}: {valueLabel(t.k, t.v)}
              <button onClick={() => rmTok(t.k, t.v)} style={{ display: 'inline-flex', marginLeft: 2, color: 'inherit' }}><Icon name="x" size={11} /></button>
            </span>
          ))}
          {text && (
            <span className="chip chip--line" style={{ gap: 4 }}>«{text}»
              <button onClick={() => setText('')} style={{ display: 'inline-flex', color: 'inherit' }}><Icon name="x" size={11} /></button>
            </span>
          )}
          <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder={tokens.length || text ? '' : 'Поиск или фильтр…'}
            onFocus={() => setFocus(true)} onBlur={() => setTimeout(() => setFocus(false), 150)}
            onKeyDown={(e) => { if (e.key === 'Enter' && draft.trim()) { if (suggestions[0] && q) addTok(suggestions[0]); else { setText(draft.trim()); setDraft(''); } } if (e.key === 'Backspace' && !draft && tokens.length) rmTok(tokens[tokens.length - 1].k, tokens[tokens.length - 1].v); }}
            style={{ flex: 1, minWidth: 90, border: 'none', outline: 'none', background: 'none', fontSize: 13, padding: '6px 2px', color: 'var(--text)' }} />
          {(tokens.length || text) > 0 && <button className="iconbtn" data-size="sm" onClick={() => { setF({ ...EMPTY }); setText(''); }} aria-label="Сбросить"><Icon name="x" size={14} /></button>}
        </div>
        {focus && suggestions.length > 0 && (
          <div className="menu" style={{ left: 6, right: 6, top: 'calc(100% + 4px)', width: 'auto' }}>
            <MenuLabel>{q ? 'Добавить фильтр' : 'Фильтры'}</MenuLabel>
            {suggestions.map((t) => (
              <button key={t.k + t.v} className="menu-item" onMouseDown={(e) => { e.preventDefault(); addTok(t); }}>
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
      <ResultCount n={results.length} />
      <ResultList calls={results} />
    </ExploreFrame>
  );
}

// ═══════════ B · ФАСЕТНАЯ ПАНЕЛЬ ═══════════
function VariantFacets() {
  const [f, setF] = uSe({ ...EMPTY });
  const [text, setText] = uSe('');
  const results = WK_CALLS.filter((c) => matchCall(c, f, text));
  const tokens = [];
  Object.entries(f).forEach(([k, arr]) => arr.forEach((v) => tokens.push({ k, v })));

  return (
    <ExploreFrame title="Фасетная панель" tag="для всех"
      desc="Явные элементы как в Notion/Linear: «+ Фильтр» раскрывает фасеты с галочками, активные собираются в съёмные пилюли. Полностью наглядно.">
      <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
        <div style={{ flex: 1 }}><Input icon="search" size="sm" placeholder="Поиск…" value={text} onChange={(e) => setText(e.target.value)} /></div>
        <Dropdown width={230} trigger={({ toggle }) => (
          <button className="btn btn--default" data-size="sm" onClick={toggle} style={countActive(f) ? { borderColor: 'var(--accent)', color: 'var(--accent-text)' } : null}>
            <Icon name="filter" size={14} />Фильтр{countActive(f) > 0 ? ` · ${countActive(f)}` : ''}
          </button>
        )}>
          {FACETS.map((fc, i) => (
            <React.Fragment key={fc.key}>
              {i > 0 && <MenuSep />}
              <MenuLabel>{fc.label}</MenuLabel>
              {fc.values.map((val) => {
                const on = f[fc.key].includes(val.v);
                return (
                  <button key={val.v} className="menu-item" data-active={on} onClick={(e) => { e.stopPropagation(); setF((p) => toggleTok(p, fc.key, val.v)); }}>
                    <span className="chk" data-done={on} style={{ width: 15, height: 15 }}><Icon name="check" size={11} /></span>
                    <span style={{ flex: 1 }}>{val.l}</span>
                  </button>
                );
              })}
            </React.Fragment>
          ))}
        </Dropdown>
        <Dropdown width={170} align="right" trigger={({ toggle }) => (
          <button className="btn btn--default" data-size="sm" onClick={toggle}><Icon name="sort" size={14} />Сортировка</button>
        )}>
          <MenuItem icon="calendar" end={<Icon name="check" size={13} />}>Сначала новые</MenuItem>
          <MenuItem icon="clock">По длительности</MenuItem>
        </Dropdown>
      </div>
      {tokens.length > 0 && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 10 }}>
          {tokens.map((t) => (
            <span key={t.k + t.v} className="chip chip--accent" style={{ gap: 4 }}>
              <Icon name={facetIcon(t.k)} size={11} />{valueLabel(t.k, t.v)}
              <button onClick={() => setF((p) => toggleTok(p, t.k, t.v))} style={{ display: 'inline-flex', color: 'inherit' }}><Icon name="x" size={11} /></button>
            </span>
          ))}
          <button className="chip chip--line" onClick={() => setF({ ...EMPTY })}>Сбросить всё</button>
        </div>
      )}
      <ResultCount n={results.length} />
      <ResultList calls={results} />
    </ExploreFrame>
  );
}

// ═══════════ C · УМНЫЕ ВИДЫ ═══════════
const SMART = [
  { id: 'all', label: 'Все', icon: 'inbox', test: () => true },
  { id: 'today', label: 'Сегодня', icon: 'calendar', test: (c) => relDay(c.when) === 'Сегодня' },
  { id: 'attn', label: 'Требуют внимания', icon: 'alert', test: (c) => c.status === 'error' || c.status === 'processing' },
  { id: 'recap', label: 'С рекапом', icon: 'sparkle', test: (c) => c.recap },
  { id: 'kontur', label: 'По «Контур»', icon: 'user', test: (c) => c.parts.includes('arman') },
];
function VariantSmart() {
  const [view, setView] = uSe('all');
  const [text, setText] = uSe('');
  const sv = SMART.find((s) => s.id === view);
  const results = WK_CALLS.filter((c) => sv.test(c) && (!text || c.title.toLowerCase().includes(text.toLowerCase())));

  return (
    <ExploreFrame title="Умные виды" tag="для рутины"
      desc="Сохранённые наборы фильтров под сценарии — как смарт-папки в Apple Mail. Частые задачи в один клик, поиск и фасеты остаются для разовых.">
      <div style={{ display: 'flex', gap: 6, alignItems: 'center', marginBottom: 10 }}>
        <div style={{ flex: 1 }}><Input icon="search" size="sm" placeholder="Поиск в виде…" value={text} onChange={(e) => setText(e.target.value)} /></div>
        <IconBtn icon="filter" size="sm" tip="Доп. фильтр" label="Фильтр" />
        <IconBtn icon="plus" size="sm" tip="Сохранить вид" label="Сохранить" />
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {SMART.map((s) => {
          const n = WK_CALLS.filter(s.test).length;
          const on = view === s.id;
          return (
            <button key={s.id} onClick={() => setView(s.id)} className="chip" style={{
              height: 28, padding: '0 10px', gap: 6,
              background: on ? 'var(--accent)' : 'var(--sunken)', color: on ? 'var(--on-accent)' : 'var(--text-2)' }}>
              <Icon name={s.icon} size={13} />{s.label}
              <span style={{ fontSize: 10.5, opacity: .8, fontFamily: 'var(--mono)' }}>{n}</span>
            </button>
          );
        })}
      </div>
      <ResultCount n={results.length} />
      <ResultList calls={results} empty="В этом виде пусто" />
    </ExploreFrame>
  );
}

// frame chrome for filter variants
function ExploreFrame({ title, tag, desc, children }) {
  return (
    <div style={{ background: 'var(--bg)', borderRadius: 'var(--r-md)', padding: 18, height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
        <span style={{ fontWeight: 700, fontSize: 15 }}>{title}</span>
        <Chip size="sm" tone="accent">{tag}</Chip>
      </div>
      <p className="u-muted" style={{ fontSize: 12, lineHeight: 1.5, margin: '0 0 14px' }}>{desc}</p>
      {children}
    </div>
  );
}

Object.assign(window, { VariantOmni, VariantFacets, VariantSmart, EngChip, StatusDot, partsOf, STATUS_COLOR });
