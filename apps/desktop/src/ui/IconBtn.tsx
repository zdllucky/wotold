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
      title={title}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon name={icon} size={s} />
    </button>
  );
}
