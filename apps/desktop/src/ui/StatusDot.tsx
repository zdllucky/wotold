import type { HTMLAttributes } from 'react';

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
type Size = 'sm' | 'md' | 'lg';

interface StatusDotProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  size?: Size;
  pulse?: boolean;
}

export function StatusDot({
  tone = 'neutral',
  size = 'md',
  pulse = false,
  className,
  ...rest
}: StatusDotProps) {
  const classes = [
    'ds-statusdot',
    `ds-statusdot--${tone}`,
    `ds-statusdot--size-${size}`,
    pulse ? 'ds-statusdot--pulse' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return <span aria-hidden="true" className={classes} {...rest} />;
}
