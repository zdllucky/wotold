// Wotold v2 (uikit) thin wrapper — emit .btn + .btn--{variant} + .btn--{size}
// поверх классов из styles/wk.css. API сохранён 1-в-1 для всех callers.

import type { ButtonHTMLAttributes, ReactNode } from 'react';

type Variant = 'primary' | 'secondary' | 'default' | 'ghost' | 'soft' | 'danger' | 'record';
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

// variant → uikit btn-class mapping. `record` рендерится как danger.
function variantClass(v: Variant): string {
  switch (v) {
    case 'primary':
      return 'btn--primary';
    case 'default':
      return 'btn--default';
    case 'ghost':
    case 'secondary':
      return 'btn--ghost';
    case 'soft':
      return 'btn--soft';
    case 'danger':
    case 'record':
      return 'btn--danger';
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
  const classes = ['btn', variantClass(variant), className ?? '']
    .filter(Boolean)
    .join(' ');
  const blockStyle = block ? { width: '100%', justifyContent: 'center' as const } : undefined;
  return (
    <button
      type={type}
      className={classes}
      // wk.css sizes via [data-size]; 'md' is the default (no attr).
      data-size={size === 'md' ? undefined : size}
      data-busy={busy ? 'true' : undefined}
      data-block={block ? 'true' : undefined}
      style={{ ...blockStyle, ...style }}
      {...rest}
    >
      {leading}
      {children}
      {trailing}
    </button>
  );
}
