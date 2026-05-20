// [B17] Atelier v2 — простая шапка страницы с .title + опц. .small-caps
// subtitle + actions slot. Старый sticky+backdrop-blur вариант сохранён
// inline для legacy-callers.

import type { CSSProperties, HTMLAttributes, ReactNode } from 'react';

interface ToolbarProps extends Omit<HTMLAttributes<HTMLDivElement>, 'title'> {
  title?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  sticky?: boolean;
  children?: ReactNode;
}

export function Toolbar({
  title,
  subtitle,
  actions,
  sticky = false,
  className,
  children,
  style,
  ...rest
}: ToolbarProps) {
  const stickyStyle: CSSProperties = sticky
    ? {
        position: 'sticky',
        top: 0,
        zIndex: 5,
        background: 'var(--bg)',
        paddingTop: 'var(--space-2)',
        paddingBottom: 'var(--space-3)',
      }
    : {};
  return (
    <div
      className={className}
      style={{
        display: 'flex',
        alignItems: 'flex-end',
        justifyContent: 'space-between',
        gap: 18,
        marginBottom: 24,
        flexWrap: 'wrap',
        ...stickyStyle,
        ...style,
      }}
      {...rest}
    >
      {title ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4, minWidth: 0 }}>
          <h1 className="title" style={{ margin: 0, fontSize: 36 }}>
            {title}
          </h1>
          {subtitle && <span className="small-caps">{subtitle}</span>}
        </div>
      ) : (
        children
      )}
      {actions && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
          {actions}
        </div>
      )}
    </div>
  );
}
