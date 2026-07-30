// [B18.6c, B21] Wotold v2 uikit — settings row (.setting-row from wk.css).
// Канон Row из прототипа wk-settings.jsx: label+hint слева, контрол справа,
// divider между строками (у последней в группе — last), align top для
// многострочных hint'ов, disabled приглушает всю строку.

import type { ReactNode } from 'react';

import { settingDomId } from '../pages/settingsIndex';

interface SettingRowProps {
  label: ReactNode;
  /** [B34.4] Якорь из `SETTINGS_ENTRIES` — по нему палитра ⌘K прокручивает к
   *  строке и подсвечивает её. Без него строка просто не ищется по имени. */
  settingId?: string;
  hint?: ReactNode;
  control?: ReactNode;
  /** Chip/бейдж рядом с label (нужно Permissions). */
  labelAdornment?: ReactNode;
  align?: 'center' | 'top';
  /** Последняя строка группы — без divider'а. */
  last?: boolean;
  disabled?: boolean;
  children?: ReactNode;
}

export function SettingRow({
  label,
  settingId,
  hint,
  control,
  labelAdornment,
  align = 'center',
  last,
  disabled,
  children,
}: SettingRowProps) {
  return (
    <div
      className="setting-row"
      id={settingId ? settingDomId(settingId) : undefined}
      data-align={align === 'top' ? 'top' : undefined}
      data-last={last ? 'true' : undefined}
      data-disabled={disabled ? 'true' : undefined}
    >
      <div className="setting-row-text">
        <div className="setting-row-label">
          {label}
          {labelAdornment}
        </div>
        {hint && <div className="set-hint">{hint}</div>}
      </div>
      <div className="setting-row-control">{control ?? children}</div>
    </div>
  );
}
