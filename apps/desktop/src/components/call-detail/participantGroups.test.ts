// [B20.6] Тесты группировки участников: несколько диаризованных голосов,
// привязанных к одному контакту, схлопываются в одну строку rail.

import { describe, expect, test } from 'vitest';
import type { CallSpeakerView } from '../../api/speakers';
import { splitParticipants } from './participantGroups';

function speaker(over: Partial<CallSpeakerView>): CallSpeakerView {
  return {
    id: 'cs-' + (over.speaker_tag ?? 'x'),
    call_id: 'call-1',
    speaker_tag: 'spk_1',
    contact_id: null,
    contact_display_name: null,
    suggestion_contact_id: null,
    suggestion_contact_display_name: null,
    suggestion_score: null,
    suggestion_source: null,
    confirmed: false,
    auto_bound_at: null,
    ...over,
  };
}

describe('splitParticipants', () => {
  test('пустой список → пусто', () => {
    expect(splitParticipants([])).toEqual({ confirmed: [], unconfirmed: [] });
  });

  test('3 голоса → 1 контакт = одна группа с 3 голосами', () => {
    const s = [
      speaker({ speaker_tag: 'spk_1', confirmed: true, contact_id: 'c1', contact_display_name: 'Глеб Гусак' }),
      speaker({ speaker_tag: 'spk_2', confirmed: true, contact_id: 'c1', contact_display_name: 'Глеб Гусак' }),
      speaker({ speaker_tag: 'spk_3', confirmed: true, contact_id: 'c1', contact_display_name: 'Глеб Гусак' }),
    ];
    const r = splitParticipants(s);
    expect(r.confirmed).toHaveLength(1);
    expect(r.confirmed[0]!.displayName).toBe('Глеб Гусак');
    expect(r.confirmed[0]!.speakers.map((x) => x.speaker_tag)).toEqual(['spk_1', 'spk_2', 'spk_3']);
    expect(r.unconfirmed).toHaveLength(0);
  });

  test('смешанные: confirmed группируются, unconfirmed остаются по-голосово', () => {
    const s = [
      speaker({ speaker_tag: 'owner', confirmed: true, contact_id: 'me', contact_display_name: 'Дамир' }),
      speaker({ speaker_tag: 'spk_1', confirmed: true, contact_id: 'c1', contact_display_name: 'Глеб' }),
      speaker({ speaker_tag: 'spk_2' }),
      speaker({ speaker_tag: 'spk_3', confirmed: true, contact_id: 'c1', contact_display_name: 'Глеб' }),
    ];
    const r = splitParticipants(s);
    expect(r.confirmed.map((g) => g.displayName)).toEqual(['Дамир', 'Глеб']);
    expect(r.confirmed[1]!.speakers).toHaveLength(2);
    expect(r.unconfirmed.map((x) => x.speaker_tag)).toEqual(['spk_2']);
  });

  test('confirmed без contact_id НЕ мержатся между собой', () => {
    const s = [
      speaker({ speaker_tag: 'spk_1', confirmed: true, contact_display_name: 'A' }),
      speaker({ speaker_tag: 'spk_2', confirmed: true, contact_display_name: 'B' }),
    ];
    const r = splitParticipants(s);
    expect(r.confirmed).toHaveLength(2);
  });

  test('порядок стабилен: группы в порядке первого появления голоса', () => {
    const s = [
      speaker({ speaker_tag: 'spk_9', confirmed: true, contact_id: 'b', contact_display_name: 'Боря' }),
      speaker({ speaker_tag: 'spk_1', confirmed: true, contact_id: 'a', contact_display_name: 'Аня' }),
      speaker({ speaker_tag: 'spk_5', confirmed: true, contact_id: 'b', contact_display_name: 'Боря' }),
    ];
    expect(splitParticipants(s).confirmed.map((g) => g.displayName)).toEqual(['Боря', 'Аня']);
  });
});
