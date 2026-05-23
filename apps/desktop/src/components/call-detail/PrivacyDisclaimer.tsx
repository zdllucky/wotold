// [M14 T-11] PrivacyDisclaimer — banner для 1:1 встреч (privacy-sensitive).
//
// Рендерится в CallDetailPage перед табами когда `call_type === 'one_on_one'`.
// Не dismissable — приватность это не уведомление, а постоянное напоминание.
// PRD §5.5 expert/one_on_one.txt: содержит личную обратную связь, evidence
// quotes paraphrased, action_items только work commitments.

import { useI18n } from '../../i18n';

export function PrivacyDisclaimer() {
  const { t } = useI18n();
  return (
    <div className="privacy-disclaimer" role="note">
      <div className="privacy-disclaimer-title">{t('privacyDisclaimer.oneOnOneTitle')}</div>
      <div className="privacy-disclaimer-body">{t('privacyDisclaimer.oneOnOneBody')}</div>
    </div>
  );
}
