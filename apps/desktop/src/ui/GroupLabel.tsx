// [B21] Заголовок группы строк внутри settings-секции (канон GroupLabel
// прототипа → .rrail-sec). Вынесен в ui, чтобы page-модули не импортировали
// друг друга (SettingsPage ↔ pages/engine circular-import guard).

import type { ReactNode } from 'react';

export function GroupLabel({ children, top = 26 }: { children: ReactNode; top?: number }) {
  return (
    <div className="rrail-sec" style={{ marginTop: top, marginBottom: 2 }}>
      {children}
    </div>
  );
}
