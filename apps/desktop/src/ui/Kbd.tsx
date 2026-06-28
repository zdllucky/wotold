// [B18.6c] Wotold v2 uikit — keyboard hint (.kbd from wk.css).

import type { ReactNode } from 'react';

interface KbdProps {
  children: ReactNode;
}

export function Kbd({ children }: KbdProps) {
  return <span className="kbd">{children}</span>;
}
