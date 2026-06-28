// [B18.9] Wotold v2 uikit — shared per-screen header bar (.view-head). Enforces
// the consistent leading (icon + bold title + count chip) across every screen;
// `children` carry the screen-specific middle + spacer + right-aligned actions.

import type { CSSProperties, ReactNode } from 'react';
import { Icon, type IconName } from './Icon';
import { Chip } from './Chip';

const ICON_STYLE: CSSProperties = { color: 'var(--text-3)', flex: '0 0 auto' };
const TITLE_STYLE: CSSProperties = { fontWeight: 650, fontSize: 'var(--t-14)', flex: '0 0 auto' };

interface ViewHeadProps {
  icon?: IconName;
  title: string;
  count?: number;
  countTone?: 'line' | 'accent';
  children?: ReactNode;
}

export function ViewHead({ icon, title, count, countTone = 'line', children }: ViewHeadProps) {
  return (
    <div className="view-head">
      {icon && <Icon name={icon} size={17} style={ICON_STYLE} />}
      <span style={TITLE_STYLE}>{title}</span>
      {count != null && (
        <Chip size="sm" tone={countTone}>
          {count}
        </Chip>
      )}
      {children}
    </div>
  );
}
