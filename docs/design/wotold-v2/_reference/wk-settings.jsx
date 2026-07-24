/* eslint-disable */
// WOTOLD · Settings — native v2 idiom: dense label→control rows, compact headers,
// lean business-register copy. Composed only from kit primitives.
const { useState: sU, useEffect: sE, useRef: sR } = React;

const SET_SECS = [
  { id: 'appearance',  label: 'Внешний вид',       icon: 'sun' },
  { id: 'account',     label: 'Учётная запись',    icon: 'user' },
  { id: 'processing',  label: 'Обработка',          icon: 'cpu' },
  { id: 'permissions', label: 'Разрешения',        icon: 'shield' },
  { id: 'recording',   label: 'Запись',            icon: 'mic' },
  { id: 'speakers',    label: 'Спикеры',           icon: 'users' },
  { id: 'labs',        label: 'Лаборатория',       icon: 'bolt' },
  { id: 'maintenance', label: 'Обслуживание',      icon: 'refresh' },
  { id: 'privacy',     label: 'Приватность',       icon: 'lock' },
];
const SET_HEAD = {
  appearance:  ['Внешний вид', 'Тема применяется сразу.'],
  account:     ['Учётная запись', 'Вход не обязателен — Wotold работает локально. Синхронизация скоро.'],
  processing:  ['Обработка', 'Где расшифровываются звонки.'],
  permissions: ['Разрешения', 'Без доступа к микрофону и системному звуку запись не начнётся.'],
  recording:   ['Запись', 'Языки, горячие клавиши, авто-определение.'],
  speakers:    ['Спикеры', 'Распознавание собеседников по голосу.'],
  labs:        ['Лаборатория', 'Экспериментальные функции.'],
  maintenance: ['Обслуживание', 'Операции над накопленными данными.'],
  privacy:     ['Приватность', 'Полная очистка локальных данных.'],
};
const LOCALES = [{ value: 'ru', label: 'Русский' }, { value: 'en', label: 'English' }, { value: 'kk', label: 'Қазақша' }];
const STT_LANGS = [{ value: 'auto', label: 'Авто' }, { value: 'ru', label: 'Русский' }, { value: 'en', label: 'English' }, { value: 'kk', label: 'Қазақша' }];
const RECAP_LANGS = [{ value: 'auto', label: 'Как в звонке' }, { value: 'ru', label: 'Русский' }, { value: 'en', label: 'English' }, { value: 'kk', label: 'Қазақша' }];

// ── native layout helpers ──
function SecHead({ id }) {
  const [, lead] = SET_HEAD[id];
  return <p className="u-muted" style={{ fontSize: 13, lineHeight: 1.5, margin: '0 0 18px' }}>{lead}</p>;
}
function GroupLabel({ children, top = 26 }) { return <div className="rrail-sec" style={{ marginTop: top, marginBottom: 2 }}>{children}</div>; }
// dense inline row: label (+hint) left, control right
function Row({ label, hint, children, align = 'center', last, disabled }) {
  return (
    <div style={{ display: 'flex', alignItems: align === 'top' ? 'flex-start' : 'center', gap: 24, padding: '13px 0', borderBottom: last ? 'none' : '1px solid var(--border)', opacity: disabled ? 0.55 : 1 }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13.5, fontWeight: 550 }}>{label}</div>
        {hint && <div className="u-muted" style={{ fontSize: 12, lineHeight: 1.45, marginTop: 4, maxWidth: 400 }}>{hint}</div>}
      </div>
      <div style={{ flex: '0 0 auto', display: 'flex', alignItems: 'center', gap: 8 }}>{children}</div>
    </div>
  );
}
// AccentPicker удалён — бренд графит-моно, акцент фиксирован

// ── HotkeyCapture ──
const HK_RESERVED = ['KeyW', 'KeyQ', 'KeyM', 'KeyH', 'KeyC', 'KeyV', 'KeyX', 'KeyA', 'KeyZ'];
function fmtCombo(e) {
  const p = [];
  if (e.metaKey) p.push('⌘'); if (e.ctrlKey) p.push('⌃'); if (e.altKey) p.push('⌥'); if (e.shiftKey) p.push('⇧');
  p.push(e.code.replace('Key', '').replace('Digit', ''));
  return p.join('');
}
function HotkeyCapture({ value, onChange }) {
  const [cap, setCap] = sU(false);
  const [err, setErr] = sU('');
  sE(() => {
    if (!cap) return;
    const h = (e) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === 'Escape') { setCap(false); setErr(''); return; }
      if (['Meta', 'Shift', 'Alt', 'Control'].includes(e.key)) return;
      if (!(e.metaKey || e.ctrlKey || e.altKey)) { setErr('Нужен модификатор'); return; }
      if (e.metaKey && HK_RESERVED.includes(e.code)) { setErr('Занято системой'); return; }
      onChange(fmtCombo(e)); setErr(''); setCap(false);
    };
    window.addEventListener('keydown', h, true);
    return () => window.removeEventListener('keydown', h, true);
  }, [cap]);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {err && <span style={{ color: 'var(--danger-text)', fontSize: 11.5, fontStyle: 'italic' }}>{err}</span>}
      <span className="hotkey-readout mono">{cap ? '…' : value}</span>
      <Btn variant={cap ? 'ghost' : 'default'} size="sm" onClick={() => { setErr(''); setCap((c) => !c); }}>{cap ? 'Esc' : 'Изменить'}</Btn>
    </div>
  );
}

// ════════ 1 · APPEARANCE ════════
function SecAppearance({ theme, setTheme, accent, setAccent, uiLang, setUiLang, ping }) {
  const w = (fn) => (v) => { fn(v); ping(); };
  return (
    <>
      <SecHead id="appearance" />
      <div style={{ marginTop: 18 }}>
        <Row label="Тема"><Segmented value={theme} onChange={w(setTheme)} options={[{ value: 'light', label: 'Светлая', icon: 'sun' }, { value: 'dark', label: 'Тёмная', icon: 'moon' }, { value: 'system', label: 'Системная' }]} /></Row>
        <Row label="Язык интерфейса" hint="Метки и кнопки. Контент звонков не меняется." last><Select value={uiLang} onChange={w(setUiLang)} options={LOCALES} width={170} /></Row>
      </div>
    </>
  );
}

// ════════ 2 · ACCOUNT ════════
function SecAccount() {
  const [state, setState] = sU('signed_out');
  const [sid, setSid] = sU('');
  return (
    <>
      <SecHead id="account" />
      <Panel style={{ padding: 18, maxWidth: 520, marginTop: 18 }}>
        {state === 'signed_out' && (
          <div style={{ display: 'grid', gap: 13 }}>
            <div style={{ fontSize: 13.5, fontWeight: 550 }}>Вход через SSO</div>
            <p className="u-muted" style={{ fontSize: 12.5, margin: '-6px 0 0', lineHeight: 1.5 }}>Откроется браузер. Сейчас вход ничего не разблокирует.</p>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
              <Btn variant="default" size="sm" icon="user" onClick={() => setState('pending_paste')}>Google</Btn>
              <Btn variant="ghost" size="sm" disabled>Apple <span className="u-faint" style={{ fontSize: 11 }}>скоро</span></Btn>
              <Btn variant="ghost" size="sm" disabled>Microsoft <span className="u-faint" style={{ fontSize: 11 }}>скоро</span></Btn>
            </div>
          </div>
        )}
        {state === 'pending_paste' && (
          <div style={{ display: 'grid', gap: 12 }}>
            <div style={{ fontSize: 13.5, fontWeight: 550 }}>Вставьте Session ID</div>
            <p className="u-muted" style={{ fontSize: 12.5, margin: '-6px 0 0', lineHeight: 1.5 }}>После входа прокси покажет JSON с полем sessionId — скопируйте значение сюда.</p>
            <Input type="password" placeholder="UUID из ответа прокси" value={sid} onChange={(e) => setSid(e.target.value)} />
            <div style={{ display: 'flex', gap: 8 }}>
              <Btn variant="ghost" size="sm" onClick={() => { setState('signed_out'); setSid(''); }}>Отмена</Btn>
              <Btn variant="primary" size="sm" disabled={!sid.trim()} onClick={() => setState('signed_in')}>Подтвердить</Btn>
            </div>
          </div>
        )}
        {state === 'signed_in' && (
          <div style={{ display: 'grid', gap: 14 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <Avatar name="Вы" color="var(--sp1)" size={38} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 600, fontSize: 14 }}>Алексей Орлов</div>
                <div className="u-faint" style={{ fontSize: 12 }}>alexey@wotold.app · до 12.07.2026</div>
              </div>
              <Chip tone="ok" icon="check">Google</Chip>
            </div>
            <div><Btn variant="danger-ghost" size="sm" icon="external" onClick={() => setState('signed_out')}>Выйти</Btn></div>
          </div>
        )}
      </Panel>
    </>
  );
}

// ════════ 3 · PROCESSING ════════
const PRESETS = [
  { id: 'light', title: 'Лёгкий', meta: '~85% · быстро', q: 1 },
  { id: 'balanced', title: 'Сбалансированный', meta: '~93% · средне', q: 2, rec: true },
  { id: 'quality', title: 'Максимальный', meta: '~97% · медленно', q: 3 },
];
const MODELS = [
  { name: 'Распознавание речи · Сбалансированный', size: '1.5 ГБ', status: 'active' },
  { name: 'Распознавание речи · Максимальный', size: '3.1 ГБ', status: 'present' },
  { name: 'Разделение голосов', size: '6 МБ', status: 'downloading', pct: 42 },
  { name: 'Распознавание голоса', size: '14 МБ', status: 'missing' },
];
function ModelStatus({ m }) {
  if (m.status === 'active') return <Chip size="sm" tone="ok">активна</Chip>;
  if (m.status === 'present') return <><Chip size="sm" tone="line">есть</Chip><IconBtn icon="trash" size="sm" label="Удалить" /></>;
  if (m.status === 'downloading') return <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, color: 'var(--accent-text)', fontSize: 11.5 }}><Dot ring pulse color="var(--accent)" />{m.pct}%</span>;
  return <Btn variant="ghost" size="sm" icon="download">Скачать</Btn>;
}
function SecProcessing({ engine, setEngine, ping }) {
  const [preset, setPreset] = sU('balanced');
  const set = (v) => { setEngine(v); ping(); };
  const setP = (v) => { setPreset(v); ping(); };
  return (
    <>
      <SecHead id="processing" />
      <GroupLabel top={20}>Движок</GroupLabel>
      <div style={{ display: 'grid', gap: 8 }}>
        <OptionCard active={engine === 'local'} icon="cpu" title="Локально на устройстве" sub="Без сети. Звук не покидает устройство." quality={2} meta="приватно" onClick={() => set('local')} />
        <OptionCard active={engine === 'cloud'} icon="cloud" title="Облако Wotold · Pro" sub="Быстрее и точнее, через прокси Wotold." quality={3} meta="высокая точность" onClick={() => set('cloud')} />
      </div>

      {engine === 'local' && <>
        <GroupLabel>Сборка моделей</GroupLabel>
        <div style={{ display: 'grid', gap: 8 }}>
          {PRESETS.map((p) => <OptionCard key={p.id} active={preset === p.id} title={p.title} badge={p.rec ? 'Рекомендуем' : null} quality={p.q} meta={p.meta} onClick={() => setP(p.id)} />)}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 13px', marginTop: 12, background: 'var(--sunken)', borderRadius: 'var(--r-md)', fontSize: 12.5, maxWidth: 560 }}>
          <Icon name="cpu" size={15} style={{ color: 'var(--text-3)' }} />
          <span className="mono">Apple M2 · 16 ГБ · Metal</span>
          <Btn variant="ghost" size="sm" icon="refresh" style={{ marginLeft: 'auto' }}>Переоценить</Btn>
        </div>

        <GroupLabel>Хранилище · 4,6 ГБ</GroupLabel>
        <div className="set-table">
          <div className="set-trow set-thead"><span style={{ flex: 1 }}>Модель</span><span style={{ width: 56 }}>Размер</span><span style={{ width: 132, textAlign: 'right' }}>Статус</span></div>
          {MODELS.map((m, i) => (
            <div className="set-trow" key={i}>
              <span style={{ flex: 1, minWidth: 0 }} className="u-trunc">{m.name}</span>
              <span className="mono u-faint" style={{ width: 56 }}>{m.size}</span>
              <span style={{ width: 132, display: 'inline-flex', gap: 6, alignItems: 'center', justifyContent: 'flex-end' }}><ModelStatus m={m} /></span>
            </div>
          ))}
        </div>
      </>}

      {engine === 'cloud' && <>
        <GroupLabel>Дневная квота</GroupLabel>
        <div style={{ display: 'grid', gap: 14, maxWidth: 420 }}>
          <div><div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, marginBottom: 6 }}><span>Минуты</span><span className="mono u-muted">38 / 60</span></div><Progress value={63} /></div>
          <div><div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, marginBottom: 6 }}><span>Токены рекапа</span><span className="mono u-muted">142K / 500K</span></div><Progress value={28} /></div>
        </div>
      </>}

      <details style={{ marginTop: 22 }}>
        <summary className="u-muted" style={{ fontSize: 12.5, cursor: 'pointer' }}>Свои ключи провайдеров</summary>
        <div style={{ display: 'grid', gap: 12, marginTop: 12, maxWidth: 420 }}>
          <Field label="Ключ распознавания речи"><Input placeholder="••••• (введите, чтобы заменить)" className="mono" /></Field>
          <Field label="Ключ подытоживания"><Input placeholder="••••• (введите, чтобы заменить)" className="mono" /></Field>
          <p className="u-faint" style={{ fontSize: 11.5, margin: 0 }}>Хранятся в системном Keychain.</p>
        </div>
      </details>
    </>
  );
}

// ════════ 4 · PERMISSIONS ════════
const PERMS = [
  { id: 'mic', label: 'Микрофон', desc: 'Ваш голос.' },
  { id: 'audio', label: 'Системный звук', desc: 'Голос собеседника в Zoom, FaceTime, Telegram. После выдачи — перезапуск.' },
  { id: 'a11y', label: 'Универсальный доступ', desc: 'Глобальные горячие клавиши поверх других приложений.' },
];
const PERM_BADGE = { granted: ['ok', 'выдано'], denied: ['danger', 'отказано'], not_requested: ['line', 'не запрошено'], blocked: ['warn', 'заблокировано'] };
function SecPermissions() {
  const [st, setSt] = sU({ mic: 'granted', audio: 'not_requested', a11y: 'denied' });
  return (
    <>
      <SecHead id="permissions" />
      <div style={{ marginTop: 18 }}>
        {PERMS.map((p, i) => {
          const s = st[p.id];
          const [tone, txt] = PERM_BADGE[s];
          return (
            <Row key={p.id} label={p.label} hint={p.desc} align="top" last={i === PERMS.length - 1}>
              <Chip size="sm" tone={tone}>{txt}</Chip>
              {s !== 'granted' && <Btn variant="primary" size="sm" onClick={() => setSt((m) => ({ ...m, [p.id]: 'granted' }))}>Запросить</Btn>}
              {s !== 'granted' && <IconBtn icon="external" size="sm" label="Системные настройки" tip="Системные настройки" />}
              <IconBtn icon="refresh" size="sm" label="Обновить" tip="Обновить" />
            </Row>
          );
        })}
      </div>
    </>
  );
}

// ════════ 5 · RECORDING ════════
function SecRecording({ ping }) {
  const [stt, setStt] = sU('auto');
  const [recap, setRecap] = sU('auto');
  const [hkStart, setHkStart] = sU('⌘⇧R');
  const [hkPause, setHkPause] = sU('⌘⇧P');
  const [autoSug, setAutoSug] = sU(false);
  const [cooldown, setCooldown] = sU('5');
  const w = (fn) => (v) => { fn(v); ping(); };
  return (
    <>
      <SecHead id="recording" />
      <GroupLabel top={20}>Языки</GroupLabel>
      <Row label="Распознавание речи" hint="На тихом микрофоне для русских звонков выберите «Русский»."><Select value={stt} onChange={w(setStt)} options={STT_LANGS} width={150} /></Row>
      <Row label="Рекап и задачи" hint="«Как в звонке» = язык распознанной речи." last><Select value={recap} onChange={w(setRecap)} options={RECAP_LANGS} width={150} /></Row>

      <GroupLabel>Горячие клавиши</GroupLabel>
      <Row label="Старт / стоп"><HotkeyCapture value={hkStart} onChange={w(setHkStart)} /></Row>
      <Row label="Пауза / продолжить" hint="Только во время активной записи." last><HotkeyCapture value={hkPause} onChange={w(setHkPause)} /></Row>

      <GroupLabel>Авто-определение</GroupLabel>
      <Row label="Предлагать запись" hint="Уведомление «Записать?» при обнаружении звонка. По умолчанию выключено для приватности." align="top" last={!autoSug}>
        <Switch checked={autoSug} onChange={w(setAutoSug)} />
      </Row>
      {autoSug && <Row label="Не предлагать снова" hint="Минимальный интервал для того же приложения." last><Select value={cooldown} onChange={w(setCooldown)} width={120} options={[{ value: '3', label: '3 мин' }, { value: '5', label: '5 мин' }, { value: '10', label: '10 мин' }, { value: '15', label: '15 мин' }]} /></Row>}
    </>
  );
}

// ════════ 6 · SPEAKERS ════════
function SecSpeakers({ ping }) {
  const [model, setModel] = sU('valid');
  const [pyannote, setPyannote] = sU('missing');
  const [autoBind, setAutoBind] = sU(false);
  const [multi, setMulti] = sU(true);
  const VM = { valid: ['ok', 'установлен'], missing: ['line', 'нет'], corrupted: ['warn', 'повреждён'], downloading: ['accent', 'загрузка'] };
  const [tone, txt] = VM[model];
  const w = (fn) => (v) => { fn(v); ping(); };
  return (
    <>
      <SecHead id="speakers" />
      <Panel style={{ padding: 14, maxWidth: 560, marginTop: 18, display: 'flex', alignItems: 'center', gap: 12 }}>
        <span style={{ width: 32, height: 32, borderRadius: 'var(--r-sm)', flex: '0 0 auto', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', background: model === 'valid' ? 'var(--accent)' : 'var(--sunken)', color: model === 'valid' ? 'var(--on-accent)' : 'var(--text-3)' }}><Icon name="users" size={17} /></span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 600, fontSize: 13.5 }}>Голосовой модуль</div>
          <div className="u-faint" style={{ fontSize: 11.5 }}>14 МБ · нужен для узнавания по голосу</div>
        </div>
        <Chip size="sm" tone={tone}>{txt}</Chip>
        {model !== 'valid' ? <Btn variant="primary" size="sm" icon="download" onClick={() => { setModel('valid'); ping(); }}>Скачать</Btn>
          : <IconBtn icon="trash" size="sm" label="Удалить" onClick={() => { setModel('missing'); ping(); }} />}
      </Panel>

      <div style={{ marginTop: 8 }}>
        <Row label="Привязывать спикеров к контактам" hint="Только при высокой уверенности. Можно отменить в звонке." align="top" disabled={model !== 'valid'}>
          <Switch checked={autoBind} onChange={(v) => model === 'valid' && w(setAutoBind)(v)} />
        </Row>
        <Row label="Несколько голосов на микрофоне" hint="Для живых встреч в одной комнате. Медленнее на ~10–20%." align="top" last>
          {pyannote === 'present'
            ? <Switch checked={multi} onChange={w(setMulti)} />
            : <Btn variant="default" size="sm" icon="download" onClick={() => { setPyannote('present'); ping(); }}>Модуль · 6 МБ</Btn>}
        </Row>
      </div>
    </>
  );
}

// ════════ 7 · LABS ════════
function SecLabs({ ping }) {
  const [newFmt, setNewFmt] = sU(true);
  const [draft, setDraft] = sU(false);
  const [nspk, setNspk] = sU('auto');
  const w = (fn) => (v) => { fn(v); ping(); };
  return (
    <>
      <SecHead id="labs" />
      <div style={{ marginTop: 18 }}>
        <Row label="Новый формат саммари" hint="Тип звонка, цитаты, решения. Выключите при проблемах." align="top"><Switch checked={newFmt} onChange={w(setNewFmt)} /></Row>
        <Row label="Ускорение генерации" hint="Черновая модель параллельно с основной. 2–3×. Нужна сборка «Максимальный» + 380 МБ." align="top"><Switch checked={draft} onChange={w(setDraft)} /></Row>
        <Row label="Число собеседников" hint="Кроме вас. Задайте точно, если авто-разделение промахивается. Лимит 3." align="top" last>
          <Select value={nspk} onChange={w(setNspk)} width={150} options={[{ value: 'auto', label: 'Авто' }, { value: '2', label: '2' }, { value: '3', label: '3' }]} />
        </Row>
      </div>
    </>
  );
}

// ════════ 8 · MAINTENANCE ════════
function SecMaintenance() {
  const [phase, setPhase] = sU('idle');
  const run = () => { setPhase('working'); setTimeout(() => setPhase('done'), 1800); };
  return (
    <>
      <SecHead id="maintenance" />
      <div style={{ marginTop: 18 }}>
        <Row label="Пустые рекапы" hint="Пересоздать саммари для звонков, обработанных раньше, у которых рекап не сформировался." align="top" last>
          {phase === 'working'
            ? <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, color: 'var(--accent-text)', fontSize: 12.5 }}><Wave bars={4} color="var(--accent-text)" height={14} />3 / 7 <Btn variant="ghost" size="sm" onClick={() => setPhase('idle')}>Стоп</Btn></span>
            : phase === 'done'
              ? <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, color: 'var(--ok)', fontSize: 12.5 }}><Icon name="checkCircle" size={15} />7 готово</span>
              : <Btn variant="default" size="sm" icon="refresh" onClick={run}>Пересоздать</Btn>}
        </Row>
      </div>
    </>
  );
}

// ════════ 9 · PRIVACY ════════
function SecPrivacy() {
  const [open, setOpen] = sU(false);
  const [done, setDone] = sU(false);
  return (
    <>
      <SecHead id="privacy" />
      <div style={{ marginTop: 18 }}>
        <Row label="Удалить все данные" hint="Записи, контакты, voice samples, сессию и ключи. Необратимо." align="top" last>
          {done ? <Chip size="sm" tone="ok" icon="check">удалено</Chip> : <Btn variant="danger-ghost" size="sm" icon="trash" onClick={() => setOpen(true)}>Удалить</Btn>}
        </Row>
      </div>
      <Modal open={open} onClose={() => setOpen(false)} title="Полная очистка"
        footer={<><Btn variant="ghost" onClick={() => setOpen(false)}>Отмена</Btn><Btn variant="danger" icon="trash" onClick={() => { setOpen(false); setDone(true); }}>Удалить всё</Btn></>}>
        <p style={{ margin: 0, lineHeight: 1.6 }}>Будут навсегда удалены все записи и аудио, контакты и voice samples, сессия входа и BYO-ключи.</p>
        <p style={{ marginTop: 10, color: 'var(--danger-text)', fontWeight: 550 }}>Действие необратимо.</p>
      </Modal>
    </>
  );
}

// ════════ SHELL ════════
function SettingsView(props) {
  const [sec, setSec] = sU('appearance');
  const [saved, setSaved] = sU(false);
  const tRef = sR(null);
  const ping = () => { setSaved(true); clearTimeout(tRef.current); tRef.current = setTimeout(() => setSaved(false), 1500); };
  const cur = SET_SECS.find((s) => s.id === sec);
  return (
    <>
      <div className="view-head">
        <Icon name="settings" size={17} style={{ color: 'var(--text-3)' }} />
        <span className="u-faint" style={{ fontSize: 'var(--t-13)' }}>Настройки</span>
        <Icon name="chevronRight" size={13} style={{ color: 'var(--text-faint)' }} />
        <span style={{ fontWeight: 600 }}>{cur.label}</span>
        <span className="set-saved" style={{ marginLeft: 10, opacity: saved ? 1 : 0 }}>✓ Сохранено</span>
      </div>
      <div className="view-body">
        <aside className="rrail" style={{ borderLeft: 'none', borderRight: '1px solid var(--border)', width: 300, flex: '0 0 300px' }}>
          <div className="scroll" style={{ flex: 1, minHeight: 0, padding: 8 }}>
            {SET_SECS.map((s) => (
              <button key={s.id} className="navitem" data-active={sec === s.id ? 'true' : undefined} onClick={() => setSec(s.id)}>
                <span className="nav-ico"><Icon name={s.icon} size={16} /></span>
                <span className="nav-label">{s.label}</span>
              </button>
            ))}
          </div>
        </aside>
        <div className="content scroll" key={sec}>
          <div className="doc" style={{ paddingTop: 28, paddingBottom: 80 }}>
            {sec === 'appearance' && <SecAppearance {...props} ping={ping} />}
            {sec === 'account' && <SecAccount />}
            {sec === 'processing' && <SecProcessing engine={props.engine} setEngine={props.setEngine} ping={ping} />}
            {sec === 'permissions' && <SecPermissions />}
            {sec === 'recording' && <SecRecording ping={ping} />}
            {sec === 'speakers' && <SecSpeakers ping={ping} />}
            {sec === 'labs' && <SecLabs ping={ping} />}
            {sec === 'maintenance' && <SecMaintenance />}
            {sec === 'privacy' && <SecPrivacy />}
          </div>
        </div>
      </div>
    </>
  );
}

Object.assign(window, { SettingsView });
