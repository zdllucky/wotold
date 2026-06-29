// [B18.6c] Wotold v2 uikit — settings row (.setting-row from wk.css).

import type { ReactNode } from 'react';

interface SettingRowProps {
  label: string;
  hint?: string;
  control?: ReactNode;
  disabled?: boolean;
  children?: ReactNode;
}

export function SettingRow({ label, hint, control, disabled, children }: SettingRowProps) {
  return (
    <div className="setting-row" style={disabled ? { opacity: 0.6 } : undefined}>
      <div className="setting-row-text">
        <div className="setting-row-label">{label}</div>
        {hint && <div className="set-hint">{hint}</div>}
      </div>
      {control ?? children}
    </div>
  );
}
