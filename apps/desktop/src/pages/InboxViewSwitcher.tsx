// [B20.4] Переключатель видов инбокса (list/cards/week/month) — вынесен из
// InboxView для 800-line guard.

import { Segmented, type SegOption } from '../ui';
import type { IconName } from '../ui/Icon';
import type { useI18n, TranslationKey } from '../i18n';

type TFn = ReturnType<typeof useI18n>['t'];

export type InboxViewMode = 'list' | 'cards' | 'week' | 'month';

const VIEW_DEFS: [InboxViewMode, IconName, TranslationKey][] = [
  ['list', 'list', 'inbox.viewList'],
  ['cards', 'grid', 'inbox.viewCards'],
  ['week', 'calendarWeek', 'inbox.viewWeek'],
  ['month', 'calendar', 'inbox.viewMonth'],
];

export function ViewSwitcher({
  view,
  setView,
  t,
}: {
  view: InboxViewMode;
  setView: (v: InboxViewMode) => void;
  t: TFn;
}) {
  const options: SegOption<InboxViewMode>[] = VIEW_DEFS.map(([v, icon, key]) => ({
    value: v,
    label: t(key),
    icon,
  }));
  return (
    <Segmented<InboxViewMode>
      options={options}
      value={view}
      onChange={setView}
      iconOnly
      ariaLabel={t('inbox.viewLabel')}
    />
  );
}
