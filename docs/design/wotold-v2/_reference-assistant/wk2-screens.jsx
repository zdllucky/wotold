/* eslint-disable */
// WOTOLD · screens A (форк для страницы «Ассистент») — Inbox + Call; ассистент звонка = отдельный чат
// звонка, отвечает по его расшифровке и может привлекать другие звонки (источник всегда указан).
const { useState: useStateA } = React;

// shared: engine selector (Dropdown) + engine chip
function EngineSelect({ engine, setEngine, up, disabled, block }) {
  const e = WK_ENGINES[engine];
  return (
    <Dropdown up={up} width={260} block={block} trigger={({ toggle }) => (
      <button className="eng-trigger" onClick={toggle} disabled={disabled}
        style={{ opacity: disabled ? .5 : 1, width: block ? '100%' : undefined, minWidth: block ? 0 : undefined }}>
        <Icon name={e.icon} size={15} style={{ flex: '0 0 auto' }} />
        <span className="u-trunc" style={block ? { flex: 1, textAlign: 'left', minWidth: 0 } : null}>{e.label}<span className="et-sub"> · {e.sub}</span></span>
        <Icon name="chevronUpDown" size={13} style={{ color: 'var(--text-faint)', flex: '0 0 auto', marginLeft: block ? 'auto' : 0 }} />
      </button>
    )}>
      <MenuLabel>Обработка звонков</MenuLabel>
      {Object.values(WK_ENGINES).map((en) => (
        <MenuItem key={en.id} icon={en.icon} active={engine === en.id} onClick={() => setEngine(en.id)}
          end={engine === en.id ? <Icon name="check" size={14} /> : null}>
          <div style={{ fontWeight: 550 }}>{en.label}</div>
          <div className="u-faint" style={{ fontSize: 11.5 }}>{en.sub} · {en.facts[0]}</div>
        </MenuItem>
      ))}
    </Dropdown>
  );
}
function EngineChip({ via, size }) {
  const e = WK_ENGINES[via];
  return <Chip icon={e.icon} tone={e.tone} size={size}>{e.label}</Chip>;
}
function StatusCell({ call }) {
  if (call.status === 'processing') return <Dot ring pulse color="var(--accent)" />;
  if (call.status === 'error') return <Dot color="var(--danger)" />;
  return <Dot color="var(--ok)" />;
}

// ════════ INBOX ════════
function InboxView({ onOpenCall, onRecord, recording, paused, elapsed, onPause }) {
  const [f, setF] = useStateA({ ...I_EMPTY });
  const [text, setText] = useStateA('');
  const [view, setView] = useStateA('list');
  const [sort, setSort] = useStateA('date');

  let calls = WK_CALLS.filter((c) => iMatch(c, f, text));
  calls = [...calls].sort((a, b) => sort === 'dur' ? b.dur - a.dur : new Date(b.when) - new Date(a.when));

  const rows = [];
  let lastGroup = null;
  calls.forEach((c) => {
    const g = sort === 'date' ? relDay(c.when) : null;
    if (g && g !== lastGroup) { rows.push({ group: g, key: 'g-' + g }); lastGroup = g; }
    rows.push({ call: c, key: c.id });
  });

  return (
    <>
      <div className="view-head">
        <Icon name="inbox" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650, fontSize: 'var(--t-14)' }}>Звонки</span>
        <Chip size="sm" tone="line">{calls.length}</Chip>
        <div style={{ display: 'flex', gap: 6, flex: '1 1 auto', maxWidth: 480, marginLeft: 10 }}>
          <div style={{ flex: 1, minWidth: 0 }}><OmniBar f={f} setF={setF} text={text} setText={setText} /></div>
          <FacetButton f={f} setF={setF} />
        </div>
        <ViewSwitcher view={view} setView={setView} />
        <div style={{ flex: 1 }} />
        {recording ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <IconBtn icon={paused ? 'play' : 'pause'} label={paused ? 'Продолжить' : 'Пауза'} onClick={onPause} />
            <button className="btn btn--danger" onClick={onRecord} style={{ gap: 8 }}>
              <Dot color="#fff" pulse={!paused} /><span className="mono" style={{ fontWeight: 600 }}>{fmtClock(elapsed)}</span><Icon name="stop" size={14} />
            </button>
          </div>
        ) : (
          <Btn variant="primary" icon="mic" onClick={onRecord}>Записать</Btn>
        )}
      </div>

      <div className="scroll" style={{ flex: 1, minHeight: 0 }}>
        {calls.length === 0 ? (
          <Empty icon="search" title="Ничего не найдено" desc="Измените запрос или фильтры." />
        ) : view === 'list' ? (
          <div className="tbl">
            <div className="tbl-head">
              <span />
              <span>Название</span>
              <span>Участники</span>
              <span className="th-sort" onClick={() => setSort('dur')}>Длит.<Icon name="sort" size={11} /></span>
              <span className="th-sort" onClick={() => setSort('date')}>Дата<Icon name="sort" size={11} /></span>
              <span />
            </div>
            {rows.map((r) => r.group ? (
              <div className="tbl-group" key={r.key}>{r.group}</div>
            ) : (
              <CallRow key={r.key} call={r.call} onOpen={() => onOpenCall(r.call.id)} />
            ))}
          </div>
        ) : view === 'cards' ? (
          <InboxCards calls={calls} onOpen={onOpenCall} />
        ) : view === 'week' ? (
          <InboxWeek calls={calls} onOpen={onOpenCall} />
        ) : (
          <InboxMonth calls={calls} onOpen={onOpenCall} />
        )}
      </div>
    </>
  );
}

function CallRow({ call, onOpen }) {
  const parts = call.parts.map(av);
  return (
    <div className="trow" role="button" onClick={onOpen}>
      <StatusCell call={call} />
      <span className="t-title u-trunc">
        <span className="u-trunc">{call.title}</span>
        {call.recap && <Icon name="sparkle" size={12} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />}
        {call.status === 'processing' && <Chip size="sm" tone="accent">обработка</Chip>}
        {call.status === 'error' && <Chip size="sm" tone="danger">ошибка</Chip>}
      </span>
      <span><AvatarGroup items={parts} size={20} max={3} /></span>
      <span className="t-cell mono">{fmtDur(call.dur)}</span>
      <span className="t-cell">{fmtDay(call.when)}</span>
      <span className="t-more" onClick={(e) => e.stopPropagation()}>
        <Dropdown align="right" width={190} trigger={({ toggle }) => <IconBtn icon="dots" size="sm" onClick={toggle} label="Действия" />}>
          <MenuItem icon="doc" onClick={onOpen}>Открыть</MenuItem>
          <MenuItem icon="refresh">Переобработать</MenuItem>
          <MenuItem icon="download">Экспорт…</MenuItem>
          <MenuSep />
          <MenuItem icon="trash" danger>Удалить</MenuItem>
        </Dropdown>
      </span>
    </div>
  );
}

// ════════ CALL DETAIL ════════
function initAssign(call) {
  const m = {};
  call.parts.forEach((k) => {
    const c = WK_CONTACTS.find((x) => x.sp === k);
    const s = WK_SPEAKERS[k];
    const confirmed = c ? c.confirmed : false;
    m[k] = confirmed
      ? { name: s.name, color: s.color, role: c ? c.role : 'не в контактах', confirmed: true, contactId: c ? c.id : null }
      : { name: s.name, color: 'var(--text-faint)', role: 'голос не определён', confirmed: false, contactId: null };
  });
  return m;
}

// отдельный чат под каждый звонок — переживает переключение экранов
const WK_CALL_THREADS = {};

function CallView({ call, engine, setEngine, onBack, onOpenCall, onAskGlobal }) {
  const [tab, setTab] = useStateA('transcript');
  const [actions, setActions] = useStateA(WK_RECAP.actions);
  const [thread, setThread] = useStateA(() => WK_CALL_THREADS[call.id] || []);
  const [askPending, setAskPending] = useStateA(false);
  const [draft, setDraft] = useStateA('');
  const [assign, setAssign] = useStateA(() => initAssign(call));
  const [playing, setPlaying] = useStateA(false);
  const [playT, setPlayT] = useStateA(0);
  const segTimes = WK_TRANSCRIPT.map((_, i) => Math.round((i / WK_TRANSCRIPT.length) * call.dur));
  React.useEffect(() => {
    if (!playing) return;
    const id = setInterval(() => setPlayT((t) => { if (t >= call.dur) { setPlaying(false); return call.dur; } return t + 1; }), 1000);
    return () => clearInterval(id);
  }, [playing, call.dur]);
  const seekPlay = (t) => { setPlayT(Math.max(0, Math.min(call.dur, t))); setPlaying(true); };
  const parts = call.parts.map(av);

  const ready = call.status === 'ready';
  const stg = call.stage ?? 0;
  const transcriptReady = ready || stg >= 2;
  const recapReady = ready;

  const assignTo = (k, c) => setAssign((m) => ({ ...m, [k]: { name: c.name, color: WK_SPEAKERS[c.sp].color, role: c.role, confirmed: true, contactId: c.id } }));
  const resetSpeaker = (k) => setAssign((m) => ({ ...m, [k]: { name: 'Говорящий', color: 'var(--text-faint)', role: 'голос не определён', confirmed: false, contactId: null } }));

  const pushMsg = (m) => setThread((t) => { const nt = [...t, m]; WK_CALL_THREADS[call.id] = nt; return nt; });
  const ask = (q) => {
    if (askPending) return;
    pushMsg({ me: true, text: q });
    setDraft(''); setAskPending(true);
    setTimeout(() => { pushMsg({ me: false, ans: WK_AS_ANSWER(q, 'call') }); setAskPending(false); }, 800);
  };

  const tabs = [{ value: 'transcript', label: 'Транскрипт', icon: 'doc' }, { value: 'recap', label: 'Рекап', icon: 'sparkle' }];
  if (ready) tabs.push({ value: 'assistant', label: 'Ассистент', icon: 'chat' });

  return (
    <>
      <div className="view-head">
        <button onClick={onBack} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-3)', fontSize: 'var(--t-13)', padding: 0 }}
          onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--text)'; }} onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--text-3)'; }}>Звонки</button>
        <Icon name="chevronRight" size={13} style={{ color: 'var(--text-faint)' }} />
        <span className="u-trunc" style={{ fontWeight: 600, maxWidth: 360 }}>{call.title}</span>
        <div style={{ flex: 1 }} />
        <Dropdown align="right" width={200} trigger={({ toggle }) => <IconBtn icon="dots" onClick={toggle} label="Ещё" />}>
          <MenuItem icon="download" end={ready ? null : <span className="u-faint" style={{ fontSize: 11 }}>—</span>}>Экспортировать…</MenuItem>
          <MenuItem icon="edit">Переименовать</MenuItem>
          <MenuItem icon="refresh">Переобработать</MenuItem>
          <MenuItem icon="copy">Копировать ссылку</MenuItem>
          <MenuSep />
          <MenuItem icon="trash" danger>Удалить</MenuItem>
        </Dropdown>
      </div>

      <div className="view-body">
        <div className="content doc-wrap">
          <div className="doc-scroll scroll">
            <div className="doc" key={call.id} style={{ paddingBottom: ready && tab !== 'assistant' ? 104 : undefined }}>
              <h1 className="doc-title">{call.title}</h1>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, alignItems: 'center', margin: '12px 0 4px' }}>
                <Chip icon="clock">{fmtTime(call.when)}</Chip>
                <Chip icon="waveform">{fmtDur(call.dur)}</Chip>
                {call.status === 'processing' && <Chip tone="accent" icon="refresh">Обработка</Chip>}
                {call.status === 'error' && <Chip tone="danger" icon="alert">Ошибка</Chip>}
              </div>

              {call.status === 'error' ? <ErrorCard /> : <>
                {call.status === 'processing' && <ProcStatusBar call={call} />}
                <div style={{ marginTop: 16 }}><Tabs tabs={tabs} value={tab === 'assistant' && !ready ? 'transcript' : tab} onChange={setTab} /></div>
                {(tab === 'transcript' || (tab === 'assistant' && !ready)) && (transcriptReady ? <Transcript assign={assign} segTimes={segTimes} playT={ready ? playT : null} onSeek={ready ? seekPlay : null} /> : <TranscriptSkeleton />)}
                {tab === 'recap' && (recapReady ? <Recap actions={actions} setActions={setActions} /> : <RecapSkeleton />)}
                {tab === 'assistant' && ready && (
                  <Assistant thread={thread} pending={askPending} onAsk={ask} callId={call.id} onOpenCall={onOpenCall}
                    onSeek={(t) => { setTab('transcript'); seekPlay(t); }} onAskGlobal={onAskGlobal} />
                )}
              </>}
            </div>
          </div>

          {ready && tab === 'assistant' && (
            <div className="composer-dock">
              <form className="composer composer-ask ai-field" onSubmit={(e) => { e.preventDefault(); if (draft.trim()) ask(draft.trim()); }}>
                <Icon name="sparkle" size={16} style={{ color: 'var(--accent-text)', flex: '0 0 auto' }} />
                <input placeholder="Спросить об этом звонке…" value={draft} onChange={(e) => setDraft(e.target.value)} />
                <IconBtn icon="send" active={!!draft.trim()} label="Отправить" onClick={(e) => { e.preventDefault(); if (draft.trim()) ask(draft.trim()); }} />
              </form>
            </div>
          )}
          {ready && tab !== 'assistant' && (
            <CallPlayer dur={call.dur} t={playT} playing={playing} onToggle={() => setPlaying((p) => !p)} onSeek={(t) => setPlayT(t)} />
          )}
        </div>

        <CallRail call={call} assign={assign} onAssign={assignTo} onReset={resetSpeaker} />
      </div>
    </>
  );
}

function ProcStatusBar({ call }) {
  const stg = call.stage ?? 0;
  const cur = WK_STAGES[stg] || WK_STAGES[0];
  return (
    <div style={{ marginTop: 16, display: 'flex', alignItems: 'center', gap: 11, padding: '11px 13px', background: 'var(--accent-soft)', border: '1px solid var(--accent-line)', borderRadius: 'var(--r-md)' }}>
      <Wave bars={4} color="var(--accent-text)" height={16} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--accent-text)' }}>{cur.label}…</div>
        <div style={{ display: 'flex', gap: 4, marginTop: 7 }}>
          {WK_STAGES.map((s, i) => (
            <span key={i} className={i === stg ? 'dot--pulse' : ''} style={{ height: 3, flex: 1, borderRadius: 2, background: i <= stg ? 'var(--accent)' : 'var(--accent-line)' }} />
          ))}
        </div>
      </div>
      <span className="u-faint mono" style={{ fontSize: 11.5, whiteSpace: 'nowrap' }}>шаг {stg + 1}/{WK_STAGES.length}</span>
    </div>
  );
}

function TranscriptSkeleton() {
  return (
    <div style={{ marginTop: 14 }}>
      {[0, 1, 2, 3].map((i) => (
        <div className="turn" key={i}>
          <div>
            <div className="skeleton" style={{ height: 12, width: 92, borderRadius: 6 }} />
            <div className="skeleton" style={{ height: 9, width: 34, marginTop: 9, marginLeft: 27, borderRadius: 5 }} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <div className="skeleton" style={{ height: 13, width: '94%', borderRadius: 6 }} />
            <div className="skeleton" style={{ height: 13, width: i % 2 ? '72%' : '86%', borderRadius: 6 }} />
            {i % 2 === 0 && <div className="skeleton" style={{ height: 13, width: '58%', borderRadius: 6 }} />}
          </div>
        </div>
      ))}
    </div>
  );
}

function RecapSkeleton() {
  return (
    <div style={{ marginTop: 18 }}>
      <div className="skeleton" style={{ height: 70, borderRadius: 'var(--r-md)' }} />
      <div className="block-h">Решения</div>
      {[0, 1].map((i) => <div className="skeleton" key={i} style={{ height: 13, width: i ? '68%' : '84%', borderRadius: 6, marginBottom: 11 }} />)}
      <div className="block-h">Задачи</div>
      {[0, 1, 2].map((i) => <div className="skeleton" key={i} style={{ height: 13, width: ['82%', '74%', '64%'][i], borderRadius: 6, marginBottom: 11 }} />)}
      <div className="u-faint" style={{ fontSize: 12, marginTop: 12, display: 'flex', alignItems: 'center', gap: 6 }}>
        <Icon name="sparkle" size={13} />Рекап сформируется после распознавания
      </div>
    </div>
  );
}

function Transcript({ assign, segTimes, playT, onSeek }) {
  return (
    <div style={{ marginTop: 14 }}>
      {WK_TRANSCRIPT.map((turn, i) => {
        const base = WK_SPEAKERS[turn.sp];
        const info = (assign && assign[turn.sp]) || { name: base.name, color: base.color };
        const start = segTimes ? segTimes[i] : turn.t;
        const end = segTimes ? (segTimes[i + 1] != null ? segTimes[i + 1] : Infinity) : Infinity;
        const active = playT != null && playT >= start && playT < end;
        return (
          <div className={'turn' + (active ? ' turn--active' : '') + (onSeek ? ' turn--seek' : '')} key={i}
            onClick={onSeek ? () => onSeek(start) : undefined}>
            <div>
              <div className="turn-sp" style={{ color: info.color }}><Avatar name={info.name} color={info.color} size={20} />{info.name}</div>
              <div className="turn-time">{fmtClock(start)}{onSeek && <Icon name="play" size={9} style={{ marginLeft: 4, opacity: active ? 1 : 0 }} />}</div>
            </div>
            <div className="turn-text">{turn.text}</div>
          </div>
        );
      })}
    </div>
  );
}

function CallPlayer({ dur, t, playing, onToggle, onSeek }) {
  const ref = React.useRef(null);
  const bars = 130;
  const pct = dur ? Math.max(0, Math.min(1, t / dur)) : 0;
  const seekAt = (clientX) => { const r = ref.current.getBoundingClientRect(); const x = Math.max(0, Math.min(1, (clientX - r.left) / r.width)); onSeek(Math.round(x * dur)); };
  const down = (e) => { e.preventDefault(); seekAt(e.clientX); const move = (ev) => seekAt(ev.clientX); const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); }; document.addEventListener('mousemove', move); document.addEventListener('mouseup', up); };
  return (
    <div className="player-dock">
      <div className="player">
        <button className="player-play" onClick={onToggle} aria-label={playing ? 'Пауза' : 'Воспроизвести'}><Icon name={playing ? 'pause' : 'play'} size={16} /></button>
        <span className="mono" style={{ fontSize: 12, color: 'var(--text-2)', width: 44, textAlign: 'right', flex: '0 0 auto' }}>{fmtClock(t)}</span>
        <div className="player-wave" ref={ref} onMouseDown={down}>
          {Array.from({ length: bars }).map((_, i) => {
            const bp = i / bars;
            return <i key={i} style={{ height: 4 + ((i * 53 + 7) % 18), background: bp <= pct ? 'var(--accent)' : 'var(--border-strong)' }} />;
          })}
          <span className="player-head" style={{ left: (pct * 100) + '%' }} />
        </div>
        <span className="mono" style={{ fontSize: 12, color: 'var(--text-faint)', width: 44, flex: '0 0 auto' }}>{fmtDur(dur)}</span>
      </div>
    </div>
  );
}

function recapMd(actions) {
  let md = '## Сводка\n\n' + WK_RECAP.summary + '\n\n## Решения\n\n';
  WK_RECAP.decisions.forEach((d) => { md += '- ' + d + '\n'; });
  md += '\n## Задачи\n\n';
  actions.forEach((a) => { const who = WK_SPEAKERS[a.who].name.split(' ')[0]; md += '- [' + (a.done ? 'x' : ' ') + '] **' + who + '.** ' + a.text + ' _(до ' + a.due + ')_\n'; });
  md += '\n## Темы\n\n' + WK_RECAP.topics.map((t) => '`' + t + '`').join(' ') + '\n';
  return md;
}

function Recap({ actions, setActions }) {
  const [mode, setMode] = useStateA('rich');
  const [copied, setCopied] = useStateA(false);
  const toggle = (i) => setActions((a) => a.map((x, j) => j === i ? { ...x, done: !x.done } : x));
  const md = recapMd(actions);
  const copy = () => { try { navigator.clipboard.writeText(md).catch(() => {}); } catch (e) {} setCopied(true); setTimeout(() => setCopied(false), 1400); };
  return (
    <div style={{ marginTop: 18 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
        <Segmented value={mode} onChange={setMode} options={[{ value: 'rich', label: 'Оформленный', icon: 'sparkle' }, { value: 'md', label: 'Markdown', icon: 'code' }]} />
        <div style={{ flex: 1 }} />
        <Btn variant="default" size="sm" icon={copied ? 'check' : 'copy'} onClick={copy}>{copied ? 'Скопировано' : 'Копировать .md'}</Btn>
      </div>
      {mode === 'md' ? (
        <pre className="md-raw">{md}</pre>
      ) : (
        <div className="md-rich">
          <h3 className="md-h">Сводка</h3>
          <p className="md-p">{WK_RECAP.summary}</p>
          <h3 className="md-h">Решения</h3>
          <ul className="md-ul">{WK_RECAP.decisions.map((d, i) => <li key={i}>{d}</li>)}</ul>
          <h3 className="md-h">Задачи</h3>
          <ul className="md-tasks">
            {actions.map((a, i) => {
              const who = WK_SPEAKERS[a.who].name.split(' ')[0];
              return (
                <li key={i}>
                  <button className="chk" data-done={a.done} onClick={() => toggle(i)} aria-label="Готово"><Icon name="check" size={12} /></button>
                  <span style={{ color: a.done ? 'var(--text-3)' : 'var(--text)', textDecoration: a.done ? 'line-through' : 'none' }}>
                    <b style={{ fontWeight: 600 }}>{who}.</b> {a.text} <span className="u-faint" style={{ fontSize: 12 }}>(до {a.due})</span>
                  </span>
                </li>
              );
            })}
          </ul>
          <h3 className="md-h">Темы</h3>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>{WK_RECAP.topics.map((t) => <code className="md-code" key={t}>{t}</code>)}</div>
        </div>
      )}
    </div>
  );
}

function Assistant({ thread, pending, onAsk, callId, onOpenCall, onSeek, onAskGlobal }) {
  return (
    <div style={{ marginTop: 18, paddingBottom: 80 }}>
      {thread.length === 0 && (
        <div style={{ color: 'var(--text-3)', fontSize: 13.5, marginBottom: 14 }}>
          Чат этого звонка. Ответы строятся по его расшифровке; если факт найден в другом звонке — источник будет указан.
        </div>
      )}
      <div className="ask-thread">
        {thread.map((m, i) => m.me
          ? <div className="ask-row fade-up" data-me="true" key={i}><div className="ask-bubble">{m.text}</div></div>
          : <div className="ask-row fade-up" data-me="false" key={i}><AnswerMsg ans={m.ans} callId={callId} onOpenCall={onOpenCall} onSeek={onSeek} onAskGlobal={onAskGlobal} /></div>)}
        {pending && <div className="ask-row" data-me="false"><div className="ask-bubble ask-pend"><Wave bars={4} color="var(--text-3)" height={13} />Поиск…</div></div>}
      </div>
      <div className="ask-suggest" style={{ marginTop: thread.length ? 16 : 0 }}>
        {WK_ASK2.map((s) => <Chip key={s.q} tone="line" icon="arrowRight" onClick={() => !pending && onAsk(s.q)}>{s.q}</Chip>)}
      </div>
    </div>
  );
}

function Pipeline({ call }) {
  return (
    <div style={{ marginTop: 22 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 9, marginBottom: 10 }}>
        <Wave bars={4} color="var(--accent)" height={16} />
        <span style={{ fontWeight: 600 }}>Обработка…</span>
        <Chip icon="clock" style={{ marginLeft: 'auto' }}>~2 мин</Chip>
      </div>
      {WK_STAGES.map((s, i) => {
        const state = i < call.stage ? 'done' : i === call.stage ? 'active' : 'pending';
        return (
          <div className="stage" data-state={state} key={i}>
            <span className="stage-ico"><Icon name={state === 'done' ? 'check' : s.icon} size={15} /></span>
            <span style={{ fontWeight: state === 'active' ? 600 : 400, fontSize: 14 }}>{s.label}</span>
            {state === 'active' && <Wave bars={4} color="var(--accent)" height={16} style={{ marginLeft: 'auto' }} />}
          </div>
        );
      })}
      <p className="u-muted" style={{ fontSize: 13, marginTop: 14 }}>Транскрипт появится автоматически. Окно можно закрыть — обработка идёт в фоне.</p>
    </div>
  );
}

function ErrorCard() {
  return (
    <div style={{ marginTop: 22 }}>
      <Panel style={{ padding: 18, background: 'var(--danger-soft)', borderColor: 'var(--danger-soft)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--danger-text)', fontWeight: 600, marginBottom: 6 }}>
          <Icon name="alert" size={16} /> Обработка не завершена
        </div>
        <div style={{ fontWeight: 600, marginBottom: 2 }}>Файл записи повреждён</div>
        <p className="u-muted" style={{ fontSize: 13.5, margin: 0 }}>Запись сохранена, но звуковую дорожку не удалось прочитать. Можно попробовать переобработать.</p>
        <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
          <Btn variant="primary" size="sm" icon="refresh">Переобработать</Btn>
        </div>
      </Panel>
    </div>
  );
}

function SpeakerRow({ k, info, onAssign, onReset }) {
  const undef = !info.confirmed;
  const [q, setQ] = useStateA('');
  const matches = WK_CONTACTS.filter((c) => c.confirmed && c.name.toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="lrow" style={{ padding: '5px 0', gap: 10 }}>
      <Avatar name={undef ? '?' : info.name} color={undef ? 'var(--text-faint)' : info.color} size={28} />
      <div style={{ minWidth: 0, flex: 1 }}>
        <div className="u-trunc" style={{ fontWeight: 550, color: undef ? 'var(--text-2)' : 'var(--text)' }}>{undef ? 'Говорящий' : info.name}</div>
        <div className="u-faint u-trunc" style={{ fontSize: 11.5 }}>{info.role}</div>
      </div>
      <Dropdown align="right" width={244} trigger={({ toggle }) => (
        undef
          ? <Btn variant="soft" size="sm" onClick={toggle}>Определить</Btn>
          : <IconBtn icon="dots" size="sm" onClick={toggle} label="Переопределить" />
      )}>
        <div onClick={(e) => e.stopPropagation()} style={{ padding: 3 }}>
          <Input icon="search" size="sm" placeholder="Поиск контакта…" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
        </div>
        <MenuLabel>{undef ? 'Кто это говорит?' : 'Участник'}</MenuLabel>
        {matches.map((c) => (
          <button key={c.id} className="menu-item" data-active={info.contactId === c.id} onClick={() => onAssign(k, c)}>
            <Avatar name={c.name} color={WK_SPEAKERS[c.sp].color} size={20} />
            <span style={{ flex: 1, minWidth: 0 }} className="u-trunc">{c.name}</span>
            {info.contactId === c.id && <Icon name="check" size={14} style={{ color: 'var(--accent-text)' }} />}
          </button>
        ))}
        {matches.length === 0 && <div className="u-faint" style={{ padding: '8px 10px', fontSize: 12.5 }}>Не найдено</div>}
        <MenuSep />
        <MenuItem icon="user">Новый контакт…</MenuItem>
        {!undef && <button className="menu-item menu-item--danger" onClick={() => onReset(k)}><span className="mi-ico"><Icon name="x" size={15} /></span><span>Снять определение</span></button>}
      </Dropdown>
    </div>
  );
}

function CallRail({ call, assign, onAssign, onReset }) {
  const statusChip = call.status === 'ready' ? <Chip tone="ok" icon="check">Готово</Chip>
    : call.status === 'processing' ? <Chip tone="accent" icon="refresh">Обработка</Chip>
    : <Chip tone="danger" icon="alert">Ошибка</Chip>;
  const undefCount = call.parts.filter((k) => !(assign[k] && assign[k].confirmed)).length;
  return (
    <aside className="rrail">
      <div className="rrail-scroll">
        <div className="rrail-sec" style={{ marginTop: 0 }}>Свойства</div>
        <div className="prop"><span className="prop-k">Статус</span><span>{statusChip}</span></div>
        <div className="prop"><span className="prop-k">Движок</span><span><EngineChip via={call.via} size="sm" /></span></div>
        <div className="prop"><span className="prop-k"><Icon name="calendar" size={13} />Дата</span><span>{fmtDay(call.when)} · {fmtTime(call.when)}</span></div>
        <div className="prop"><span className="prop-k"><Icon name="clock" size={13} />Длительность</span><span className="mono">{fmtDur(call.dur)}</span></div>

        <div className="rrail-sec" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span>Участники · {call.parts.length}</span>
          {undefCount > 0 && <Chip size="sm" tone="warn">{undefCount} не определён{undefCount > 1 ? 'о' : ''}</Chip>}
        </div>
        {call.parts.map((k) => <SpeakerRow key={k} k={k} info={assign[k]} onAssign={onAssign} onReset={onReset} />)}

        <div className="rrail-sec">Действия</div>
        <div style={{ display: 'grid', gap: 6 }}>
          <Btn variant="default" size="sm" icon="download" block disabled={call.status !== 'ready'}>Экспортировать рекап</Btn>
          <Btn variant="ghost" size="sm" icon="folder" block>Открыть папку записи</Btn>
        </div>
      </div>
    </aside>
  );
}

Object.assign(window, { EngineSelect, EngineChip, StatusCell, InboxView, CallView });
