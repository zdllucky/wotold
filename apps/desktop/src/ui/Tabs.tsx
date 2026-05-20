import { createContext, useContext, useId, type ReactNode } from 'react';

interface TabsContextValue {
  value: string;
  onChange: (value: string) => void;
  /** [B17 a11y] Per-instance id-prefix. Используется для построения стабильных
   *  aria-controls / aria-labelledby пар между Trigger и Panel. */
  idPrefix: string;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabs(): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs.* must be used within <Tabs>');
  return ctx;
}

function triggerId(prefix: string, value: string): string {
  return `${prefix}-tab-${value}`;
}

function panelId(prefix: string, value: string): string {
  return `${prefix}-panel-${value}`;
}

interface TabsProps {
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}

export function Tabs({ value, onChange, children }: TabsProps) {
  // [B17] Wrapper-div держим без класса — handoff (`MIGRATION.md` §4)
  // ставит `.tabs` row + `.tab` items на flat-уровне. Маркируем data-role
  // для отладки, но без визуальных side-effects.
  // [B17 a11y] idPrefix через useId() — стабильный per-instance scope для
  // aria-controls / aria-labelledby пар.
  const idPrefix = useId();
  return (
    <TabsContext.Provider value={{ value, onChange, idPrefix }}>
      <div data-role="tabs">{children}</div>
    </TabsContext.Provider>
  );
}

interface TabsListProps {
  children: ReactNode;
}

function TabsList({ children }: TabsListProps) {
  return (
    <div className="tabs" role="tablist">
      {children}
    </div>
  );
}

interface TabsTriggerProps {
  value: string;
  disabled?: boolean;
  counter?: ReactNode;
  children: ReactNode;
}

function TabsTrigger({ value, disabled, counter, children }: TabsTriggerProps) {
  const ctx = useTabs();
  const active = ctx.value === value;
  return (
    <button
      type="button"
      role="tab"
      id={triggerId(ctx.idPrefix, value)}
      aria-controls={panelId(ctx.idPrefix, value)}
      aria-selected={active}
      tabIndex={active ? 0 : -1}
      data-active={active ? 'true' : 'false'}
      className={`tab${active ? ' tab--active' : ''}`}
      disabled={disabled}
      onClick={() => ctx.onChange(value)}
    >
      {children}
      {counter !== undefined && counter !== null && (
        <span
          className="mono"
          style={{ marginLeft: 6, color: 'var(--muted)', fontSize: 12 }}
        >
          {counter}
        </span>
      )}
    </button>
  );
}

interface TabsPanelProps {
  value: string;
  children: ReactNode;
}

function TabsPanel({ value, children }: TabsPanelProps) {
  const ctx = useTabs();
  if (ctx.value !== value) return null;
  return (
    <div
      role="tabpanel"
      id={panelId(ctx.idPrefix, value)}
      aria-labelledby={triggerId(ctx.idPrefix, value)}
      tabIndex={0}
    >
      {children}
    </div>
  );
}

Tabs.List = TabsList;
Tabs.Trigger = TabsTrigger;
Tabs.Panel = TabsPanel;
