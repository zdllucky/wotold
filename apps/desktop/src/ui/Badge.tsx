// [B17] Тонкий badge — pill с акцентным или семантическим тоном.
// Стиль inline через token vars, чтобы не плодить кастомные классы.

import type { CSSProperties, HTMLAttributes, ReactNode } from 'react';

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
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

export function Badge({ tone = 'neutral', className, children, style, ...rest }: BadgeProps) {
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
        letterSpacing: '0.06em',
        textTransform: 'uppercase',
        padding: '2px 8px',
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
