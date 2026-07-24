/* eslint-disable */
// WOTOLD · app shell — sidebar, recording dock, command palette, state
const { useState: uS, useEffect: uE, useRef: uR, useCallback: uC } = React;

// ── Brand mark ──
function Brand() {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 9, fontWeight: 700, fontSize: 15, letterSpacing: '-.01em' }}>
      <svg width="29" height="20" viewBox="0 0 72 50" fill="none" aria-hidden="true" style={{ display: 'block', flex: '0 0 auto' }}>
        <rect x="3" y="6" width="56" height="17" rx="8.5" fill="var(--text)" />
        <rect x="13" y="27" width="56" height="17" rx="8.5" fill="var(--text-3)" />
      </svg>
      Wotold
    </span>
  );
}

// ── Mini (folded) rail — icons only ──
function MiniRail({ view, recording, paused, theme, onExpand, onNav, onRecord, onPause, onSearch, onToggleTheme, onResizeStart }) {
  return (
    <aside className="minirail">
      <IconBtn icon="sidebar" label="Развернуть панель" tip="Развернуть ⌘\" tipSide="right" onClick={onExpand} />
      <div className="minirail-sep" />
      {recording ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6, alignItems: 'center' }}>
          <button className="mr-rec" data-rec="true" aria-label="Остановить" onClick={onRecord}><Icon name="stop" size={18} /></button>
          <IconBtn icon={paused ? 'play' : 'pause'} size="sm" tip={paused ? 'Продолжить' : 'Пауза'} tipSide="right" label="Пауза" onClick={onPause} />
        </div>
      ) : (
        <button className="mr-rec tip tip--right" data-tip="Записать звонок" aria-label="Запись" onClick={onRecord}><Icon name="mic" size={19} /></button>
      )}
      <IconBtn icon="command" label="Команды" tip="Команды ⌘K" tipSide="right" onClick={onSearch} />
      <div className="minirail-sep" />
      <IconBtn icon="inbox" label="Звонки" tip="Звонки" tipSide="right" active={view === 'inbox' || view === 'call'} onClick={() => onNav('inbox')} />
      <IconBtn icon="users" label="Контакты" tip="Контакты" tipSide="right" active={view === 'contacts'} onClick={() => onNav('contacts')} />
      <div className="mr-spacer" />
      <IconBtn icon={theme === 'dark' ? 'sun' : 'moon'} label="Тема" tip="Светлая / тёмная" tipSide="right" onClick={onToggleTheme} />
      <IconBtn icon="settings" label="Настройки" tip="Настройки" tipSide="right" active={view === 'settings'} onClick={() => onNav('settings')} />
      <div className="rail-resize" onMouseDown={onResizeStart} />
    </aside>
  );
}

// ── Sidebar ──
function Sidebar({ view, activeCallId, engine, setEngine, recording, paused, elapsed, onPause, onNav, onOpenCall, onRecord, onSearch, onCollapse, onResizeStart, theme, onToggleTheme }) {
  const recent = WK_CALLS.slice(0, 5);
  return (
    <aside className="rail">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '12px 12px 8px' }}>
        <Brand />
        <IconBtn icon="sidebar" size="sm" label="Свернуть" tip="Свернуть ⌘\\" onClick={onCollapse} />
      </div>

      <div style={{ padding: '0 10px 8px' }}>
        {recording ? (
          <div style={{ display: 'grid', gap: 6 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '2px 4px' }}>
              <Dot color="var(--danger)" pulse={!paused} />
              <span style={{ color: 'var(--danger)', fontWeight: 600, fontSize: 12.5 }}>{paused ? 'Пауза' : 'Идёт запись'}</span>
              <span className="mono" style={{ marginLeft: 'auto', fontSize: 13, fontWeight: 600 }}>{fmtClock(elapsed)}</span>
            </div>
            <div style={{ display: 'flex', gap: 6 }}>
              <Btn variant="default" icon={paused ? 'play' : 'pause'} onClick={onPause} style={{ flex: 1 }}>{paused ? 'Продолжить' : 'Пауза'}</Btn>
              <Btn variant="danger" icon="stop" onClick={onRecord}>Стоп</Btn>
            </div>
          </div>
        ) : (
          <Btn variant="primary" block icon="mic" onClick={onRecord}>Записать звонок</Btn>
        )}
      </div>

      <div style={{ padding: '0 10px' }}>
        <button className="input" onClick={onSearch} style={{ cursor: 'pointer', height: 30, color: 'var(--text-faint)', borderColor: 'var(--border-2)' }}>
          <Icon name="command" size={15} className="iico" />
          <span style={{ flex: 1, textAlign: 'left', fontSize: 13 }}>Команды</span>
          <Kbd>⌘K</Kbd>
        </button>
      </div>

      <nav className="scroll" style={{ flex: 1, minHeight: 0, padding: '10px' }}>
        <NavItem icon="inbox" label="Звонки" active={view === 'inbox' || view === 'call'} meta={WK_CALLS.length} onClick={() => onNav('inbox')} />
        <NavItem icon="users" label="Контакты" active={view === 'contacts'} meta={WK_CONTACTS.length} onClick={() => onNav('contacts')} />

        <div style={{ height: 8 }} />
        <SecLabel>Недавние</SecLabel>
        {recent.map((c) => (
          <NavItem key={c.id} label={c.title} active={view === 'call' && activeCallId === c.id}
            leading={<span className="nav-ico" style={{ width: 16, display: 'inline-flex', justifyContent: 'center' }}><StatusCell call={c} /></span>}
            meta={fmtDur(c.dur)} onClick={() => onOpenCall(c.id)} />
        ))}
      </nav>

      <div style={{ borderTop: '1px solid var(--border)', padding: 8 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <button className="navitem" data-active={view === 'settings' ? 'true' : undefined} onClick={() => onNav('settings')} style={{ flex: 1 }}>
            <span className="nav-ico"><Icon name="settings" size={16} /></span>
            <span className="nav-label">Настройки</span>
          </button>
          <IconBtn icon="code" size="sm" label="Дизайн-система" tip="Дизайн-система" active={view === 'design'} onClick={() => onNav('design')} />
          <IconBtn icon={theme === 'dark' ? 'sun' : 'moon'} size="sm" label="Тема" tip="Светлая / тёмная" onClick={onToggleTheme} />
        </div>
      </div>
      <div className="rail-resize" onMouseDown={onResizeStart} />
    </aside>
  );
}

// ── Recording dock (global) ──
function RecDock({ paused, elapsed, onPause, onStop, onMinimize }) {
  return (
    <div className="composer-dock">
      <div className="composer composer--rec fade-up" style={{ maxWidth: 'none' }}>
        <Dot color="var(--danger)" pulse={!paused} />
        <span style={{ color: 'var(--danger)', fontWeight: 600, fontSize: 13 }}>{paused ? 'Пауза' : 'Идёт запись'}</span>
        <span className="mono" style={{ fontSize: 16, fontWeight: 600 }}>{fmtClock(elapsed)}</span>
        {!paused && <Wave bars={7} color="var(--danger)" height={20} />}
        {!paused && (
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, marginLeft: 8, fontSize: 12, color: 'var(--text-3)' }}>
            <Icon name="sparkle" size={13} style={{ color: 'var(--accent-text)' }} />
            анализ сегмента · {Math.floor(elapsed / 8) + 1}
          </span>
        )}
        <div style={{ flex: 1 }} />
        <IconBtn icon="pip" label="Свернуть в виджет" onClick={onMinimize} />
        <IconBtn icon={paused ? 'play' : 'pause'} label={paused ? 'Продолжить' : 'Пауза'} onClick={onPause} />
        <button className="stop-btn" onClick={onStop} aria-label="Остановить"><Icon name="stop" size={16} /></button>
      </div>
    </div>
  );
}

// ── Floating recording widget (when main window is minimized) ──
function RecWidget({ paused, elapsed, onPause, onStop, onExpand }) {
  const [pos, setPos] = uS({ x: Math.max(8, window.innerWidth - 330), y: Math.max(8, window.innerHeight - 96) });
  const drag = (e) => {
    e.preventDefault();
    const sx = e.clientX, sy = e.clientY, ox = pos.x, oy = pos.y;
    const move = (ev) => setPos({ x: Math.max(8, Math.min(window.innerWidth - 300, ox + ev.clientX - sx)), y: Math.max(8, Math.min(window.innerHeight - 60, oy + ev.clientY - sy)) });
    const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
  };
  return (
    <div className="rec-widget fade-up" style={{ left: pos.x, top: pos.y }}>
      <button className="rw-grip" onMouseDown={drag} aria-label="Перетащить"><Icon name="dots" size={14} /></button>
      <Dot color="var(--danger)" pulse={!paused} />
      <span className="mono" style={{ fontWeight: 600, fontSize: 14 }}>{fmtClock(elapsed)}</span>
      {!paused && <Wave bars={5} color="var(--danger)" height={16} />}
      <IconBtn icon={paused ? 'play' : 'pause'} size="sm" onClick={onPause} label={paused ? 'Продолжить' : 'Пауза'} />
      <button className="stop-btn" style={{ width: 30, height: 30 }} onClick={onStop} aria-label="Остановить"><Icon name="stop" size={13} /></button>
      <IconBtn icon="pip" size="sm" onClick={onExpand} label="Развернуть окно" />
    </div>
  );
}

// ── Command palette ──
function Palette({ onClose, onNav, onOpenCall, onRecord }) {
  const [q, setQ] = uS('');
  const [sel, setSel] = uS(0);
  const ref = uR(null);
  uE(() => { ref.current && ref.current.focus(); }, []);
  uE(() => { setSel(0); }, [q]);

  const actions = [
    { id: 'rec', icon: 'record', label: 'Записать звонок', kbd: '⌘⇧R', run: onRecord },
    { id: 'inbox', icon: 'inbox', label: 'Все звонки', run: () => onNav('inbox') },
    { id: 'contacts', icon: 'users', label: 'Контакты', run: () => onNav('contacts') },
    { id: 'settings', icon: 'settings', label: 'Настройки', run: () => onNav('settings') },
  ];
  const ql = q.trim().toLowerCase();
  const fA = actions.filter((a) => a.label.toLowerCase().includes(ql));
  const fC = WK_CALLS.filter((c) => c.title.toLowerCase().includes(ql));
  const flat = [...fA.map((a) => ({ ...a, kind: 'a' })), ...fC.map((c) => ({ id: c.id, label: c.title, kind: 'c' }))];
  const exec = (it) => { it.kind === 'a' ? it.run() : onOpenCall(it.id); onClose(); };
  const onKey = (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(s + 1, flat.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (e.key === 'Enter') { e.preventDefault(); flat[sel] && exec(flat[sel]); }
    else if (e.key === 'Escape') onClose();
  };

  return (
    <div className="overlay fade" onMouseDown={onClose}>
      <div className="palette fade-up" onMouseDown={(e) => e.stopPropagation()}>
        <div className="palette-input">
          <Icon name="search" size={18} style={{ color: 'var(--text-faint)' }} />
          <input ref={ref} placeholder="Перейти к звонку или команда…" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} />
          <Kbd>esc</Kbd>
        </div>
        <div className="palette-list scroll">
          {fA.length > 0 && <MenuLabel>Команды</MenuLabel>}
          {fA.map((a) => { const i = flat.findIndex((f) => f.id === a.id && f.kind === 'a');
            return <MenuItem key={'a' + a.id} icon={a.icon} active={i === sel} end={a.kbd ? <Kbd>{a.kbd}</Kbd> : null} onClick={() => exec({ ...a, kind: 'a' })}>{a.label}</MenuItem>; })}
          {fC.length > 0 && <MenuLabel>Звонки</MenuLabel>}
          {fC.map((c) => { const i = flat.findIndex((f) => f.id === c.id && f.kind === 'c');
            return <MenuItem key={'c' + c.id} icon="doc" active={i === sel} end={<span className="u-faint" style={{ fontSize: 11 }}>{fmtDay(c.when)}</span>} onClick={() => exec({ id: c.id, kind: 'c' })}>{c.title}</MenuItem>; })}
          {flat.length === 0 && <div className="u-muted" style={{ padding: '18px 12px', fontSize: 13 }}>Ничего не найдено</div>}
        </div>
      </div>
    </div>
  );
}

// ── App ──
function App() {
  const [view, setView] = uS('inbox');
  const [activeCallId, setActiveCallId] = uS(null);
  const [collapsed, setCollapsed] = uS(false);
  const [railW, setRailW] = uS(() => { const v = parseInt(localStorage.getItem('wk-railw') || ''); return (v >= 216 && v <= 380) ? v : 256; });
  const [paletteOpen, setPaletteOpen] = uS(false);

  const [theme, setTheme] = uS('light');
  const [accent, setAccent] = uS('ink'); // бренд графит-моно — акцент фиксирован
  const [uiLang, setUiLang] = uS('ru');
  const [density, setDensity] = uS('cozy');
  const [engine, setEngine] = uS('local');

  const [recording, setRecording] = uS(false);
  const [paused, setPaused] = uS(false);
  const [elapsed, setElapsed] = uS(0);
  const [widget, setWidget] = uS(false);

  // inbox controls
  const [query, setQuery] = uS('');
  const [filter, setFilter] = uS('all');
  const [sort, setSort] = uS('date');

  uE(() => {
    const root = document.documentElement;
    const resolved = theme === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : theme;
    root.setAttribute('data-theme', resolved);
    root.setAttribute('data-accent', accent);
    root.setAttribute('data-density', density);
    root.setAttribute('lang', uiLang);
  }, [theme, accent, density, uiLang]);

  uE(() => {
    if (!recording || paused) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [recording, paused]);

  const toggleRec = uC(() => {
    setWidget(false);
    setRecording((r) => { if (r) { setPaused(false); return false; } setElapsed(0); return true; });
  }, []);

  uE(() => { try { localStorage.setItem('wk-railw', String(railW)); } catch (e) {} }, [railW]);
  const startResize = uC((e) => {
    e.preventDefault();
    const sx = e.clientX, sw = railW;
    const move = (ev) => { const w = sw + (ev.clientX - sx); if (w < 198) { setCollapsed(true); end(); return; } setRailW(Math.max(216, Math.min(380, w))); };
    const end = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', end); document.body.style.cursor = ''; document.body.style.userSelect = ''; };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', end);
    document.body.style.cursor = 'col-resize'; document.body.style.userSelect = 'none';
  }, [railW]);
  const startExpand = uC((e) => {
    e.preventDefault();
    const sx = e.clientX;
    const move = (ev) => { const w = 56 + (ev.clientX - sx); if (w > 150) { setCollapsed(false); setRailW(Math.max(216, Math.min(380, w))); } else { setCollapsed(true); } };
    const end = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', end); document.body.style.cursor = ''; document.body.style.userSelect = ''; };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', end);
    document.body.style.cursor = 'col-resize'; document.body.style.userSelect = 'none';
  }, []);

  uE(() => {
    const h = (e) => {
      const tag = (e.target.tagName || '').toLowerCase();
      if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) { e.preventDefault(); setPaletteOpen((o) => !o); }
      else if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key.toLowerCase() === 'r' || e.key === 'к' || e.key === 'К')) { e.preventDefault(); toggleRec(); }
      else if ((e.metaKey || e.ctrlKey) && (e.key === '\\')) { e.preventDefault(); setCollapsed((c) => !c); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [toggleRec]);

  const nav = (v) => { setView(v); setActiveCallId(null); };
  const openCall = (id) => { setActiveCallId(id); setView('call'); };
  const activeCall = WK_CALLS.find((c) => c.id === activeCallId);

  return (
    <div className="app" data-collapsed={collapsed} style={{ ['--rail-w']: railW + 'px' }}>
      {collapsed ? (
        <MiniRail view={view} recording={recording} paused={paused} theme={theme} onResizeStart={startExpand}
          onExpand={() => setCollapsed(false)} onNav={nav} onRecord={toggleRec} onPause={() => setPaused((p) => !p)}
          onSearch={() => setPaletteOpen(true)} onToggleTheme={() => setTheme((t) => t === 'dark' ? 'light' : 'dark')} />
      ) : (
        <Sidebar view={view} activeCallId={activeCallId} engine={engine} setEngine={setEngine} recording={recording}
          paused={paused} elapsed={elapsed} onPause={() => setPaused((p) => !p)} onResizeStart={startResize}
          onNav={nav} onOpenCall={openCall} onRecord={toggleRec} onSearch={() => setPaletteOpen(true)}
          onCollapse={() => setCollapsed(true)} theme={theme} onToggleTheme={() => setTheme((t) => t === 'dark' ? 'light' : 'dark')} />
      )}

      <main className="main">
        {view === 'inbox' && <InboxView onOpenCall={openCall} onRecord={toggleRec} recording={recording} paused={paused} elapsed={elapsed} onPause={() => setPaused((p) => !p)} />}
        {view === 'design' && <DesignSystemView theme={theme} setTheme={setTheme} accent={accent} setAccent={setAccent} density={density} setDensity={setDensity} />}
        {view === 'call' && activeCall && <CallView key={activeCall.id} call={activeCall} engine={engine} setEngine={setEngine} onBack={() => nav('inbox')} />}
        {view === 'contacts' && <ContactsView onOpenCall={openCall} />}
        {view === 'settings' && <SettingsView theme={theme} setTheme={setTheme} accent={accent} setAccent={setAccent} uiLang={uiLang} setUiLang={setUiLang} engine={engine} setEngine={setEngine} />}

        {recording && !widget && view !== 'call' && <RecDock paused={paused} elapsed={elapsed} onPause={() => setPaused((p) => !p)} onStop={toggleRec} onMinimize={() => setWidget(true)} />}
      </main>

      {recording && !widget && view === 'call' && <div style={{ position: 'fixed', left: collapsed ? 'var(--rail-mini)' : 'var(--rail-w)', right: 0, bottom: 0, zIndex: 30 }}><RecDock paused={paused} elapsed={elapsed} onPause={() => setPaused((p) => !p)} onStop={toggleRec} onMinimize={() => setWidget(true)} /></div>}

      {recording && widget && <><div className="widget-backdrop" /><RecWidget paused={paused} elapsed={elapsed} onPause={() => setPaused((p) => !p)} onStop={toggleRec} onExpand={() => setWidget(false)} /></>}

      {paletteOpen && <Palette onClose={() => setPaletteOpen(false)} onNav={nav} onOpenCall={openCall} onRecord={toggleRec} />}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('wk-root')).render(<App />);
