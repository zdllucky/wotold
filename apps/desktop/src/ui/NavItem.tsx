// [B18.6c] Wotold v2 uikit — sidebar nav item (.navitem from wk.css).

import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

interface NavItemProps {
  icon?: IconName;
  label: string;
  active?: boolean;
  meta?: string | number;
  onClick?: () => void;
  leading?: ReactNode;
}

export function NavItem({ icon, label, active, meta, onClick, leading }: NavItemProps) {
  return (
    <button
      type="button"
      className="navitem"
      data-active={active ? 'true' : undefined}
      onClick={onClick}
    >
      {leading ?? (icon && <span className="nav-ico"><Icon name={icon} size={16} /></span>)}
      <span className="nav-label">{label}</span>
      {meta != null && <span className="nav-meta">{meta}</span>}
    </button>
  );
}
