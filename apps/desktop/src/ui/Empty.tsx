import type { ReactNode } from 'react';

interface EmptyProps {
  icon?: ReactNode;
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}

export function Empty({ icon, title, description, action }: EmptyProps) {
  return (
    <div className="ds-empty">
      {icon && <div className="ds-empty-icon">{icon}</div>}
      {title && <p className="ds-empty-title">{title}</p>}
      {description && <p className="ds-empty-description">{description}</p>}
      {action}
    </div>
  );
}
