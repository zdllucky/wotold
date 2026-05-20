import type { HTMLAttributes, ReactNode } from 'react';

interface ToolbarProps extends Omit<HTMLAttributes<HTMLDivElement>, 'title'> {
  title?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
}

export function Toolbar({ title, actions, className, children, ...rest }: ToolbarProps) {
  return (
    <div className={['ds-toolbar', className ?? ''].filter(Boolean).join(' ')} {...rest}>
      {title ? <h2 className="ds-toolbar-title">{title}</h2> : children}
      {actions && <div className="ds-toolbar-actions">{actions}</div>}
    </div>
  );
}
