import { useState } from 'react';

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
  Toolbar,
} from '../ui';

const COLOR_TOKENS = [
  'color-bg',
  'color-surface',
  'color-surface-raised',
  'color-surface-sunken',
  'color-border',
  'color-border-strong',
  'color-text',
  'color-text-muted',
  'color-text-subtle',
  'color-accent',
  'color-accent-hover',
  'color-accent-soft',
  'color-danger',
  'color-danger-soft',
  'color-success',
  'color-success-soft',
  'color-warning',
  'color-warning-soft',
];

const TYPE_TOKENS: Array<{ name: string; sample: string }> = [
  { name: 'text-xs', sample: 'The quick brown fox' },
  { name: 'text-sm', sample: 'The quick brown fox' },
  { name: 'text-base', sample: 'The quick brown fox' },
  { name: 'text-lg', sample: 'The quick brown fox' },
  { name: 'text-xl', sample: 'The quick brown fox' },
  { name: 'text-2xl', sample: 'The quick brown fox' },
  { name: 'text-display', sample: 'Display' },
];

const SPACING_TOKENS = ['space-1', 'space-2', 'space-3', 'space-4', 'space-5', 'space-6', 'space-7', 'space-8'];
const RADIUS_TOKENS = ['radius-sm', 'radius-md', 'radius-lg', 'radius-pill'];
const TONES: Array<'neutral' | 'accent' | 'success' | 'warning' | 'danger'> = [
  'neutral',
  'accent',
  'success',
  'warning',
  'danger',
];

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
    <section className="ds-showcase">
      <Toolbar
        title="Design system"
        actions={<Badge tone="warning">dev only</Badge>}
      />
      <p className="text-muted">
        Эталон для всех экранов. Не хватает компонента или токена — расширяем DS, а не лепим
        inline. Production-сборка скрывает этот таб.
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
    <div className="ds-showcase-section">
      <div>
        <h3>Цвет</h3>
        <div className="ds-swatches">
          {COLOR_TOKENS.map((t) => (
            <div className="ds-swatch" key={t}>
              <div
                className="ds-swatch-chip"
                style={{ background: `var(--${t})` }}
                title={t}
              />
              <span className="ds-swatch-label">--{t}</span>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3>Типографика</h3>
        {TYPE_TOKENS.map((t) => (
          <div className="ds-type-row" key={t.name}>
            <code>--{t.name}</code>
            <span style={{ fontSize: `var(--${t.name})` }}>{t.sample}</span>
          </div>
        ))}
      </div>

      <div>
        <h3>Spacing</h3>
        {SPACING_TOKENS.map((t) => (
          <div className="ds-spacing-row" key={t}>
            <code>--{t}</code>
            <div className="ds-spacing-bar" style={{ width: `var(--${t})` }} />
          </div>
        ))}
      </div>

      <div>
        <h3>Radius</h3>
        <div className="ds-row">
          {RADIUS_TOKENS.map((t) => (
            <div className="ds-swatch" key={t}>
              <div
                className="ds-swatch-chip"
                style={{
                  background: 'var(--color-accent-soft)',
                  borderRadius: `var(--${t})`,
                  width: '4rem',
                  height: '4rem',
                }}
              />
              <span className="ds-swatch-label">--{t}</span>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3>Elevation</h3>
        <div className="ds-row">
          {[1, 2, 3].map((n) => (
            <div className="ds-swatch" key={n}>
              <div
                className="ds-swatch-chip"
                style={{
                  width: '6rem',
                  height: '4rem',
                  background: 'var(--color-surface)',
                  boxShadow: `var(--shadow-${n})`,
                  border: 'none',
                }}
              />
              <span className="ds-swatch-label">--shadow-{n}</span>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3>Motion</h3>
        <p className="text-muted">
          <code>--duration-fast</code> = 120ms · <code>--duration-normal</code> = 220ms ·{' '}
          <code>--ease-out-expo</code>
        </p>
      </div>
    </div>
  );
}

function ComponentsPanel() {
  return (
    <div className="ds-showcase-section">
      <div>
        <h3>Button — варианты</h3>
        <div className="ds-row">
          <Button variant="primary">Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger">Danger</Button>
          <Button variant="record" pill leading={<StatusDot tone="neutral" />}>
            Record
          </Button>
        </div>
      </div>

      <div>
        <h3>Button — размеры</h3>
        <div className="ds-row">
          <Button size="sm">Small</Button>
          <Button size="md">Medium</Button>
          <Button size="lg">Large</Button>
        </div>
      </div>

      <div>
        <h3>Button — состояния</h3>
        <div className="ds-row">
          <Button variant="primary">Default</Button>
          <Button variant="primary" disabled>
            Disabled
          </Button>
          <Button variant="primary" busy>
            Busy
          </Button>
        </div>
      </div>

      <div>
        <h3>Badge / Pill / StatusDot</h3>
        <div className="ds-row">
          {TONES.map((t) => (
            <Badge tone={t} key={`b-${t}`}>
              {t}
            </Badge>
          ))}
        </div>
        <div className="ds-row">
          {TONES.map((t) => (
            <Pill tone={t} key={`p-${t}`}>
              {t}
            </Pill>
          ))}
        </div>
        <div className="ds-row">
          {TONES.map((t) => (
            <span key={`s-${t}`} style={{ display: 'inline-flex', gap: 'var(--space-1)', alignItems: 'center' }}>
              <StatusDot tone={t} />
              {t}
            </span>
          ))}
          <span style={{ display: 'inline-flex', gap: 'var(--space-1)', alignItems: 'center' }}>
            <StatusDot tone="danger" pulse />
            pulse
          </span>
        </div>
      </div>

      <div>
        <h3>Card</h3>
        <div className="ds-grid">
          <Card>
            <Card.Header>
              <Card.Title>Default</Card.Title>
              <Badge tone="neutral">label</Badge>
            </Card.Header>
            <p className="text-muted">Базовая карточка с границей и фоном.</p>
          </Card>
          <Card variant="sunken">
            <Card.Title>Sunken</Card.Title>
            <p className="text-muted">Утопленная — для секций внутри settings.</p>
          </Card>
          <Card variant="raised">
            <Card.Title>Raised</Card.Title>
            <p className="text-muted">Тень — overlay, prompts.</p>
          </Card>
        </div>
      </div>

      <div>
        <h3>Empty state</h3>
        <Card>
          <Empty
            icon="∅"
            title="Ничего нет"
            description="Здесь появится список, когда что-нибудь добавишь."
            action={
              <Button variant="primary" size="sm">
                Добавить
              </Button>
            }
          />
        </Card>
      </div>

      <div>
        <h3>Toolbar</h3>
        <Card>
          <Toolbar
            title="Заголовок"
            actions={
              <>
                <Button size="sm" variant="ghost">
                  Действие
                </Button>
                <Button size="sm" variant="primary">
                  Primary
                </Button>
              </>
            }
          />
        </Card>
      </div>

      <div>
        <h3>Tabs</h3>
        <Card>
          <TabsExample />
        </Card>
      </div>
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
        <p>Контент первой вкладки.</p>
      </Tabs.Panel>
      <Tabs.Panel value="two">
        <p>Вторая.</p>
      </Tabs.Panel>
      <Tabs.Panel value="three">
        <p>Третья.</p>
      </Tabs.Panel>
    </Tabs>
  );
}

function FormsPanel() {
  const [text, setText] = useState('');
  const [select, setSelect] = useState('a');
  const [area, setArea] = useState('');

  return (
    <div className="ds-showcase-section">
      <Card>
        <Card.Title>Field-компоненты</Card.Title>
        <InputField
          label="Текстовое поле"
          hint="С подсказкой под полем."
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Введи что-нибудь"
        />
        <InputField
          label="С ошибкой"
          error="Поле обязательное."
          defaultValue=""
        />
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
      </Card>
    </div>
  );
}
