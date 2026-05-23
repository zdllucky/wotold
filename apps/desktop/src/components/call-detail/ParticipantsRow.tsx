// Participants row — sp chips для confirmed speakers + анонимные chips
// для unconfirmed (sortformer выделил голос, но юзер не привязал к контакту).
// [V5.2] Dedupe по contact_id — STT может одного человека разбить на S1+S2,
// показываем только уникальных людей.
// [Bug-fix] Anonymous chips — раньше диаризация могла найти 2 голоса,
// но в UI был виден только confirmed owner. Теперь видны все distinct
// `speaker:N` tags даже без contact binding'а.

import type { CallSpeakerView } from '../../api/speakers';
import { useI18n } from '../../i18n';
import { pluralParticipants } from '../../utils/callMeta';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];
/** Совпадает с backend `crate::pipeline::merge::OWNER_TAG`. */
const OWNER_TAG = 'owner';

function initials(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? '')
    .join('');
}

/** Извлечь номер из `speaker:0` → "0". Для UI label. */
function speakerOrdinal(tag: string): string {
  const m = tag.match(/^speaker:(\d+)$/);
  return m?.[1] ?? '?';
}

interface ParticipantsRowProps {
  speakers: CallSpeakerView[];
  /** Опциональный callback — клик по анонимному chip предлагает bind. */
  onConfirmAnonymous?: (speaker: CallSpeakerView) => void;
}

export function ParticipantsRow({ speakers, onConfirmAnonymous }: ParticipantsRowProps) {
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
  // [Bug-fix] Anonymous — distinct `speaker:N` без contact binding.
  // Исключаем OWNER (он либо bound либо появится среди named), unknown,
  // и dedup по speaker_tag (один тег = один анонимный chip).
  const anonymousSet = new Set<string>();
  const anonymous: CallSpeakerView[] = [];
  for (const s of speakers) {
    if (s.contact_display_name) continue;
    const tag = s.speaker_tag;
    if (!tag.startsWith('speaker:')) continue;
    if (tag === 'speaker:unknown' || tag === OWNER_TAG) continue;
    if (anonymousSet.has(tag)) continue;
    anonymousSet.add(tag);
    anonymous.push(s);
  }
  const total = named.length + anonymous.length;
  if (total === 0) return null;
  const declN =
    locale === 'ru'
      ? pluralParticipants(total)
      : total === 1
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
      {anonymous.map((s) => {
        const label = t('participants.anonymousLabel', { n: speakerOrdinal(s.speaker_tag) });
        return (
          <button
            type="button"
            key={s.id}
            className="sp sp--anonymous"
            onClick={() => onConfirmAnonymous?.(s)}
            title={t('participants.anonymousHint')}
            style={{
              background: 'transparent',
              border: '1px dashed var(--line-soft)',
              cursor: onConfirmAnonymous ? 'pointer' : 'default',
              color: 'var(--subtle)',
              fontStyle: 'italic',
            }}
          >
            <span
              className="sp-avatar"
              style={{
                background: 'var(--bg-2)',
                color: 'var(--subtle)',
              }}
            >
              ?
            </span>
            {label}
          </button>
        );
      })}
      <span className="muted" style={{ fontSize: 12, marginLeft: 4 }}>
        · {total} {declN}
      </span>
    </div>
  );
}
