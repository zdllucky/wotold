// [V6.1] CallErrorRow — inline error inside a calls list row.
//
// Quiet, не кричит. Audio безусловно сохранён локально (важная
// reassurance для юзера). "подробнее →" опен'ит full ErrorScreen
// в CallDetailPage.

import { useI18n } from '../../i18n';
import type { CallError } from '../../types/callState';

export interface CallErrorRowProps {
  error: CallError;
  onOpenDetails: () => void;
}

export function CallErrorRow({ error, onOpenDetails }: CallErrorRowProps) {
  const { t } = useI18n();
  // First short phrase from message (before " — " / "." / newline).
  const shortMsg =
    error.message.split(/[—.\n]/)[0]?.trim() || t('callState.errorFallback');
  return (
    <div className="call-error-row" data-comment-anchor="calls-list-error-row">
      <span>
        {shortMsg} · {t('callState.audioSaved')}
      </span>
      <button
        type="button"
        className="btn btn--quiet call-error-row-link"
        onClick={onOpenDetails}
      >
        {t('callState.moreDetails')}
      </button>
    </div>
  );
}
