import type { ReactNode } from 'react';

interface EmptyProps {
  icon?: ReactNode;
  title?: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}

// [B17] Пустое состояние — класс `.empty`. Serif из описания ушёл вместе
// с прошлым поколением: шрифт теперь один, Onest.
// Без эмодзи-плейсхолдеров (handoff: «Drop emoji icons; text carries enough»).
// Caller может явно передать `icon` если нужен (modal/page-level empty).
export function Empty({ icon, title, description, action }: EmptyProps) {
  return (
    <div className="empty">
      {icon}
      {title && (
        <p
          style={{
            fontFamily: 'var(--font)',
            fontSize: 19,
            color: 'var(--text)',
            fontStyle: 'normal',
            margin: 0,
          }}
        >
          {title}
        </p>
      )}
      {description && (
        <p
          style={{
            margin: 0,
            maxWidth: '34rem',
            fontStyle: 'italic',
            color: 'var(--text-3)',
          }}
        >
          {description}
        </p>
      )}
      {action}
    </div>
  );
}
