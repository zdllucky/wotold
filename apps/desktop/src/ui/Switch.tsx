// [B18.6c] Wotold v2 uikit — toggle switch (.switch from wk.css).

import type { CSSProperties } from 'react';

interface SwitchProps {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
  style?: CSSProperties;
  className?: string;
}

export function Switch({ checked, onChange, label, disabled, style, className }: SwitchProps) {
  return (
    <button
      type="button"
      className={className ? `switch ${className}` : 'switch'}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-on={checked ? 'true' : undefined}
      disabled={disabled}
      style={style}
      onClick={() => onChange(!checked)}
    />
  );
}
