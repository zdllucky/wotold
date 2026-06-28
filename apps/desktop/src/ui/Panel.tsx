// [B18.6c] Wotold v2 uikit — surface panel (.panel from wk.css).

import type { HTMLAttributes } from 'react';

interface PanelProps extends HTMLAttributes<HTMLDivElement> {
  raised?: boolean;
}

export function Panel({ raised, className, children, ...rest }: PanelProps) {
  return (
    <div
      className={'panel' + (raised ? ' panel--raised' : '') + (className ? ' ' + className : '')}
      {...rest}
    >
      {children}
    </div>
  );
}
