import type { HTMLAttributes, ReactNode } from 'react';

type Variant = 'default' | 'sunken' | 'raised';

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  variant?: Variant;
  compact?: boolean;
  children: ReactNode;
}

export function Card({
  variant = 'default',
  compact = false,
  className,
  children,
  ...rest
}: CardProps) {
  const classes = [
    'ds-card',
    variant === 'sunken' ? 'ds-card--sunken' : '',
    variant === 'raised' ? 'ds-card--raised' : '',
    compact ? 'ds-card--compact' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');
  return (
    <div className={classes} {...rest}>
      {children}
    </div>
  );
}

interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

function CardHeader({ className, children, ...rest }: CardHeaderProps) {
  return (
    <div className={['ds-card-header', className ?? ''].filter(Boolean).join(' ')} {...rest}>
      {children}
    </div>
  );
}

interface CardTitleProps extends HTMLAttributes<HTMLHeadingElement> {
  children: ReactNode;
}

function CardTitle({ className, children, ...rest }: CardTitleProps) {
  return (
    <h3 className={['ds-card-title', className ?? ''].filter(Boolean).join(' ')} {...rest}>
      {children}
    </h3>
  );
}

Card.Header = CardHeader;
Card.Title = CardTitle;
