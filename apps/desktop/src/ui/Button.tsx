import type { ButtonHTMLAttributes, ReactNode } from 'react';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'record';
type Size = 'sm' | 'md' | 'lg';

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  variant?: Variant;
  size?: Size;
  pill?: boolean;
  block?: boolean;
  busy?: boolean;
  leading?: ReactNode;
  trailing?: ReactNode;
  children: ReactNode;
}

export function Button({
  variant = 'secondary',
  size = 'md',
  pill = false,
  block = false,
  busy = false,
  leading,
  trailing,
  className,
  type = 'button',
  children,
  ...rest
}: ButtonProps) {
  const classes = [
    'ds-button',
    `ds-button--variant-${variant}`,
    `ds-button--size-${size}`,
    pill ? 'ds-button--pill' : '',
    block ? 'ds-button--block' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <button type={type} className={classes} data-busy={busy ? 'true' : undefined} {...rest}>
      {leading}
      {children}
      {trailing}
    </button>
  );
}
