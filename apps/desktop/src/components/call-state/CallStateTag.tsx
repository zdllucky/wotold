// [V6.1] CallStateTag — unified status badge для всех call lifecycle states.
//
// Pure presentational. Используется в CallsPage rows + CallDetailPage
// header + PipelineStrip summary. Translation labels — через t().

import { useI18n } from '../../i18n';
import type { CallState } from '../../types/callState';

export interface CallStateTagProps {
  state: CallState;
  /** Optional appended value: "64%", "#2", "STT_TIMEOUT". */
  detail?: string | number;
  /** Override default i18n label (например если caller хочет custom копи). */
  labelOverride?: string;
}

export function CallStateTag({ state, detail, labelOverride }: CallStateTagProps) {
  const { t } = useI18n();
  const label = labelOverride ?? t(`callState.${state}`);
  return (
    <span className={`stat-tag stat-tag--${state}`}>
      <span className="stat-tag-dot" aria-hidden="true" />
      {label}
      {detail !== undefined && detail !== '' && <> · {detail}</>}
    </span>
  );
}
