import type { HTMLAttributes, ReactNode } from 'react';

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  children: ReactNode;
}

export function Pill({ tone = 'neutral', className, children, ...rest }: PillProps) {
  const classes = ['ds-pill', `ds-pill--${tone}`, className ?? ''].filter(Boolean).join(' ');
  return (
    <span className={classes} {...rest}>
      {children}
    </span>
  );
}
