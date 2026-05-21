// [V7] Баннер «Авто-привязано: N · ↩ Отменить». Рендерится пока есть
// speaker'ы с auto_bound_at != null AND confirmed=1. Один клик «отменить»
// unbind'ит все авто-привязки этого звонка (caveat: не трогает manual
// confirmed). Юзер может потом пере-подтвердить вручную через таб
// «Участники» или inline «? кто это» chip.

import { useState } from 'react';
import { unbindCallSpeaker, type CallSpeakerView } from '../../api/speakers';
import { useI18n } from '../../i18n';

interface AutoBoundBannerProps {
  speakers: CallSpeakerView[];
  onUndone: () => void;
}

export function AutoBoundBanner({ speakers, onUndone }: AutoBoundBannerProps) {
  const { t } = useI18n();
  const [undoing, setUndoing] = useState(false);
  const autoBound = speakers.filter(
    (s) => s.auto_bound_at != null && s.confirmed && s.contact_id,
  );
  if (autoBound.length === 0) return null;
  const names = autoBound
    .map((s) => s.contact_display_name)
    .filter((n): n is string => Boolean(n))
    .join(', ');
  const handleUndo = async () => {
    setUndoing(true);
    try {
      await Promise.all(autoBound.map((s) => unbindCallSpeaker(s.id)));
    } catch (e) {
      console.warn('auto-bound undo failed:', e);
    } finally {
      setUndoing(false);
      onUndone();
    }
  };
  return (
    <div
      className="activity-strip"
      data-comment-anchor="call-auto-bound-banner"
      style={{ marginBottom: 14 }}
    >
      <span className="stat-tag-dot" aria-hidden="true" />
      <span>
        {autoBound.length === 1
          ? t('callDetail.autoBoundOne', { name: names })
          : t('callDetail.autoBoundMany', { n: autoBound.length, names })}
      </span>
      <button
        type="button"
        className="btn btn--quiet btn--sm"
        onClick={() => void handleUndo()}
        disabled={undoing}
        style={{ marginLeft: 'auto' }}
      >
        {undoing ? t('common.loading') : t('callDetail.autoBoundUndo')}
      </button>
    </div>
  );
}
