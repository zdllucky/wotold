// [B18.6c] Wotold v2 uikit — segmented control (.seg from wk.css).

import type { CSSProperties } from 'react';
import { Icon, type IconName } from './Icon';

export interface SegOption<V extends string> {
  value: V;
  label: string;
  icon?: IconName;
}

interface SegmentedProps<V extends string> {
  options: SegOption<V>[];
  value: V;
  onChange: (v: V) => void;
  size?: 'sm';
  /** Icon-only buttons (label → aria-label/title) — the prototype view-switcher. */
  iconOnly?: boolean;
  ariaLabel?: string;
  style?: CSSProperties;
  className?: string;
}

export function Segmented<V extends string>({
  options,
  value,
  onChange,
  size,
  iconOnly,
  ariaLabel,
  style,
  className,
}: SegmentedProps<V>) {
  return (
    <div
      className={className ? `seg ${className}` : 'seg'}
      data-size={size}
      role="tablist"
      aria-label={ariaLabel}
      style={style}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          data-active={value === o.value ? 'true' : undefined}
          aria-selected={value === o.value}
          aria-label={iconOnly ? o.label : undefined}
          title={iconOnly ? o.label : undefined}
          onClick={() => onChange(o.value)}
          style={iconOnly ? { padding: '0 9px' } : undefined}
        >
          {o.icon && <Icon name={o.icon} size={iconOnly ? 15 : 14} />}
          {!iconOnly && o.label}
        </button>
      ))}
    </div>
  );
}
