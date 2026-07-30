// [M14 T-11] CallTypeBadge — chip в header CallDetailPage показывающий
// classified call_type (sales_discovery, standup, one_on_one и т.д.).
//
// Render guards (avoid noise):
// - callType null → не рендерим (legacy schema_version=1 или ещё не
//   обработанные звонки).
// - callType === 'other' с low confidence (<0.5) → не рендерим.
// - В остальных случаях — chip с label из i18n + accent dot.
//
// Pattern mirror [EngineChip](apps/desktop/src/components/EngineChip.tsx).

import { useI18n } from '../../i18n';
import type { Call } from '../../api/recording';

type CallType = NonNullable<Call['call_type']>;

interface CallTypeBadgeProps {
  callType: Call['call_type'];
  confidence?: number | null;
}

export function CallTypeBadge({ callType, confidence }: CallTypeBadgeProps) {
  const { t } = useI18n();
  if (callType === null || callType === undefined) {
    return null;
  }
  if (callType === 'other' && (confidence ?? 0) < 0.5) {
    return null;
  }
  const labelKey = `callType.${callType}` as
    | 'callType.sales_discovery'
    | 'callType.sales_demo'
    | 'callType.product_sync'
    | 'callType.standup'
    | 'callType.customer_interview'
    | 'callType.one_on_one'
    | 'callType.strategy_brainstorm'
    | 'callType.status_update'
    | 'callType.other';

  return (
    <span
      className={`engine-chip engine-chip--header call-type-chip call-type-chip--${callType.replace('_', '-')}`}
      title={t(labelKey)}
      aria-label={t(labelKey)}
    >
      <span className={`engine-chip-dot dot--${dotVariantFor(callType)}`} aria-hidden="true" />
      {t(labelKey)}
    </span>
  );
}

/// Color mapping (варианты .dot):
/// - Sales (discovery, demo) → accent (бизнес-критично)
/// - Internal team (standup, product_sync, status_update) → muted
/// - Research (customer_interview) → accent-soft
/// - 1:1 (one_on_one) → warning (privacy-sensitive)
/// - Brainstorm → success (creative)
/// - Other → muted
function dotVariantFor(ct: CallType): string {
  switch (ct) {
    case 'sales_discovery':
    case 'sales_demo':
      return 'accent';
    case 'customer_interview':
      return 'accent';
    case 'one_on_one':
      return 'warning';
    case 'strategy_brainstorm':
      return 'success';
    case 'product_sync':
    case 'standup':
    case 'status_update':
    case 'other':
    default:
      return 'muted';
  }
}
