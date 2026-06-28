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
  ariaLabel?: string;
  style?: CSSProperties;
  className?: string;
}

export function Segmented<V extends string>({
  options,
  value,
  onChange,
  size,
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
          onClick={() => onChange(o.value)}
        >
          {o.icon && <Icon name={o.icon} size={14} />}
          {o.label}
        </button>
      ))}
    </div>
  );
}
