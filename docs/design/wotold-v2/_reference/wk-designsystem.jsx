/* eslint-disable */
// WOTOLD · Design System reference (debug) — живой каталог токенов и компонентов.
// Открывается из сайдбара (иконка </>). Нужен, чтобы не галлюцинировать компоненты.
const { useState: uDs } = React;

function DsSection({ title, note, children }) {
  return (
    <section style={{ marginBottom: 40 }}>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, marginBottom: 14, borderBottom: '1px solid var(--border)', paddingBottom: 8 }}>
        <h2 style={{ fontSize: 16, fontWeight: 650, margin: 0 }}>{title}</h2>
        {note && <span className="u-faint" style={{ fontSize: 12 }}>{note}</span>}
      </div>
      {children}
    </section>
  );
}
function DsRow({ label, children }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '130px 1fr', gap: 16, alignItems: 'center', padding: '9px 0', borderBottom: '1px solid var(--border-soft, var(--border))' }}>
      <span className="u-faint mono" style={{ fontSize: 11.5 }}>{label}</span>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>{children}</div>
    </div>
  );
}

const DS_TOKENS = [
  ['--bg', 'фон'], ['--panel', 'панель'], ['--sunken', 'утоплен.'], ['--hover', 'ховер'], ['--active', 'актив'],
  ['--border', 'границы'], ['--border-strong', 'границы+'],
  ['--text', 'текст'], ['--text-2', 'текст-2'], ['--text-3', 'текст-3'], ['--text-faint', 'бледный'],
  ['--accent', 'акцент'], ['--accent-soft', 'акц-фон'], ['--danger', 'опасн.'], ['--ok', 'успех'], ['--warn', 'предупр.'],
];
const DS_SPK = ['--sp1', '--sp2', '--sp3', '--sp4', '--sp5'];

function Swatch({ varname, label }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 5, width: 70 }}>
      <div style={{ height: 40, borderRadius: 'var(--r-sm)', background: `var(${varname})`, border: '1px solid var(--border)' }} />
      <div style={{ fontSize: 10, fontWeight: 600 }}>{label}</div>
      <div className="u-faint mono" style={{ fontSize: 9 }}>{varname}</div>
    </div>
  );
}

function DesignSystemView({ theme, setTheme, accent, setAccent, density, setDensity }) {
  const [seg, setSeg] = uDs('a');
  const [sw1, setSw1] = uDs(true);
  const [sw2, setSw2] = uDs(false);
  const [tab, setTab] = uDs('one');
  const [selVal, setSelVal] = uDs('ru');
  const [optSel, setOptSel] = uDs('cloud');
  const [tg, setTg] = uDs(true);
  const iconNames = Object.keys(WK_ICONS);

  return (
    <>
      <div className="view-head">
        <Icon name="code" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650 }}>Дизайн-система</span>
        <Chip size="sm" tone="accent">debug</Chip>
        <div style={{ flex: 1 }} />
        <Segmented value={theme} onChange={setTheme} options={[{ value: 'light', label: 'Светлая', icon: 'sun' }, { value: 'dark', label: 'Тёмная', icon: 'moon' }]} />
        <div className="vh-sep" />
        <Segmented value={density} onChange={setDensity} options={[{ value: 'cozy', label: 'Просторно' }, { value: 'compact', label: 'Компактно' }]} />
      </div>

      <div className="scroll" style={{ flex: 1, minHeight: 0 }}>
        <div style={{ maxWidth: 880, margin: '0 auto', padding: '28px 32px 80px' }}>

          <DsSection title="Цвета" note="токены темы — меняются вместе со светлой и тёмной">
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
              {DS_TOKENS.map(([v, l]) => <Swatch key={v} varname={v} label={l} />)}
            </div>
            <div className="u-faint mono" style={{ fontSize: 11, marginBottom: 8 }}>палитра говорящих (--sp1..5)</div>
            <div style={{ display: 'flex', gap: 10 }}>
              {DS_SPK.map((v) => <div key={v} style={{ width: 44, height: 44, borderRadius: 'var(--r-sm)', background: `var(${v})` }} />)}
            </div>
          </DsSection>

          <DsSection title="Типографика" note="Hanken Grotesk · IBM Plex Mono">
            <div style={{ fontSize: 40, fontWeight: 700, letterSpacing: '-.02em', lineHeight: 1.1 }}>Заголовок дисплей · 40/700</div>
            <div style={{ fontSize: 22, fontWeight: 700, letterSpacing: '-.01em', marginTop: 12 }}>Заголовок раздела · 22/700</div>
            <div style={{ fontSize: 18, fontWeight: 600, marginTop: 12 }}>Подзаголовок · 18/600</div>
            <div style={{ fontSize: 13, color: 'var(--text-2)', marginTop: 12, maxWidth: 540 }}>Основной текст · 13/400. Нейтральный деловой тон, без разговорности. Расшифровка, разделение голосов и рекап формируются автоматически.</div>
            <div className="mono" style={{ fontSize: 13, marginTop: 12, color: 'var(--text-3)' }}>mono · 0:42 · 38/60 мин · sk-•••</div>
          </DsSection>

          <DsSection title="Кнопки">
            <DsRow label="variant">
              <Btn variant="primary">Primary</Btn>
              <Btn variant="default">Default</Btn>
              <Btn variant="ghost">Ghost</Btn>
              <Btn variant="soft">Soft</Btn>
              <Btn variant="danger">Danger</Btn>
              <Btn variant="danger-ghost">Danger ghost</Btn>
            </DsRow>
            <DsRow label="с иконкой">
              <Btn variant="primary" icon="mic">Записать</Btn>
              <Btn variant="default" icon="download">Экспорт</Btn>
              <Btn variant="soft" icon="check">Подтвердить</Btn>
            </DsRow>
            <DsRow label="size">
              <Btn variant="default" size="sm">Small</Btn>
              <Btn variant="default">Medium</Btn>
              <Btn variant="default" size="lg">Large</Btn>
            </DsRow>
            <DsRow label="state">
              <Btn variant="primary" disabled>Disabled</Btn>
              <Btn variant="primary" block style={{ maxWidth: 180 }}>Block</Btn>
            </DsRow>
            <DsRow label="icon-кнопки">
              <IconBtn icon="dots" label="меню" />
              <IconBtn icon="trash" label="удалить" />
              <IconBtn icon="settings" active label="актив" />
              <IconBtn icon="refresh" size="sm" label="sm" />
              <IconBtn icon="folder" size="lg" label="lg" />
            </DsRow>
          </DsSection>

          <DsSection title="Чипы и теги">
            <DsRow label="tone">
              <Chip>neutral</Chip>
              <Chip tone="accent">accent</Chip>
              <Chip tone="ok">ok</Chip>
              <Chip tone="danger">danger</Chip>
              <Chip tone="warn">warn</Chip>
              <Chip tone="line">line</Chip>
            </DsRow>
            <DsRow label="с иконкой">
              <Chip icon="cpu" tone="ok">На устройстве</Chip>
              <Chip icon="cloud" tone="accent">Облако</Chip>
              <Chip icon="clock">0:42</Chip>
              <Chip icon="tag" tone="line">партнёр</Chip>
            </DsRow>
            <DsRow label="size sm">
              <Chip size="sm" tone="accent">обработка</Chip>
              <Chip size="sm" tone="danger">ошибка</Chip>
              <Chip size="sm" tone="line">5/5</Chip>
            </DsRow>
          </DsSection>

          <DsSection title="Аватары">
            <DsRow label="speaker">
              <Avatar name="Вы" color="var(--sp1)" size={32} />
              <Avatar name="Арман Сулейменов" color="var(--sp2)" size={32} />
              <Avatar name="Елена Ковач" color="var(--sp3)" size={32} />
              <Avatar name="?" color="var(--text-faint)" size={32} />
            </DsRow>
            <DsRow label="size">
              <Avatar name="АА" color="var(--sp4)" size={20} />
              <Avatar name="АА" color="var(--sp4)" size={28} />
              <Avatar name="АА" color="var(--sp4)" size={44} />
            </DsRow>
            <DsRow label="group">
              <AvatarGroup items={[{ name: 'Вы', color: 'var(--sp1)' }, { name: 'Арман', color: 'var(--sp2)' }, { name: 'Елена', color: 'var(--sp3)' }, { name: 'Дмитрий', color: 'var(--sp4)' }, { name: 'Гость', color: 'var(--sp5)' }]} max={4} size={24} />
            </DsRow>
          </DsSection>

          <DsSection title="Поля ввода">
            <div style={{ display: 'grid', gap: 14, maxWidth: 360 }}>
              <Field label="Текстовое поле" hint="подсказка под полем"><Input icon="search" placeholder="Поиск…" /></Field>
              <Field label="Без иконки"><Input placeholder="Имя контакта" /></Field>
              <Field label="Многострочное"><Textarea placeholder="Заметка…" rows={3} /></Field>
            </div>
          </DsSection>

          <DsSection title="Контролы">
            <DsRow label="Segmented">
              <Segmented value={seg} onChange={setSeg} options={[{ value: 'a', label: 'Список', icon: 'list' }, { value: 'b', label: 'Карточки', icon: 'grid' }, { value: 'c', label: 'Календарь', icon: 'calendar' }]} />
            </DsRow>
            <DsRow label="Select">
              <Select value={selVal} onChange={setSelVal} width={240} options={[{ value: 'ru', label: 'Русский' }, { value: 'en', label: 'English' }, { value: 'kk', label: 'Қазақша' }]} />
            </DsRow>
            <DsRow label="Switch">
              <Switch checked={sw1} onChange={setSw1} /><span className="u-faint" style={{ fontSize: 12 }}>вкл</span>
              <Switch checked={sw2} onChange={setSw2} /><span className="u-faint" style={{ fontSize: 12 }}>выкл</span>
            </DsRow>
            <DsRow label="Tabs">
              <Tabs tabs={[{ value: 'one', label: 'Транскрипт', icon: 'doc' }, { value: 'two', label: 'Рекап', icon: 'sparkle' }, { value: 'three', label: 'Ассистент', icon: 'command', count: 2 }]} value={tab} onChange={setTab} />
            </DsRow>
          </DsSection>

          <DsSection title="Настройки · примитивы" note="Select · QualityDots · SettingRow · OptionCard — те же, что в разделе Настройки">
            <DsRow label="QualityDots"><QualityDots level={1} /><QualityDots level={2} /><QualityDots level={3} /></DsRow>
            <DsRow label="SettingRow">
              <div style={{ flex: 1, minWidth: 0 }}><SettingRow label="Параметр-переключатель" hint="Короткое описание под лейблом." control={<Switch checked={tg} onChange={setTg} />} /></div>
            </DsRow>
            <div style={{ marginTop: 8 }}>
              <div className="u-faint mono" style={{ fontSize: 11.5, marginBottom: 8 }}>OptionCard</div>
              <div style={{ display: 'grid', gap: 8, maxWidth: 520 }}>
                <OptionCard active={optSel === 'local'} icon="cpu" title="Локально" sub="Без сети, приватно." quality={2} meta="приватно" onClick={() => setOptSel('local')} />
                <OptionCard active={optSel === 'cloud'} icon="cloud" title="Облако Wotold" badge="Рекомендуем" sub="Быстрее и точнее." quality={3} meta="высокая точность" onClick={() => setOptSel('cloud')} />
              </div>
            </div>
          </DsSection>

          <DsSection title="Меню" note="Dropdown · кликните">
            <Dropdown width={210} trigger={({ toggle }) => <Btn variant="default" icon="dots" onClick={toggle}>Открыть меню</Btn>}>
              <MenuLabel>Действия</MenuLabel>
              <MenuItem icon="download">Экспортировать…</MenuItem>
              <MenuItem icon="edit">Переименовать</MenuItem>
              <MenuItem icon="refresh" end={<Kbd>⌘R</Kbd>}>Переобработать</MenuItem>
              <MenuSep />
              <MenuItem icon="trash" danger>Удалить</MenuItem>
            </Dropdown>
          </DsSection>

          <DsSection title="Обратная связь">
            <DsRow label="Progress"><div style={{ width: 220 }}><Progress value={63} /></div></DsRow>
            <DsRow label="Skeleton">
              <div style={{ display: 'flex', flexDirection: 'column', gap: 7, width: 240 }}>
                <div className="skeleton" style={{ height: 13, width: '90%', borderRadius: 6 }} />
                <div className="skeleton" style={{ height: 13, width: '70%', borderRadius: 6 }} />
              </div>
            </DsRow>
            <DsRow label="Dot">
              <Dot color="var(--ok)" /><Dot color="var(--accent)" ring /><Dot color="var(--accent)" ring pulse /><Dot color="var(--danger)" pulse />
            </DsRow>
            <DsRow label="Wave"><Wave bars={6} color="var(--danger)" height={20} /></DsRow>
            <DsRow label="Kbd"><Kbd>⌘K</Kbd><Kbd>⌘⇧R</Kbd><Kbd>esc</Kbd></DsRow>
            <DsRow label="Tooltip"><button className="iconbtn tip" data-tip="Подсказка" aria-label="tip"><Icon name="info" size={16} /></button><span className="u-faint" style={{ fontSize: 12 }}>← наведите</span></DsRow>
          </DsSection>

          <DsSection title="Empty state">
            <Panel><Empty icon="inbox" title="Пока пусто" desc="Звонки появятся здесь после первой записи." action={<Btn variant="primary" size="sm" icon="mic">Записать</Btn>} /></Panel>
          </DsSection>

          <DsSection title="Иконки" note={`${iconNames.length} шт · 1.6 stroke · currentColor`}>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(86px, 1fr))', gap: 4 }}>
              {iconNames.map((n) => (
                <div key={n} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 6, padding: '12px 4px', borderRadius: 'var(--r-sm)', border: '1px solid var(--border)' }}>
                  <Icon name={n} size={20} />
                  <span className="u-faint mono" style={{ fontSize: 9.5 }}>{n}</span>
                </div>
              ))}
            </div>
          </DsSection>

        </div>
      </div>
    </>
  );
}

Object.assign(window, { DesignSystemView });
