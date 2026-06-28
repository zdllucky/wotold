// [B18.6c] Wotold v2 uikit — icon button (.iconbtn from wk.css).

import { Icon, type IconName } from './Icon';

interface IconBtnProps {
  icon: IconName;
  label: string;
  size?: 'sm' | 'lg';
  active?: boolean;
  tip?: string;
  tipSide?: 'right';
  iconSize?: number;
  onClick?: () => void;
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
  const className =
    'iconbtn' + (tip ? ' tip' : '') + (tip && tipSide === 'right' ? ' tip--right' : '');
  return (
    <button
      type="button"
      className={className}
      data-size={size}
      data-active={active ? 'true' : undefined}
      data-tip={tip}
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
}
