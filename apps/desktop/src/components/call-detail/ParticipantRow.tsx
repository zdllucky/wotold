// [B20.7] Строка подтверждённого участника в rail: один голос → иконка
// отвязки справа; несколько голосов → dropdown со строками по голосу
// (label + прослушать сэмпл + отвязать конкретный голос).
//
// ВАЖНО: панель Dropdown закрывается на любой клик (Menu.tsx onClick close) —
// кнопка прослушивания гасит всплытие (playback не схлопывает меню),
// отвязка и клик по строке bubble'ят → меню закрывается.

import type { CallSpeakerView } from '../../api/speakers';
import type { SpeakerSample } from '../SpeakerCard';
import type { ConfirmedGroup } from './participantGroups';
import { Dropdown } from '../../ui/Menu';
import { IconBtn } from '../../ui/IconBtn';
import { useI18n } from '../../i18n';
import { humanSpeakerLabel } from '../../utils/callMeta';
import { VoiceSampleButton } from './VoiceSampleButton';

function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
  );
}

interface ParticipantRowProps {
  group: ConfirmedGroup;
  color: string;
  samplesByTag: Map<string, SpeakerSample | null>;
  onUnbind: (callSpeakerId: string) => void;
}

function VoiceMenuRow({
  speaker,
  sample,
  onUnbind,
}: {
  speaker: CallSpeakerView;
  sample: SpeakerSample | null;
  onUnbind: (callSpeakerId: string) => void;
}) {
  const { t } = useI18n();
  const label = humanSpeakerLabel(speaker.speaker_tag, t);
  return (
    <div
      className="menu-item"
      role="presentation"
      style={{ cursor: 'default', display: 'flex', alignItems: 'center', gap: 8 }}
    >
      <span className="u-trunc" style={{ flex: 1 }}>
        {label}
      </span>
      {/* stopPropagation только у прослушивания: панель Dropdown закрывается
          на любой клик, playback не должен схлопывать меню. Отвязка bubble'ит
          → меню закрывается (естественное поведение). */}
      <span style={{ display: 'inline-flex' }} onClick={(e) => e.stopPropagation()}>
        <VoiceSampleButton sample={sample} />
      </span>
      <IconBtn
        icon="x"
        size="sm"
        label={t('speakers.unbindAria', { label })}
        onClick={() => onUnbind(speaker.id)}
      />
    </div>
  );
}

export function ParticipantRow({ group, color, samplesByTag, onUnbind }: ParticipantRowProps) {
  const { t } = useI18n();
  const name = group.displayName || t('callDetail.railSpeakerUnknown');
  const multi = group.speakers.length > 1;
  const first = group.speakers[0];

  return (
    <div className="lrow" style={{ padding: '5px 0', gap: 10 }}>
      <span
        className="avatar"
        style={{ width: 28, height: 28, background: color, fontSize: 11, flex: '0 0 auto' }}
      >
        {initials(name)}
      </span>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div className="u-trunc" style={{ fontWeight: 550, color: 'var(--text)' }}>
          {name}
        </div>
        {multi && (
          <div className="u-faint" style={{ fontSize: 11.5 }}>
            {t('callDetail.railVoicesCount', { n: group.speakers.length })}
          </div>
        )}
      </div>
      {multi ? (
        <Dropdown
          align="right"
          width={230}
          trigger={({ toggle, open }) => (
            <IconBtn
              icon="chevronDown"
              size="sm"
              label={t('callDetail.railVoicesMenu')}
              onClick={toggle}
              hasPopup
              expanded={open}
            />
          )}
        >
          {group.speakers.map((s) => (
            <VoiceMenuRow
              key={s.id}
              speaker={s}
              sample={samplesByTag.get(s.speaker_tag) ?? null}
              onUnbind={onUnbind}
            />
          ))}
        </Dropdown>
      ) : (
        first && (
          <IconBtn
            icon="x"
            size="sm"
            label={t('speakers.unbindAria', { label: name })}
            onClick={() => onUnbind(first.id)}
          />
        )
      )}
    </div>
  );
}
