import type { HTMLAttributes, ReactNode } from 'react';

interface ToolbarProps extends Omit<HTMLAttributes<HTMLDivElement>, 'title'> {
  title?: ReactNode;
  /** [B16] Опциональный subtitle под title — для context (например count, дата). */
  subtitle?: ReactNode;
  actions?: ReactNode;
  /** [B16] Sticky positioning поверх scroll (используется на длинных страницах). */
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
  ...rest
}: ToolbarProps) {
  const classes = ['ds-toolbar', sticky && 'ds-toolbar--sticky', className ?? '']
    .filter(Boolean)
    .join(' ');
  return (
    <div className={classes} {...rest}>
      {title ? (
        <div className="ds-toolbar-titles">
          <h2 className="ds-toolbar-title">{title}</h2>
          {subtitle && <p className="ds-toolbar-subtitle">{subtitle}</p>}
        </div>
      ) : (
        children
      )}
      {actions && <div className="ds-toolbar-actions">{actions}</div>}
    </div>
  );
}
