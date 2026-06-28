// [B18.6c] Wotold v2 uikit — sidebar nav item (.navitem from wk.css).

import type { CSSProperties, ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

interface NavItemProps {
  icon?: IconName;
  label: string;
  active?: boolean;
  /** Sets aria-current="page" — use for the selected nav/route item. */
  current?: boolean;
  meta?: ReactNode;
  onClick?: () => void;
  leading?: ReactNode;
  title?: string;
  style?: CSSProperties;
  className?: string;
}

export function NavItem({
  icon,
  label,
  active,
  current,
  meta,
  onClick,
  leading,
  title,
  style,
  className,
}: NavItemProps) {
  return (
    <button
      type="button"
      className={className ? `navitem ${className}` : 'navitem'}
      data-active={active ? 'true' : undefined}
      aria-current={current ? 'page' : undefined}
      title={title}
      style={style}
      onClick={onClick}
    >
      {leading ?? (icon && <span className="nav-ico"><Icon name={icon} size={16} /></span>)}
      <span className="nav-label">{label}</span>
      {meta != null && <span className="nav-meta">{meta}</span>}
    </button>
  );
}
