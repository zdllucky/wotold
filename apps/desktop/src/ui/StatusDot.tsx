// [B17] .dot + tone через привязку CSS-переменной.

import type { CSSProperties, HTMLAttributes } from 'react';

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
type Size = 'sm' | 'md' | 'lg';

interface StatusDotProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  size?: Size;
  pulse?: boolean;
}

function colorFor(tone: Tone): string {
  switch (tone) {
    case 'accent':
      return 'var(--accent)';
    case 'success':
      return 'var(--ok)';
    case 'warning':
      return 'var(--warn)';
    case 'danger':
      return 'var(--danger)';
    case 'neutral':
      return 'var(--text-faint)';
  }
}

function sizeFor(size: Size): { width: string; height: string } {
  switch (size) {
    case 'sm':
      return { width: '5px', height: '5px' };
    case 'lg':
      return { width: '10px', height: '10px' };
    case 'md':
      return { width: '6px', height: '6px' };
  }
}

export function StatusDot({
  tone = 'neutral',
  size = 'md',
  pulse = false,
  className,
  style,
  ...rest
}: StatusDotProps) {
  const baseStyle: CSSProperties = {
    background: colorFor(tone),
    ...sizeFor(size),
  };
  const classes = ['dot', pulse ? 'dot--pulse' : '', className ?? '']
    .filter(Boolean)
    .join(' ');
  return (
    <span
      aria-hidden="true"
      className={classes}
      style={{ ...baseStyle, ...style }}
      {...rest}
    />
  );
}
