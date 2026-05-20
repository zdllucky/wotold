// [B17] Atelier v2 thin wrapper — emit .btn + .btn--{variant} + .btn--{size}
// поверх классов из styles/wotold.css. API сохранён 1-в-1 для всех callers.

import type { ButtonHTMLAttributes, ReactNode } from 'react';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'record';
type Size = 'sm' | 'md' | 'lg';

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  variant?: Variant;
  size?: Size;
  /** Legacy — Atelier v2 buttons всегда radius-sm; флаг игнорируется. */
  pill?: boolean;
  block?: boolean;
  busy?: boolean;
  leading?: ReactNode;
  trailing?: ReactNode;
  children: ReactNode;
}

// Старый variant → Atelier btn-class mapping. `record` НЕ используется в
// HomePage (там raw .rec-btn), но если кто-то вызвал — рендерим как danger.
function variantClass(v: Variant): string {
  switch (v) {
    case 'primary':
      return 'btn--primary';
    case 'ghost':
    case 'secondary':
      return 'btn--ghost';
    case 'danger':
    case 'record':
      return 'btn--danger';
  }
}

function sizeClass(s: Size): string {
  switch (s) {
    case 'sm':
      return 'btn--sm';
    case 'lg':
      return 'btn--lg';
    case 'md':
      return '';
  }
}

export function Button({
  variant = 'secondary',
  size = 'md',
  block = false,
  busy = false,
  leading,
  trailing,
  className,
  type = 'button',
  style,
  children,
  pill: _pill,
  ...rest
}: ButtonProps) {
  void _pill;
  const classes = [
    'btn',
    variantClass(variant),
    sizeClass(size),
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');
  const blockStyle = block ? { width: '100%', justifyContent: 'center' as const } : undefined;
  return (
    <button
      type={type}
      className={classes}
      data-busy={busy ? 'true' : undefined}
      style={{ ...blockStyle, ...style }}
      {...rest}
    >
      {leading}
      {children}
      {trailing}
    </button>
  );
}
