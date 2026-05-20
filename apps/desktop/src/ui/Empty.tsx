import type { ReactNode } from 'react';

interface EmptyProps {
  icon?: ReactNode;
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}

// [B16] Дефолтный fallback icon когда не передан явный — чтобы пустые
// состояния не выглядели текстовой стенкой даже если caller забыл.
const DEFAULT_ICON: ReactNode = '✨';

export function Empty({ icon, title, description, action }: EmptyProps) {
  return (
    <div className="ds-empty">
      <div className="ds-empty-icon">{icon ?? DEFAULT_ICON}</div>
      {title && <p className="ds-empty-title">{title}</p>}
      {description && <p className="ds-empty-description">{description}</p>}
      {action}
    </div>
  );
}
