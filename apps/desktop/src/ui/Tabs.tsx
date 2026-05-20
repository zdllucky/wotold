import { createContext, useContext, type ReactNode } from 'react';

interface TabsContextValue {
  value: string;
  onChange: (value: string) => void;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabs(): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs.* must be used within <Tabs>');
  return ctx;
}

interface TabsProps {
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}

export function Tabs({ value, onChange, children }: TabsProps) {
  return (
    <TabsContext.Provider value={{ value, onChange }}>
      <div className="ds-tabs">{children}</div>
    </TabsContext.Provider>
  );
}

interface TabsListProps {
  children: ReactNode;
}

function TabsList({ children }: TabsListProps) {
  return (
    <div className="ds-tabs-list" role="tablist">
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
      aria-selected={active}
      data-active={active ? 'true' : 'false'}
      className="ds-tabs-trigger"
      disabled={disabled}
      onClick={() => ctx.onChange(value)}
    >
      {children}
      {counter !== undefined && counter !== null && (
        <span className="ds-tabs-counter">{counter}</span>
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
    <div role="tabpanel" className="ds-tabs-panel">
      {children}
    </div>
  );
}

Tabs.List = TabsList;
Tabs.Trigger = TabsTrigger;
Tabs.Panel = TabsPanel;
