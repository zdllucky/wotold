// [B18.6c] Wotold v2 uikit — chip / tag (.chip from wk.css).

import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

type ChipTone = 'neutral' | 'accent' | 'ok' | 'danger' | 'warn' | 'line';

interface ChipProps {
  tone?: ChipTone;
  icon?: IconName;
  size?: 'sm';
  children: ReactNode;
  onClick?: () => void;
  title?: string;
}

export function Chip({ tone = 'neutral', icon, size, children, onClick, title }: ChipProps) {
  const Tag = onClick ? 'button' : 'span';
  const className = 'chip' + (tone && tone !== 'neutral' ? ' chip--' + tone : '');
  const iconSize = size === 'sm' ? 11 : 12;
  return (
    <Tag
      className={className}
      data-size={size}
      title={title}
      onClick={onClick}
      {...(onClick ? { type: 'button' as const } : {})}
    >
      {icon && <Icon name={icon} size={iconSize} />}
      {children}
    </Tag>
  );
}
