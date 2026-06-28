// [B17] Atelier v2 — pill чуть больше badge. Тот же подход через inline style + token vars.

import type { CSSProperties, HTMLAttributes, ReactNode } from 'react';

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  children: ReactNode;
}

function styleFor(tone: Tone): CSSProperties {
  switch (tone) {
    case 'accent':
      return { background: 'var(--accent-soft)', color: 'var(--accent)' };
    case 'success':
      return { background: 'var(--ok-soft)', color: 'var(--ok)' };
    case 'warning':
      return { background: 'var(--warn-soft)', color: 'var(--warn)' };
    case 'danger':
      return { background: 'var(--danger-soft)', color: 'var(--danger)' };
    case 'neutral':
      return { background: 'var(--sunken)', color: 'var(--text-3)' };
  }
}

export function Pill({ tone = 'neutral', className, children, style, ...rest }: PillProps) {
  return (
    <span
      className={className}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        fontFamily: 'var(--font)',
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: '0.08em',
        textTransform: 'uppercase',
        padding: '3px 10px',
        borderRadius: 'var(--r-pill)',
        lineHeight: 1.4,
        ...styleFor(tone),
        ...style,
      }}
      {...rest}
    >
      {children}
    </span>
  );
}
