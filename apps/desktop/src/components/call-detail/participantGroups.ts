// [B20.6] Группировка call_speakers для rail: подтверждённые голоса одного
// контакта → одна строка «участник» (порядок = первое появление голоса),
// неподтверждённые остаются по-голосово (у каждого своя кнопка «Определить»).

import type { CallSpeakerView } from '../../api/speakers';

export interface ConfirmedGroup {
  /** contact_id, либо синтетический ключ для confirmed-без-контакта. */
  key: string;
  displayName: string;
  /** Голоса группы в порядке появления в списке. */
  speakers: CallSpeakerView[];
}

export interface ParticipantSplit {
  confirmed: ConfirmedGroup[];
  unconfirmed: CallSpeakerView[];
}

export function splitParticipants(speakers: readonly CallSpeakerView[]): ParticipantSplit {
  const confirmed: ConfirmedGroup[] = [];
  const byKey = new Map<string, ConfirmedGroup>();
  const unconfirmed: CallSpeakerView[] = [];

  for (const s of speakers) {
    if (!s.confirmed) {
      unconfirmed.push(s);
      continue;
    }
    // Без contact_id мержить нельзя — это разные неизвестные люди.
    const key = s.contact_id ?? `__no_contact_${s.id}`;
    const existing = byKey.get(key);
    if (existing) {
      byKey.set(key, { ...existing, speakers: [...existing.speakers, s] });
      const idx = confirmed.findIndex((g) => g.key === key);
      confirmed[idx] = byKey.get(key)!;
    } else {
      const group: ConfirmedGroup = {
        key,
        displayName: s.contact_display_name ?? '',
        speakers: [s],
      };
      byKey.set(key, group);
      confirmed.push(group);
    }
  }

  return { confirmed, unconfirmed };
}
