// [B17] Atelier v2 — dev-only showcase page для всех токенов и компонентов.
// Inline-styles вместо отдельных DS-классов: каждая секция использует
// только токены из styles/tokens.css.

import { useState, type CSSProperties } from 'react';

import {
  Badge,
  Button,
  Card,
  Empty,
  InputField,
  Pill,
  SelectField,
  StatusDot,
  Tabs,
  TextareaField,
  UsageBar,
} from '../ui';

const COLOR_TOKENS = [
  'bg',
  'bg-2',
  'paper',
  'surface',
  'surface-raised',
  'line',
  'line-soft',
  'line-strong',
  'ink',
  'ink-2',
  'muted',
  'subtle',
  'accent',
  'accent-hover',
  'accent-soft',
  'signal',
  'signal-soft',
  'success',
  'success-soft',
  'warning',
  'warning-soft',
  'sp-1',
  'sp-2',
  'sp-3',
  'sp-4',
  'sp-5',
];

const TYPE_TOKENS: Array<{ name: string; sample: string }> = [
  { name: 'text-xs', sample: 'The quick brown fox' },
  { name: 'text-sm', sample: 'The quick brown fox' },
  { name: 'text-base', sample: 'The quick brown fox' },
  { name: 'text-lg', sample: 'The quick brown fox' },
  { name: 'text-xl', sample: 'The quick brown fox' },
  { name: 'text-2xl', sample: 'The quick brown fox' },
  { name: 'text-display', sample: 'Display' },
  { name: 'text-hero', sample: 'Hero' },
];

const SPACING_TOKENS = [
  'space-1',
  'space-2',
  'space-3',
  'space-4',
  'space-5',
  'space-6',
  'space-7',
  'space-8',
  'space-9',
];
const RADIUS_TOKENS = ['radius-sm', 'radius-md', 'radius-lg', 'radius-xl', 'radius-pill'];
const TONES: Array<'neutral' | 'accent' | 'success' | 'warning' | 'danger'> = [
  'neutral',
  'accent',
  'success',
  'warning',
  'danger',
];

const SECTION_HEADING: CSSProperties = {
  fontFamily: 'var(--font-serif)',
  fontSize: 22,
  fontWeight: 500,
  marginBottom: 14,
  marginTop: 32,
  color: 'var(--ink)',
};
const ROW: CSSProperties = {
  display: 'flex',
  gap: 12,
  flexWrap: 'wrap',
  alignItems: 'center',
};
const GRID: CSSProperties = {
  display: 'grid',
  gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))',
  gap: 14,
};
const SWATCH_LABEL: CSSProperties = {
  fontFamily: 'var(--font-mono)',
  fontSize: 11,
  color: 'var(--muted)',
  marginTop: 6,
  display: 'block',
};

function initialDsTab(): 'tokens' | 'components' | 'forms' {
  if (typeof window === 'undefined') return 'tokens';
  const params = new URLSearchParams(window.location.search);
  const t = params.get('dstab');
  if (t === 'components' || t === 'forms') return t;
  return 'tokens';
}

export function DesignSystemPage() {
  const [tab, setTab] = useState<'tokens' | 'components' | 'forms'>(initialDsTab);

  return (
    <section>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: 16,
          marginBottom: 28,
          flexWrap: 'wrap',
        }}
      >
        <h1 className="title" style={{ fontSize: 36, margin: 0 }}>
          Design system
        </h1>
        <Badge tone="warning">dev only</Badge>
      </div>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 15,
          maxWidth: 540,
          marginBottom: 18,
        }}
      >
        Эталон для всех экранов. Не хватает компонента или токена — расширяем DS,
        а не лепим inline. Production-сборка скрывает этот таб.
      </p>

      <Tabs value={tab} onChange={(v) => setTab(v as typeof tab)}>
        <Tabs.List>
          <Tabs.Trigger value="tokens">Токены</Tabs.Trigger>
          <Tabs.Trigger value="components">Компоненты</Tabs.Trigger>
          <Tabs.Trigger value="forms">Формы</Tabs.Trigger>
        </Tabs.List>

        <Tabs.Panel value="tokens">
          <TokensPanel />
        </Tabs.Panel>
        <Tabs.Panel value="components">
          <ComponentsPanel />
        </Tabs.Panel>
        <Tabs.Panel value="forms">
          <FormsPanel />
        </Tabs.Panel>
      </Tabs>
    </section>
  );
}

function TokensPanel() {
  return (
    <div>
      <h3 style={SECTION_HEADING}>Цвет</h3>
      <div style={GRID}>
        {COLOR_TOKENS.map((t) => (
          <div
            key={t}
            style={{ display: 'flex', flexDirection: 'column' }}
          >
            <div
              style={{
                width: '100%',
                height: 48,
                borderRadius: 'var(--radius-md)',
                background: `var(--${t})`,
                border: '1px solid var(--line)',
              }}
              title={t}
            />
            <span style={SWATCH_LABEL}>--{t}</span>
          </div>
        ))}
      </div>

      <h3 style={SECTION_HEADING}>Типографика</h3>
      {TYPE_TOKENS.map((t) => (
        <div
          key={t.name}
          style={{
            display: 'grid',
            gridTemplateColumns: '140px 1fr',
            gap: 16,
            padding: '8px 0',
            borderBottom: '1px solid var(--line-soft)',
            alignItems: 'baseline',
          }}
        >
          <code
            className="mono"
            style={{ fontSize: 11, color: 'var(--muted)' }}
          >
            --{t.name}
          </code>
          <span
            style={{
              fontSize: `var(--${t.name})`,
              fontFamily: 'var(--font-serif)',
              color: 'var(--ink)',
            }}
          >
            {t.sample}
          </span>
        </div>
      ))}

      <h3 style={SECTION_HEADING}>Spacing</h3>
      {SPACING_TOKENS.map((t) => (
        <div
          key={t}
          style={{
            display: 'grid',
            gridTemplateColumns: '140px 1fr',
            gap: 16,
            padding: '6px 0',
            alignItems: 'center',
          }}
        >
          <code
            className="mono"
            style={{ fontSize: 11, color: 'var(--muted)' }}
          >
            --{t}
          </code>
          <div
            style={{
              height: 16,
              background: 'var(--accent-soft)',
              width: `var(--${t})`,
              borderRadius: 'var(--radius-sm)',
            }}
          />
        </div>
      ))}

      <h3 style={SECTION_HEADING}>Radius</h3>
      <div style={ROW}>
        {RADIUS_TOKENS.map((t) => (
          <div
            key={t}
            style={{ display: 'flex', flexDirection: 'column', alignItems: 'center' }}
          >
            <div
              style={{
                width: 64,
                height: 64,
                background: 'var(--accent-soft)',
                borderRadius: `var(--${t})`,
                border: '1px solid var(--line)',
              }}
            />
            <span style={SWATCH_LABEL}>--{t}</span>
          </div>
        ))}
      </div>

      <h3 style={SECTION_HEADING}>Elevation</h3>
      <div style={ROW}>
        {[1, 2, 3].map((n) => (
          <div
            key={n}
            style={{ display: 'flex', flexDirection: 'column', alignItems: 'center' }}
          >
            <div
              style={{
                width: 96,
                height: 64,
                background: 'var(--surface)',
                boxShadow: `var(--shadow-${n})`,
                borderRadius: 'var(--radius-md)',
              }}
            />
            <span style={SWATCH_LABEL}>--shadow-{n}</span>
          </div>
        ))}
      </div>

      <h3 style={SECTION_HEADING}>Motion</h3>
      <p className="muted" style={{ fontFamily: 'var(--font-serif)', fontSize: 15 }}>
        <code className="mono">--duration-fast</code> = 120ms ·{' '}
        <code className="mono">--duration-normal</code> = 220ms ·{' '}
        <code className="mono">--duration-slow</code> = 360ms ·{' '}
        <code className="mono">--ease-out-expo</code> /{' '}
        <code className="mono">--ease-out-quart</code>
      </p>
    </div>
  );
}

function ComponentsPanel() {
  return (
    <div>
      <h3 style={SECTION_HEADING}>Button — варианты</h3>
      <div style={ROW}>
        <Button variant="primary">Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="ghost">Ghost</Button>
        <Button variant="danger">Danger</Button>
        <button className="rec-btn rec-btn--sm" aria-label="Record" />
      </div>

      <h3 style={SECTION_HEADING}>Button — размеры</h3>
      <div style={ROW}>
        <Button size="sm">Small</Button>
        <Button size="md">Medium</Button>
        <Button size="lg">Large</Button>
      </div>

      <h3 style={SECTION_HEADING}>Button — состояния</h3>
      <div style={ROW}>
        <Button variant="primary">Default</Button>
        <Button variant="primary" disabled>
          Disabled
        </Button>
        <Button variant="primary" busy>
          Busy
        </Button>
      </div>

      <h3 style={SECTION_HEADING}>Badge / Pill / StatusDot</h3>
      <div style={ROW}>
        {TONES.map((t) => (
          <Badge tone={t} key={`b-${t}`}>
            {t}
          </Badge>
        ))}
      </div>
      <div style={{ ...ROW, marginTop: 10 }}>
        {TONES.map((t) => (
          <Pill tone={t} key={`p-${t}`}>
            {t}
          </Pill>
        ))}
      </div>
      <div style={{ ...ROW, marginTop: 10 }}>
        {TONES.map((t) => (
          <span
            key={`s-${t}`}
            style={{ display: 'inline-flex', gap: 6, alignItems: 'center' }}
          >
            <StatusDot tone={t} />
            <span style={{ fontSize: 13 }}>{t}</span>
          </span>
        ))}
        <span style={{ display: 'inline-flex', gap: 6, alignItems: 'center' }}>
          <StatusDot tone="danger" pulse />
          <span style={{ fontSize: 13 }}>pulse</span>
        </span>
      </div>

      <h3 style={SECTION_HEADING}>Speaker chips</h3>
      <div style={ROW}>
        {[1, 2, 3, 4, 5].map((i) => (
          <span className="sp" key={i}>
            <span
              className="sp-avatar"
              style={{ background: `var(--sp-${i})` }}
            >
              S{i}
            </span>
            Спикер {i}
          </span>
        ))}
      </div>

      <h3 style={SECTION_HEADING}>Card</h3>
      <div style={GRID}>
        <Card>
          <Card.Header>
            <Card.Title>Default</Card.Title>
            <Badge tone="neutral">label</Badge>
          </Card.Header>
          <p className="muted" style={{ margin: '8px 0 0', fontSize: 14 }}>
            Базовая карточка с границей и фоном.
          </p>
        </Card>
        <Card variant="sunken">
          <Card.Title>Sunken</Card.Title>
          <p className="muted" style={{ margin: '8px 0 0', fontSize: 14 }}>
            Утопленная — для секций внутри settings.
          </p>
        </Card>
        <Card variant="raised">
          <Card.Title>Raised</Card.Title>
          <p className="muted" style={{ margin: '8px 0 0', fontSize: 14 }}>
            Тень — overlay, prompts.
          </p>
        </Card>
      </div>

      <h3 style={SECTION_HEADING}>Confidence bar</h3>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10, maxWidth: 320 }}>
        {[0.3, 0.6, 0.85, 0.99].map((p) => (
          <div key={p}>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                marginBottom: 4,
              }}
            >
              <span className="small-caps">Уверенность</span>
              <span className="mono" style={{ fontSize: 12 }}>
                {Math.round(p * 100)}%
              </span>
            </div>
            <div className="conf">
              <div className="conf-fill" style={{ width: `${p * 100}%` }} />
            </div>
          </div>
        ))}
      </div>

      <h3 style={SECTION_HEADING}>Empty state</h3>
      <Card>
        <Empty
          title="Ничего нет"
          description="Здесь появится список, когда что-нибудь добавишь."
          action={
            <Button variant="primary" size="sm">
              Добавить
            </Button>
          }
        />
      </Card>

      <h3 style={SECTION_HEADING}>Tabs</h3>
      <Card>
        <TabsExample />
      </Card>

      <h3 style={SECTION_HEADING}>UsageBar</h3>
      <Card>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <UsageBar label="STT секунды (ok)" used={500} limit={3600} />
          <UsageBar label="STT секунды (warning)" used={2800} limit={3600} />
          <UsageBar label="STT секунды (danger)" used={3500} limit={3600} />
          <UsageBar label="LLM (∞ лимит)" used={120} limit={0} />
          <UsageBar
            label="custom format"
            used={500}
            limit={3600}
            format={(v) => `${v}s`}
          />
        </div>
      </Card>
    </div>
  );
}

function TabsExample() {
  const [t, setT] = useState('one');
  return (
    <Tabs value={t} onChange={setT}>
      <Tabs.List>
        <Tabs.Trigger value="one">Один</Tabs.Trigger>
        <Tabs.Trigger value="two" counter="3">
          Два
        </Tabs.Trigger>
        <Tabs.Trigger value="three" counter="∅">
          Три
        </Tabs.Trigger>
        <Tabs.Trigger value="four" disabled>
          Disabled
        </Tabs.Trigger>
      </Tabs.List>
      <Tabs.Panel value="one">
        <p style={{ fontFamily: 'var(--font-serif)' }}>Контент первой вкладки.</p>
      </Tabs.Panel>
      <Tabs.Panel value="two">
        <p style={{ fontFamily: 'var(--font-serif)' }}>Вторая.</p>
      </Tabs.Panel>
      <Tabs.Panel value="three">
        <p style={{ fontFamily: 'var(--font-serif)' }}>Третья.</p>
      </Tabs.Panel>
    </Tabs>
  );
}

function FormsPanel() {
  const [text, setText] = useState('');
  const [select, setSelect] = useState('a');
  const [area, setArea] = useState('');

  return (
    <div>
      <Card>
        <Card.Title>Field-компоненты</Card.Title>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <InputField
            label="Текстовое поле"
            hint="С подсказкой под полем."
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Введи что-нибудь"
          />
          <InputField label="С ошибкой" error="Поле обязательное." defaultValue="" />
          <InputField label="Disabled" disabled defaultValue="нельзя" />
          <SelectField
            label="Select"
            value={select}
            onChange={(e) => setSelect(e.target.value)}
          >
            <option value="a">Вариант A</option>
            <option value="b">Вариант B</option>
            <option value="c">Вариант C</option>
          </SelectField>
          <TextareaField
            label="Textarea"
            value={area}
            onChange={(e) => setArea(e.target.value)}
            rows={3}
            placeholder="Многострочный ввод"
          />
        </div>
      </Card>
    </div>
  );
}
