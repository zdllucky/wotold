/* eslint-disable */
// WOTOLD · screens B — Contacts (two-pane) + Settings
const { useState: useStateB } = React;

// ════════ CONTACTS ════════
function makeSamples(c) {
  const srcs = WK_CALLS.filter((call) => call.parts.includes(c.sp));
  const n = Math.min(5, Math.max(1, Math.round(c.calls / 4)));
  return Array.from({ length: n }, (_, i) => {
    const call = srcs[i % Math.max(1, srcs.length)] || null;
    return { id: c.id + '-s' + i, dur: 6 + ((i * 7 + c.calls) % 18), seed: i * 13 + c.calls * 7,
      src: call ? call.title : 'Запись', when: call ? call.when : '2026-06-21T10:00:00' };
  });
}

function VoiceSamples({ contact, color }) {
  const [samples, setSamples] = useStateB(() => makeSamples(contact));
  const [playing, setPlaying] = useStateB(null);
  return (
    <Panel style={{ padding: 7 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '3px 7px 6px' }}>
        <span className="u-faint" style={{ fontSize: 11 }}>можно прослушать или удалить</span>
        <Chip size="sm" tone="line">{samples.length}/5</Chip>
      </div>
      {samples.length === 0 && <div className="u-faint" style={{ padding: '10px 7px', fontSize: 13, textAlign: 'center' }}>Нет сэмплов голоса</div>}
      {samples.map((s) => {
        const on = playing === s.id;
        return (
          <div key={s.id} className="lrow" style={{ padding: '6px 7px', gap: 9, alignItems: 'center', borderRadius: 'var(--r-sm)' }}>
            <button className="iconbtn" onClick={() => setPlaying(on ? null : s.id)} aria-label={on ? 'Пауза' : 'Прослушать'}
              style={{ background: on ? 'var(--accent)' : 'var(--sunken)', color: on ? 'var(--on-accent)' : 'var(--text-2)', flex: '0 0 auto' }}>
              <Icon name={on ? 'pause' : 'play'} size={14} />
            </button>
            <div style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: 'flex', alignItems: 'center', gap: 2, height: 16 }}>
                {Array.from({ length: 32 }).map((_, i) => (
                  <i key={i} style={{ flex: '0 0 auto', width: 2.5, borderRadius: 2, background: color, opacity: on ? .95 : .5,
                    height: 3 + ((i * 7 + s.seed) % 12), animation: on ? 'wbar .9s ease-in-out infinite' : 'none', animationDelay: (i * 0.04) + 's' }} />
                ))}
              </span>
              <div style={{ display: 'flex', alignItems: 'baseline', gap: 6, marginTop: 4 }}>
                <span className="u-faint u-trunc" style={{ fontSize: 10.5 }}>{s.src}</span>
                <span className="u-faint mono" style={{ fontSize: 10.5, marginLeft: 'auto', flex: '0 0 auto' }}>{fmtDay(s.when)} · {fmtDur(s.dur)}</span>
              </div>
            </div>
            <IconBtn icon="trash" size="sm" label="Удалить сэмпл" onClick={() => setSamples((a) => a.filter((x) => x.id !== s.id))} />
          </div>
        );
      })}
    </Panel>
  );
}

function AddContactModal({ open, onClose }) {
  const [f, setF] = useStateB({ name: '', title: '', email: '', phone: '', org: '', tags: '' });
  const set = (k) => (e) => setF((p) => ({ ...p, [k]: e.target.value }));
  return (
    <Modal open={open} onClose={onClose} title="Новый контакт" width={480}
      footer={<><Btn variant="ghost" onClick={onClose}>Отмена</Btn><Btn variant="primary" icon="check" onClick={onClose} disabled={!f.name.trim()}>Добавить</Btn></>}>
      <div style={{ display: 'grid', gap: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span className="u-faint" style={{ fontSize: 12, marginRight: 'auto' }}>Импорт из приложения</span>
          <Btn variant="default" size="sm" icon="users">Google</Btn>
          <Btn variant="default" size="sm" icon="users">iCloud</Btn>
          <Btn variant="default" size="sm" icon="upload">vCard</Btn>
        </div>
        <div style={{ height: 1, background: 'var(--border)' }} />
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <Avatar name={f.name || '?'} color="var(--sp2)" size={44} />
          <div className="u-faint" style={{ fontSize: 12, lineHeight: 1.5 }}>Поля по стандарту vCard — корректно синхронизируются с внешними контактами. Голос привяжется после первого звонка.</div>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          <Field label="Имя"><Input placeholder="Анна Лебедева" value={f.name} onChange={set('name')} autoFocus /></Field>
          <Field label="Должность"><Input placeholder="Маркетолог" value={f.title} onChange={set('title')} /></Field>
          <Field label="Email"><Input placeholder="anna@company.com" value={f.email} onChange={set('email')} /></Field>
          <Field label="Телефон"><Input placeholder="+7 900 000-00-00" value={f.phone} onChange={set('phone')} /></Field>
        </div>
        <Field label="Компания"><Input placeholder="Контур" value={f.org} onChange={set('org')} /></Field>
        <Field label="Теги" hint="через запятую"><Input placeholder="партнёр, маркетинг" value={f.tags} onChange={set('tags')} /></Field>
      </div>
    </Modal>
  );
}

function ContactsView({ onOpenCall }) {
  const [sel, setSel] = useStateB(WK_CONTACTS[0].id);
  const [q, setQ] = useStateB('');
  const [showAdd, setShowAdd] = useStateB(false);
  const list = WK_CONTACTS.filter((c) => c.name.toLowerCase().includes(q.toLowerCase()));
  const c = WK_CONTACTS.find((x) => x.id === sel) || list[0];
  const s = WK_SPEAKERS[c.sp];
  const theirCalls = WK_CALLS.filter((call) => call.parts.includes(c.sp));

  return (
    <>
      <div className="view-head">
        <Icon name="users" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650 }}>Контакты</span>
        <Chip size="sm" tone="line">{WK_CONTACTS.length}</Chip>
        <div style={{ flex: '1 1 auto', maxWidth: 300, marginLeft: 10 }}>
          <Input icon="search" placeholder="Поиск контактов…" value={q} onChange={(e) => setQ(e.target.value)} size="sm" />
        </div>
        <div style={{ flex: 1 }} />
        <Btn variant="primary" size="sm" icon="plus" onClick={() => setShowAdd(true)}>Добавить</Btn>
      </div>
      <div className="view-body">
        <aside className="rrail" style={{ borderLeft: 'none', borderRight: '1px solid var(--border)', width: 300, flex: '0 0 300px' }}>
          <div className="scroll" style={{ flex: 1, minHeight: 0, padding: 6 }}>
            {list.length === 0 && <div className="u-faint" style={{ padding: 16, fontSize: 13, textAlign: 'center' }}>Не найдено</div>}
            {list.map((k) => {
              const ks = WK_SPEAKERS[k.sp];
              return (
                <button key={k.id} className="lrow" onClick={() => setSel(k.id)}
                  style={k.id === sel ? { background: 'var(--active)' } : null}>
                  <Avatar name={ks.name} color={ks.color} size={32} />
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div className="u-trunc" style={{ fontWeight: 550 }}>{k.name}</div>
                    <div className="u-faint u-trunc" style={{ fontSize: 11.5 }}>{k.role}</div>
                  </div>
                  {!k.confirmed && <Dot color="var(--warn)" />}
                </button>
              );
            })}
          </div>
        </aside>

        <div className="content scroll fade" key={c.id}>
          <div className="doc" style={{ paddingTop: 28 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14, marginBottom: 18 }}>
              <Avatar name={s.name} color={s.color} size={56} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <h1 className="doc-title" style={{ fontSize: 'var(--t-18)' }}>{c.name}</h1>
                <div className="u-muted">{c.role}</div>
              </div>
              {c.confirmed
                ? <Chip tone="ok" icon="check">Голос подтверждён</Chip>
                : <Btn variant="soft" size="sm" icon="check">Подтвердить голос</Btn>}
            </div>

            <div style={{ display: 'flex', gap: 6, marginBottom: 18 }}>
              {c.tags.map((t) => <Chip key={t} tone="line" icon="tag">{t}</Chip>)}
            </div>

            <div className="rrail-sec" style={{ marginTop: 0 }}>Недавние звонки</div>
            {theirCalls.map((call) => (
              <button key={call.id} className="lrow" onClick={() => onOpenCall(call.id)}>
                <StatusCell call={call} />
                <span style={{ flex: 1, minWidth: 0 }} className="u-trunc">{call.title}</span>
                <span className="u-faint mono" style={{ fontSize: 11.5, whiteSpace: 'nowrap' }}>{fmtDay(call.when)}</span>
              </button>
            ))}

            <div className="rrail-sec">Сэмплы голоса</div>
            <VoiceSamples contact={c} color={s.color} />
          </div>
        </div>
      </div>
      <AddContactModal open={showAdd} onClose={() => setShowAdd(false)} />
    </>
  );
}

// ════════ SETTINGS ════════
const ACCENTS = [{ id: 'iris', label: 'Iris', color: '#5B5BD6' }, { id: 'teal', label: 'Teal', color: '#0E9888' }, { id: 'amber', label: 'Amber', color: '#C2710C' }];

const SETTINGS_SECS = [
  { id: 'appearance', label: 'Внешний вид', icon: 'sun', sub: 'Тема, акцент, плотность' },
  { id: 'processing', label: 'Обработка', icon: 'cpu', sub: 'Движок и ключи' },
  { id: 'notifications', label: 'Уведомления', icon: 'alert', sub: 'Что присылать' },
  { id: 'storage', label: 'Хранилище', icon: 'folder', sub: 'Записи и модули' },
  { id: 'account', label: 'Аккаунт', icon: 'user', sub: 'Профиль и сессия' },
];

function SettingsView(props) {
  const [sec, setSec] = useStateB('appearance');
  const cur = SETTINGS_SECS.find((s) => s.id === sec);
  return (
    <>
      <div className="view-head">
        <Icon name="settings" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650 }}>Настройки</span>
      </div>
      <div className="view-body">
        <aside className="rail" style={{ borderLeft: 'none', borderRight: '1px solid var(--border)', width: 250, flex: '0 0 250px' }}>
          <div className="scroll" style={{ flex: 1, minHeight: 0, padding: 8 }}>
            {SETTINGS_SECS.map((s) => (
              <button key={s.id} className="lrow" onClick={() => setSec(s.id)} style={sec === s.id ? { background: 'var(--active)' } : null}>
                <span style={{ width: 30, height: 30, borderRadius: 'var(--r-sm)', flex: '0 0 auto', display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                  background: sec === s.id ? 'var(--accent-soft)' : 'var(--sunken)', color: sec === s.id ? 'var(--accent-text)' : 'var(--text-3)' }}>
                  <Icon name={s.icon} size={16} />
                </span>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="u-trunc" style={{ fontWeight: 550, fontSize: 13.5 }}>{s.label}</div>
                  <div className="u-faint u-trunc" style={{ fontSize: 11.5 }}>{s.sub}</div>
                </div>
              </button>
            ))}
          </div>
        </aside>

        <div className="content scroll fade" key={sec}>
          <div className="doc" style={{ paddingTop: 28, maxWidth: 640 }}>
            <h1 className="doc-title" style={{ fontSize: 'var(--t-18)', marginBottom: 20 }}>{cur.label}</h1>
            {sec === 'appearance' && <SecAppearance {...props} />}
            {sec === 'processing' && <SecProcessing {...props} />}
            {sec === 'notifications' && <SecNotifications {...props} />}
            {sec === 'storage' && <SecStorage />}
            {sec === 'account' && <SecAccount />}
          </div>
        </div>
      </div>
    </>
  );
}

function SecAppearance({ theme, setTheme, accent, setAccent, density, setDensity }) {
  return (
    <div style={{ display: 'grid', gap: 18, paddingBottom: 40 }}>
      <Field label="Тема">
        <Segmented value={theme} onChange={setTheme} options={[
          { value: 'light', label: 'Светлая', icon: 'sun' }, { value: 'dark', label: 'Тёмная', icon: 'moon' }, { value: 'system', label: 'Системная' },
        ]} />
      </Field>
      <Field label="Акцент">
        <div style={{ display: 'flex', gap: 10 }}>
          {ACCENTS.map((a) => (
            <button key={a.id} onClick={() => setAccent(a.id)} className="tip" data-tip={a.label}
              style={{ width: 30, height: 30, borderRadius: '50%', background: a.color,
                boxShadow: accent === a.id ? '0 0 0 2px var(--bg), 0 0 0 4px var(--accent)' : '0 0 0 1px var(--border-strong)' }} />
          ))}
        </div>
      </Field>
      <Field label="Плотность">
        <Segmented value={density} onChange={setDensity} options={[{ value: 'cozy', label: 'Просторно' }, { value: 'compact', label: 'Компактно' }]} />
      </Field>
    </div>
  );
}

function SecProcessing({ engine, setEngine }) {
  return (
    <div style={{ paddingBottom: 40 }}>
      <p className="u-muted" style={{ fontSize: 13, marginTop: -6, marginBottom: 14 }}>Где расшифровываются звонки. По умолчанию — на устройстве, без отправки звука.</p>
      <div style={{ display: 'grid', gap: 8 }}>
        {Object.values(WK_ENGINES).map((e) => {
          const active = engine === e.id;
          return (
            <button key={e.id} onClick={() => setEngine(e.id)} className="panel" style={{
              display: 'flex', gap: 12, padding: 12, textAlign: 'left', alignItems: 'flex-start',
              borderColor: active ? 'var(--accent)' : 'var(--border)', boxShadow: active ? '0 0 0 2px var(--accent-soft)' : 'none' }}>
              <span style={{ width: 30, height: 30, borderRadius: 'var(--r-sm)', flex: '0 0 auto', display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                background: active ? 'var(--accent)' : 'var(--sunken)', color: active ? 'var(--on-accent)' : 'var(--text-3)' }}>
                <Icon name={e.icon} size={17} />
              </span>
              <span style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 8 }}>
                <span style={{ display: 'flex', alignItems: 'center', gap: 8, whiteSpace: 'nowrap' }}>
                  <b style={{ fontWeight: 600 }}>{e.label}</b><span className="u-faint" style={{ fontSize: 12 }}>· {e.sub}</span>
                  {active && <Icon name="check" size={15} style={{ color: 'var(--accent-text)', marginLeft: 'auto' }} />}
                </span>
                <span style={{ display: 'flex', gap: 6 }}>{e.facts.map((f) => <Chip key={f} size="sm" tone="line">{f}</Chip>)}</span>
              </span>
            </button>
          );
        })}
      </div>
      <details style={{ marginTop: 12 }}>
        <summary className="u-muted" style={{ fontSize: 12.5, cursor: 'pointer' }}>Свои ключи (BYO)</summary>
        <div style={{ display: 'grid', gap: 12, marginTop: 12 }}>
          <Field label="Ключ распознавания речи"><Input placeholder="sk-•••••••••••••••" className="mono" /></Field>
          <Field label="Ключ подытоживания"><Input placeholder="sk-ant-•••••••••••" className="mono" /></Field>
        </div>
      </details>
    </div>
  );
}

function SecNotifications({ notif, setNotif }) {
  return (
    <div style={{ display: 'grid', gap: 2, paddingBottom: 40 }}>
      {[['ready', 'Звонок обработан', 'Когда расшифровка и рекап готовы'], ['error', 'Ошибка обработки', 'Если обработка не завершилась'], ['quota', 'Квота облака заканчивается', 'Остаётся менее 10 минут']].map(([k, label, sub]) => (
        <div key={k} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '11px 0', borderBottom: '1px solid var(--border)' }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 13.5, fontWeight: 500 }}>{label}</div>
            <div className="u-faint" style={{ fontSize: 11.5 }}>{sub}</div>
          </div>
          <Switch checked={notif[k]} onChange={(v) => setNotif({ ...notif, [k]: v })} />
        </div>
      ))}
    </div>
  );
}

function SecStorage() {
  return (
    <div style={{ paddingBottom: 40 }}>
      <div style={{ display: 'flex', gap: 28, alignItems: 'flex-end', marginBottom: 12 }}>
        <div><div className="mono" style={{ fontSize: 24, fontWeight: 600 }}>38<span className="u-faint" style={{ fontSize: 14 }}> / 60 мин</span></div><div className="u-faint" style={{ fontSize: 11.5 }}>облако · сегодня</div></div>
        <div><div className="mono" style={{ fontSize: 24, fontWeight: 600 }}>2,4<span className="u-faint" style={{ fontSize: 14 }}> ГБ</span></div><div className="u-faint" style={{ fontSize: 11.5 }}>модули на устройстве</div></div>
      </div>
      <div style={{ maxWidth: 320, marginBottom: 18 }}><Progress value={63} /></div>
      <div style={{ display: 'flex', gap: 8 }}>
        <Btn variant="default" size="sm" icon="folder">Папка записей</Btn>
        <Btn variant="default" size="sm" icon="refresh">Проверить модули</Btn>
      </div>
    </div>
  );
}

function SecAccount() {
  return (
    <div style={{ paddingBottom: 40 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 14, marginBottom: 20 }}>
        <Avatar name="Вы" color="var(--sp1)" size={48} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 600, fontSize: 15 }}>Вы</div>
          <div className="u-faint" style={{ fontSize: 12.5 }}>you@wotold.app</div>
        </div>
        <Btn variant="default" size="sm" icon="edit">Изменить</Btn>
      </div>
      <div style={{ display: 'grid', gap: 2 }}>
        {[['Тариф', 'Pro · до 60 мин облака в день'], ['Устройство', 'macOS · этот компьютер']].map(([k, v]) => (
          <div key={k} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '11px 0', borderBottom: '1px solid var(--border)' }}>
            <span className="u-muted" style={{ width: 110, fontSize: 13 }}>{k}</span>
            <span style={{ flex: 1, fontSize: 13.5 }}>{v}</span>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 18 }}><Btn variant="danger-ghost" size="sm" icon="external">Выйти из аккаунта</Btn></div>
    </div>
  );
}

Object.assign(window, { ContactsView });
