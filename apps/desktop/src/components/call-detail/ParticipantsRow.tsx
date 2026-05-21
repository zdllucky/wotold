// Participants row — sp chips для confirmed speakers + "· N участника".
// [V5.2] Dedupe по contact_id — STT может одного человека разбить на S1+S2,
// показываем только уникальных людей.

import type { CallSpeakerView } from '../../api/speakers';
import { useI18n } from '../../i18n';
import { pluralParticipants } from '../../utils/callMeta';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

function initials(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? '')
    .join('');
}

interface ParticipantsRowProps {
  speakers: CallSpeakerView[];
}

export function ParticipantsRow({ speakers }: ParticipantsRowProps) {
  const { t, locale } = useI18n();
  const namedAll = speakers.filter((s) => s.confirmed && s.contact_display_name);
  // Уникальные по contact_id (если есть; иначе fallback на speaker.id).
  const seen = new Set<string>();
  const named: CallSpeakerView[] = [];
  for (const s of namedAll) {
    const key = s.contact_id ?? `__sp_${s.id}`;
    if (!seen.has(key)) {
      seen.add(key);
      named.push(s);
    }
  }
  if (named.length === 0) return null;
  const declN =
    locale === 'ru'
      ? pluralParticipants(named.length)
      : named.length === 1
        ? t('participants.one')
        : t('participants.many');
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        flexWrap: 'wrap',
      }}
    >
      {named.map((s, i) => (
        <span className="sp" key={s.id}>
          <span
            className="sp-avatar"
            style={{ background: SP_COLORS[i % SP_COLORS.length] }}
          >
            {initials(s.contact_display_name ?? '')}
          </span>
          {s.contact_display_name}
        </span>
      ))}
      <span className="muted" style={{ fontSize: 12, marginLeft: 4 }}>
        · {named.length} {declN}
      </span>
    </div>
  );
}
