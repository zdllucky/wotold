import type { EngineKind } from '@wotold/contracts';
import { useI18n } from '../i18n';

export type { EngineKind };
export type EngineChipVariant = 'inline' | 'header' | 'recording';

interface EngineChipProps {
  kind: EngineKind;
  variant?: EngineChipVariant;
}

export function EngineChip({ kind, variant = 'inline' }: EngineChipProps) {
  const { t } = useI18n();

  const labelKey = `engineChip.${kind}` as
    | 'engineChip.local'
    | 'engineChip.cloud_managed'
    | 'engineChip.cloud_byo';
  const ariaKey = `engineChip.${kind}Aria` as
    | 'engineChip.localAria'
    | 'engineChip.cloud_managedAria'
    | 'engineChip.cloud_byoAria';

  return (
    <span
      className={[
        'engine-chip',
        `engine-chip--${kind.replace('_', '-')}`,
        variant !== 'inline' ? `engine-chip--${variant}` : '',
      ]
        .filter(Boolean)
        .join(' ')}
      aria-label={t(ariaKey)}
      title={t(ariaKey)}
    >
      <span className="engine-chip-dot" aria-hidden="true" />
      {t(labelKey)}
    </span>
  );
}
