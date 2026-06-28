// Menu item primitive used inside HeaderActions kebab overlay.
// Inline-styled per Atelier v2 — no separate CSS class needed for now,
// hover/disabled handled via onMouseEnter/Leave.

import type { ReactNode } from 'react';

interface MenuItemProps {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  title?: string;
}

export function MenuItem({ children, onClick, disabled, danger, title }: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      disabled={disabled}
      title={title}
      style={{
        display: 'block',
        width: '100%',
        textAlign: 'left',
        padding: '8px 12px',
        border: 'none',
        background: 'transparent',
        color: danger ? 'var(--danger)' : 'var(--text)',
        fontSize: 13.5,
        fontFamily: 'var(--font)',
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.5 : 1,
        borderRadius: 'var(--r-xs)',
      }}
      onMouseEnter={(e) => {
        if (!disabled) e.currentTarget.style.background = 'var(--sunken)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = 'transparent';
      }}
    >
      {children}
    </button>
  );
}
