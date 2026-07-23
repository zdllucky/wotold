// [B26.8] Время сообщения (низ-справа облачка): короткий формат
// (сегодня HH:MM / вчера HH:MM / дата), клик — toggle на полную дату+время.
// Выделяется цветом только в раскрытом состоянии.

import { useState } from 'react';

import { useI18n } from '../../i18n';
import { formatMsgTime, formatMsgTimeFull } from '../../utils/msgTime';

export function MsgTime({ createdAt }: { createdAt: string }) {
  const { t, locale } = useI18n();
  const [full, setFull] = useState(false);
  const fullLabel = formatMsgTimeFull(createdAt, locale);
  const label = full
    ? fullLabel
    : formatMsgTime(createdAt, new Date(), locale, t('assistant.msgYesterday'));
  if (!label) return null;
  return (
    <button
      type="button"
      className="msg-time"
      data-expanded={full || undefined}
      aria-label={fullLabel}
      aria-pressed={full}
      onClick={() => setFull((v) => !v)}
    >
      {label}
    </button>
  );
}
