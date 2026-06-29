// [B18.6c] DesignSystemPage — Wotold v2 "uikit" showroom (dev-only route 'ds').
// Living catalog of tokens + ui primitives so we never hallucinate components.
// Mirrors prototype ~/Downloads/Wotold v2/wk-designsystem.jsx. 0 legacy tokens.

import { useState } from 'react';
import {
  Avatar,
  AvatarGroup,
  Button,
  Chip,
  Dot,
  Empty,
  Icon,
  IconBtn,
  InputField,
  Kbd,
  Dropdown,
  MenuItem,
  MenuLabel,
  MenuSep,
  OptionCard,
  Panel,
  QualityDots,
  Segmented,
  Select,
  SettingRow,
  Skeleton,
  Switch,
  Tabs,
  TextareaField,
  UsageBar,
  Wave,
  type IconName,
} from '../ui';
import { useTheme } from '../theme/useTheme';
import { DsRow, DsSection, Swatch } from './DesignSystemBits';

// — colour tokens shown as swatches (theme-reactive) —
const DS_TOKENS: Array<[string, string]> = [
  ['--bg', 'фон'],
  ['--panel', 'панель'],
  ['--sunken', 'утоплен.'],
  ['--hover', 'ховер'],
  ['--active', 'актив'],
  ['--border', 'границы'],
  ['--border-strong', 'границы+'],
  ['--text', 'текст'],
  ['--text-2', 'текст-2'],
  ['--text-3', 'текст-3'],
  ['--text-faint', 'бледный'],
  ['--accent', 'акцент'],
  ['--accent-soft', 'акц-фон'],
  ['--danger', 'опасн.'],
  ['--ok', 'успех'],
  ['--warn', 'предупр.'],
];

const DS_SPK = ['--sp1', '--sp2', '--sp3', '--sp4', '--sp5'];

const THEME_OPTS = [
  { value: 'light' as const, label: 'Светлая', icon: 'sun' as IconName },
  { value: 'dark' as const, label: 'Тёмная', icon: 'moon' as IconName },
  { value: 'system' as const, label: 'Система', icon: 'cpu' as IconName },
];

const ALL_ICONS: IconName[] = [
  'record', 'stop', 'pause', 'play', 'mic', 'headphones', 'search', 'command',
  'plus', 'settings', 'user', 'users', 'inbox', 'phone', 'doc', 'sparkle',
  'chevronDown', 'chevronRight', 'chevronLeft', 'chevronUpDown', 'check', 'x',
  'dots', 'trash', 'download', 'upload', 'folder', 'refresh', 'filter', 'sort',
  'clock', 'calendar', 'cpu', 'cloud', 'key', 'shield', 'wifiOff', 'alert',
  'info', 'checkCircle', 'arrowRight', 'arrowUp', 'sun', 'moon', 'external',
  'copy', 'edit', 'tag', 'sidebar', 'waveform', 'scissors', 'bolt', 'send',
  'globe', 'link', 'list', 'grid', 'calendarWeek', 'code', 'pip', 'lock',
];

export function DesignSystemPage() {
  const { theme, setTheme } = useTheme();

  const [seg, setSeg] = useState('a');
  const [sw1, setSw1] = useState(true);
  const [sw2, setSw2] = useState(false);
  const [tab, setTab] = useState('one');
  const [selVal, setSelVal] = useState('ru');
  const [optSel, setOptSel] = useState('cloud');
  const [settingToggle, setSettingToggle] = useState(true);

  return (
    // [B18.9-fix] Shared shell: bleed past .app-main 34/44 padding + fill the
    // viewport so the .view-head pins flush and the .scroll body scrolls below
    // — same pattern as Inbox/Contacts/Settings.
    <div className="main" style={{ margin: '-34px -44px', height: '100vh' }}>
      <div className="view-head" data-tauri-drag-region="deep">
        <Icon name="code" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650 }}>Дизайн-система</span>
        <Chip size="sm" tone="accent">
          debug
        </Chip>
        <div style={{ flex: 1 }} />
        <Segmented
          value={theme}
          onChange={setTheme}
          options={THEME_OPTS}
          ariaLabel="Тема оформления"
        />
      </div>

      <div className="scroll" style={{ flex: 1, minHeight: 0 }}>
        <div style={{ maxWidth: 880, margin: '0 auto', padding: '28px 32px 80px' }}>
          {/* 1 · Colours */}
          <DsSection title="Цвета" note="токены темы — меняются вместе со светлой и тёмной">
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
              {DS_TOKENS.map(([v, l]) => (
                <Swatch key={v} varName={v} label={l} />
              ))}
            </div>
            <div
              className="mono"
              style={{ fontSize: 'var(--t-11)', color: 'var(--text-faint)', marginBottom: 8 }}
            >
              палитра говорящих (--sp1..5)
            </div>
            <div style={{ display: 'flex', gap: 10 }}>
              {DS_SPK.map((v) => (
                <div
                  key={v}
                  style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5 }}
                >
                  <div
                    style={{
                      width: 44,
                      height: 44,
                      borderRadius: 'var(--r-sm)',
                      background: `var(${v})`,
                    }}
                  />
                  <span
                    className="mono"
                    style={{ fontSize: 9, color: 'var(--text-faint)' }}
                  >
                    {v}
                  </span>
                </div>
              ))}
            </div>
          </DsSection>

          {/* 2 · Typography */}
          <DsSection title="Типографика" note="Hanken Grotesk · IBM Plex Mono">
            <div
              style={{
                fontSize: 'var(--t-28)',
                fontWeight: 700,
                letterSpacing: '-.02em',
                lineHeight: 1.1,
                color: 'var(--text)',
              }}
            >
              Заголовок дисплей · 28/700
            </div>
            <div
              style={{
                fontSize: 'var(--t-22)',
                fontWeight: 700,
                letterSpacing: '-.01em',
                marginTop: 12,
                color: 'var(--text)',
              }}
            >
              Заголовок раздела · 22/700
            </div>
            <div
              style={{ fontSize: 'var(--t-18)', fontWeight: 600, marginTop: 12, color: 'var(--text)' }}
            >
              Подзаголовок · 18/600
            </div>
            <div
              style={{
                fontSize: 'var(--t-13)',
                fontWeight: 400,
                color: 'var(--text-2)',
                marginTop: 12,
                maxWidth: 540,
                lineHeight: 1.5,
              }}
            >
              Основной текст · 13/400. Нейтральный деловой тон, без разговорности.
              Расшифровка, разделение голосов и рекап формируются автоматически.
            </div>
            <div
              className="mono"
              style={{ fontSize: 'var(--t-13)', marginTop: 12, color: 'var(--text-3)' }}
            >
              mono · 0:42 · 38/60 мин · sk-•••
            </div>
          </DsSection>

          {/* 3 · Buttons */}
          <DsSection title="Кнопки">
            <DsRow label="variant">
              <Button variant="primary">Primary</Button>
              <Button variant="secondary">Default</Button>
              <Button variant="ghost">Ghost</Button>
              <Button variant="soft">Soft</Button>
              <Button variant="danger">Danger</Button>
              <Button variant="ghost" style={{ color: 'var(--danger)' }}>
                Danger ghost
              </Button>
            </DsRow>
            <DsRow label="с иконкой">
              <Button variant="primary" leading={<Icon name="mic" size={15} />}>
                Записать
              </Button>
              <Button variant="secondary" leading={<Icon name="download" size={15} />}>
                Экспорт
              </Button>
              <Button variant="soft" leading={<Icon name="check" size={15} />}>
                Подтвердить
              </Button>
            </DsRow>
            <DsRow label="size">
              <Button variant="secondary" size="sm">
                Small
              </Button>
              <Button variant="secondary">Medium</Button>
              <Button variant="secondary" size="lg">
                Large
              </Button>
            </DsRow>
            <DsRow label="state">
              <Button variant="primary" disabled>
                Disabled
              </Button>
              <Button variant="primary" block style={{ maxWidth: 180 }}>
                Block
              </Button>
            </DsRow>
            <DsRow label="icon-кнопки">
              <IconBtn icon="dots" label="меню" />
              <IconBtn icon="trash" label="удалить" />
              <IconBtn icon="settings" active label="актив" />
              <IconBtn icon="refresh" size="sm" label="sm" />
              <IconBtn icon="folder" size="lg" label="lg" />
            </DsRow>
            <DsRow label="tooltip">
              <IconBtn icon="sparkle" label="подсказка" tip="Подсказка снизу" />
              <IconBtn icon="bolt" label="подсказка справа" tip="Подсказка справа" tipSide="right" />
            </DsRow>
          </DsSection>

          {/* 4 · Chips */}
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
              <Chip icon="cpu" tone="ok">
                На устройстве
              </Chip>
              <Chip icon="cloud" tone="accent">
                Облако
              </Chip>
              <Chip icon="clock">0:42</Chip>
              <Chip icon="tag" tone="line">
                партнёр
              </Chip>
            </DsRow>
            <DsRow label="size sm">
              <Chip size="sm" tone="accent">
                обработка
              </Chip>
              <Chip size="sm" tone="danger">
                ошибка
              </Chip>
              <Chip size="sm" tone="line">
                5/5
              </Chip>
            </DsRow>
          </DsSection>

          {/* 5 · Avatars */}
          <DsSection title="Аватары">
            <DsRow label="speaker">
              <Avatar name="Вы" color="var(--sp1)" size={28} />
              <Avatar name="Арман Сулейменов" color="var(--sp2)" size={28} />
              <Avatar name="Елена Ковач" color="var(--sp3)" size={28} />
              <Avatar name="?" color="var(--text-faint)" size={28} />
            </DsRow>
            <DsRow label="size">
              <Avatar name="АА" color="var(--sp4)" size={20} />
              <Avatar name="АА" color="var(--sp4)" size={28} />
              <Avatar name="АА" color="var(--sp4)" size={44} />
            </DsRow>
            <DsRow label="group">
              <AvatarGroup
                items={[
                  { name: 'Вы', color: 'var(--sp1)' },
                  { name: 'Арман', color: 'var(--sp2)' },
                  { name: 'Елена', color: 'var(--sp3)' },
                  { name: 'Дмитрий', color: 'var(--sp4)' },
                  { name: 'Гость', color: 'var(--sp5)' },
                ]}
                max={4}
                size={24}
              />
            </DsRow>
          </DsSection>

          {/* 6 · Inputs */}
          <DsSection title="Поля ввода">
            <div style={{ display: 'grid', gap: 14, maxWidth: 360 }}>
              <InputField label="Текстовое поле" hint="подсказка под полем" placeholder="Поиск…" />
              <InputField label="Без иконки" placeholder="Имя контакта" />
              <TextareaField label="Многострочное" hint="заметка о звонке" placeholder="Заметка…" rows={3} />
            </div>
          </DsSection>

          {/* 7 · Controls */}
          <DsSection title="Контролы">
            <DsRow label="Segmented">
              <Segmented
                value={seg}
                onChange={setSeg}
                ariaLabel="Вид"
                options={[
                  { value: 'a', label: 'Список', icon: 'list' },
                  { value: 'b', label: 'Карточки', icon: 'grid' },
                  { value: 'c', label: 'Календарь', icon: 'calendar' },
                ]}
              />
            </DsRow>
            <DsRow label="Select">
              <Select
                value={selVal}
                onChange={setSelVal}
                width={240}
                ariaLabel="Язык"
                options={[
                  { value: 'ru', label: 'Русский' },
                  { value: 'en', label: 'English' },
                  { value: 'kk', label: 'Қазақша' },
                ]}
              />
            </DsRow>
            <DsRow label="Switch">
              <Switch checked={sw1} onChange={setSw1} label="вкл" />
              <span style={{ fontSize: 'var(--t-12)', color: 'var(--text-faint)' }}>вкл</span>
              <Switch checked={sw2} onChange={setSw2} label="выкл" />
              <span style={{ fontSize: 'var(--t-12)', color: 'var(--text-faint)' }}>выкл</span>
            </DsRow>
            <DsRow label="Tabs">
              <Tabs value={tab} onChange={setTab}>
                <Tabs.List>
                  <Tabs.Trigger value="one">Транскрипт</Tabs.Trigger>
                  <Tabs.Trigger value="two">Рекап</Tabs.Trigger>
                  <Tabs.Trigger value="three" counter={2}>
                    Ассистент
                  </Tabs.Trigger>
                </Tabs.List>
              </Tabs>
            </DsRow>
          </DsSection>

          {/* 8 · Settings primitives */}
          <DsSection
            title="Настройки · примитивы"
            note="QualityDots · SettingRow · OptionCard"
          >
            <DsRow label="QualityDots">
              <QualityDots level={1} />
              <QualityDots level={2} />
              <QualityDots level={3} />
            </DsRow>
            <DsRow label="SettingRow">
              <div style={{ flex: 1, minWidth: 0 }}>
                <SettingRow
                  label="Параметр-переключатель"
                  hint="Короткое описание под лейблом."
                  control={<Switch checked={settingToggle} onChange={setSettingToggle} label="вкл" />}
                />
              </div>
            </DsRow>
            <div style={{ marginTop: 8 }}>
              <div
                className="mono"
                style={{ fontSize: 'var(--t-11)', color: 'var(--text-faint)', marginBottom: 8 }}
              >
                OptionCard
              </div>
              <div style={{ display: 'grid', gap: 8, maxWidth: 520 }}>
                <OptionCard
                  active={optSel === 'local'}
                  icon="cpu"
                  title="Локально"
                  sub="Без сети, приватно."
                  quality={2}
                  meta="приватно"
                  onClick={() => setOptSel('local')}
                />
                <OptionCard
                  active={optSel === 'cloud'}
                  icon="cloud"
                  title="Облако Wotold"
                  badge="Рекомендуем"
                  sub="Быстрее и точнее."
                  quality={3}
                  meta="высокая точность"
                  onClick={() => setOptSel('cloud')}
                />
              </div>
            </div>
          </DsSection>

          {/* 9 · Menu */}
          <DsSection title="Меню" note="Dropdown · кликните">
            <Dropdown
              width={210}
              trigger={({ toggle }) => (
                <Button variant="secondary" leading={<Icon name="dots" size={15} />} onClick={toggle}>
                  Открыть меню
                </Button>
              )}
            >
              <MenuLabel>Действия</MenuLabel>
              <MenuItem icon="download" end={<Kbd>⌘E</Kbd>}>
                Экспортировать…
              </MenuItem>
              <MenuItem icon="edit">Переименовать</MenuItem>
              <MenuItem icon="refresh" end={<Kbd>⌘R</Kbd>}>
                Переобработать
              </MenuItem>
              <MenuSep />
              <MenuItem icon="trash" danger>
                Удалить
              </MenuItem>
            </Dropdown>
          </DsSection>

          {/* 10 · Feedback */}
          <DsSection title="Обратная связь">
            <DsRow label="Progress">
              <div style={{ width: 240 }}>
                <UsageBar label="Использовано" used={63} limit={100} format={(v) => `${v}%`} />
              </div>
            </DsRow>
            <DsRow label="Skeleton">
              <div style={{ display: 'flex', flexDirection: 'column', gap: 7, width: 240 }}>
                <Skeleton width="90%" height="13px" />
                <Skeleton width="70%" height="13px" />
              </div>
            </DsRow>
            <DsRow label="Dot">
              <Dot color="var(--ok)" />
              <Dot color="var(--accent)" ring />
              <Dot color="var(--accent)" ring pulse />
              <Dot color="var(--danger)" pulse />
            </DsRow>
            <DsRow label="Wave">
              <Wave bars={6} color="var(--danger)" />
            </DsRow>
            <DsRow label="Kbd">
              <Kbd>⌘K</Kbd>
              <Kbd>⌘⇧R</Kbd>
              <Kbd>esc</Kbd>
            </DsRow>
          </DsSection>

          {/* 11 · Empty */}
          <DsSection title="Empty state">
            <Panel>
              <Empty
                icon={<Icon name="inbox" size={28} style={{ color: 'var(--text-faint)' }} />}
                title="Пока пусто"
                description="Звонки появятся здесь после первой записи."
                action={
                  <Button variant="primary" size="sm" leading={<Icon name="mic" size={14} />}>
                    Записать
                  </Button>
                }
              />
            </Panel>
          </DsSection>

          {/* 12 · Icons */}
          <DsSection title="Иконки" note={`${ALL_ICONS.length} шт · 1.6 stroke · currentColor`}>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(86px, 1fr))',
                gap: 4,
              }}
            >
              {ALL_ICONS.map((n) => (
                <div
                  key={n}
                  style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    gap: 6,
                    padding: '12px 4px',
                    borderRadius: 'var(--r-sm)',
                    border: '1px solid var(--border)',
                    color: 'var(--text-2)',
                  }}
                >
                  <Icon name={n} size={20} />
                  <span
                    className="mono"
                    style={{ fontSize: 9.5, color: 'var(--text-faint)' }}
                  >
                    {n}
                  </span>
                </div>
              ))}
            </div>
          </DsSection>
        </div>
      </div>
    </div>
  );
}
