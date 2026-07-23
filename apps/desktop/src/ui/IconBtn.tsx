// [B18.6c] Wotold v2 uikit — icon button (.iconbtn from wk.css).
// [B27.5] tip рендерится портальным <Tooltip> (CSS .tip::after клипался краями).

import type { MouseEvent } from 'react';

import { Icon, type IconName } from './Icon';
import { Tooltip } from './Tooltip';

interface IconBtnProps {
  icon: IconName;
  label: string;
  size?: 'sm' | 'lg';
  active?: boolean;
  tip?: string;
  tipSide?: 'right';
  iconSize?: number;
  onClick?: (e: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  title?: string;
  /** For dropdown/menu triggers — sets aria-haspopup="menu" + aria-expanded. */
  hasPopup?: boolean;
  expanded?: boolean;
}

export function IconBtn({
  icon,
  label,
  size,
  active,
  tip,
  tipSide,
  iconSize,
  onClick,
  disabled,
  title,
  hasPopup,
  expanded,
}: IconBtnProps) {
  const s = iconSize ?? (size === 'sm' ? 15 : size === 'lg' ? 18 : 16);
  const btn = (
    <button
      type="button"
      className="iconbtn"
      data-size={size}
      data-active={active ? 'true' : undefined}
      aria-label={label}
      aria-haspopup={hasPopup ? 'menu' : undefined}
      aria-expanded={hasPopup ? expanded ?? false : undefined}
      title={title}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon name={icon} size={s} />
    </button>
  );
  if (!tip) return btn;
  return <Tooltip content={tip} side={tipSide === 'right' ? 'right' : 'top'}>{btn}</Tooltip>;
}
