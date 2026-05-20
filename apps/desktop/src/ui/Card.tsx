// [B17] Atelier v2 thin wrapper — .card / .card--raised / .card--inset.

import type { HTMLAttributes, ReactNode } from 'react';

type Variant = 'default' | 'sunken' | 'raised';

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  variant?: Variant;
  /** Legacy compact prop — sub-tle padding reduce через inline style. */
  compact?: boolean;
  children: ReactNode;
}

export function Card({
  variant = 'default',
  compact = false,
  className,
  children,
  style,
  ...rest
}: CardProps) {
  const classes = [
    'card',
    variant === 'sunken' ? 'card--inset' : '',
    variant === 'raised' ? 'card--raised' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');
  const compactStyle = compact ? { padding: 'var(--space-4)' } : undefined;
  return (
    <div className={classes} style={{ ...compactStyle, ...style }} {...rest}>
      {children}
    </div>
  );
}

interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

function CardHeader({ className, children, style, ...rest }: CardHeaderProps) {
  return (
    <div
      className={className}
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 'var(--space-3)',
        ...style,
      }}
      {...rest}
    >
      {children}
    </div>
  );
}

interface CardTitleProps extends HTMLAttributes<HTMLHeadingElement> {
  children: ReactNode;
}

function CardTitle({ className, children, ...rest }: CardTitleProps) {
  return (
    <h3
      className={['title', className ?? ''].filter(Boolean).join(' ')}
      style={{ fontSize: 18, margin: 0 }}
      {...rest}
    >
      {children}
    </h3>
  );
}

Card.Header = CardHeader;
Card.Title = CardTitle;
