import { describe, expect, test } from 'vitest';

import {
  capitalize,
  findSpeakerAtTime,
  formatHeaderMeta,
  hashCallId,
  humanDuration,
  humanSpeakerLabel,
  pluralParticipants,
  simpleDateTitle,
} from './callMeta';
import type { Call } from '../api/recording';
import type { CallSpeakerView } from '../api/speakers';

function mkCall(overrides: Partial<Call> = {}): Call {
  return {
    id: 'c-abc12345',
    title: null,
    started_at: '2026-05-20T16:04:30Z',
    ended_at: '2026-05-20T16:05:42Z',
    duration_sec: 72,
    status: 'ready',
    provider: 'soniox',
    path_label: 'managed',
    lang_detected: 'ru',
    failed_reason: null,
    recap_failed_reason: null,
    created_at: '2026-05-20T16:04:30Z',
    updated_at: '2026-05-20T16:05:42Z',
    ...overrides,
  };
}

describe('humanDuration', () => {
  test('seconds only', () => {
    expect(humanDuration(0)).toBe('0 сек');
    expect(humanDuration(45)).toBe('45 сек');
    expect(humanDuration(59)).toBe('59 сек');
  });

  test('whole minutes', () => {
    expect(humanDuration(60)).toBe('1 мин');
    expect(humanDuration(300)).toBe('5 мин');
  });

  test('minutes + seconds', () => {
    expect(humanDuration(72)).toBe('1 мин 12 сек');
    expect(humanDuration(3599)).toBe('59 мин 59 сек');
  });

  test('hours', () => {
    expect(humanDuration(3600)).toBe('1 ч');
    expect(humanDuration(3660)).toBe('1 ч 1 мин');
    expect(humanDuration(7320)).toBe('2 ч 2 мин');
  });
});

describe('capitalize', () => {
  test('uppercases first char only', () => {
    expect(capitalize('среда')).toBe('Среда');
    expect(capitalize('a')).toBe('A');
  });

  test('empty / single → returns as-is', () => {
    expect(capitalize('')).toBe('');
  });
});

describe('formatHeaderMeta', () => {
  test('full meta line with duration', () => {
    const call = mkCall({
      started_at: '2026-05-20T16:04:00Z',
      duration_sec: 72,
    });
    const out = formatHeaderMeta(call);
    // Russian weekday capitalized + date + time + humanized duration, joined by ' · '
    expect(out).toMatch(/^[А-ЯЁ][а-яё]+/); // capitalized weekday cyrillic
    expect(out).toContain('20 мая');
    expect(out).toContain('1 мин 12 сек');
    expect(out.split(' · ').length).toBe(4);
  });

  test('omits duration when zero', () => {
    const call = mkCall({ duration_sec: 0 });
    const out = formatHeaderMeta(call);
    expect(out.split(' · ').length).toBe(3);
  });

  test('returns raw started_at on invalid date', () => {
    const call = mkCall({ started_at: 'not-a-date' });
    const out = formatHeaderMeta(call);
    expect(out).toBe('not-a-date');
  });
});

describe('simpleDateTitle', () => {
  test('formats date in Russian', () => {
    const call = mkCall({ started_at: '2026-05-20T16:04:00Z' });
    expect(simpleDateTitle(call)).toBe('Звонок · 20 мая');
  });

  test('fallback on invalid date includes id slice', () => {
    const call = mkCall({ id: 'abcdef1234567890', started_at: 'invalid' });
    expect(simpleDateTitle(call)).toBe('Звонок abcdef12');
  });
});

describe('hashCallId', () => {
  test('deterministic for same id', () => {
    const a = hashCallId('call-123');
    const b = hashCallId('call-123');
    expect(a).toBe(b);
  });

  test('different ids → likely different hashes', () => {
    expect(hashCallId('call-a')).not.toBe(hashCallId('call-b'));
  });

  test('always non-negative and < 1000', () => {
    for (const id of ['', 'a', 'long-call-id-zzzz', '1234567890123456789']) {
      const h = hashCallId(id);
      expect(h).toBeGreaterThanOrEqual(0);
      expect(h).toBeLessThan(1000);
    }
  });
});

describe('pluralParticipants', () => {
  test('1 → участник', () => {
    expect(pluralParticipants(1)).toBe('участник');
    expect(pluralParticipants(21)).toBe('участник');
    expect(pluralParticipants(101)).toBe('участник');
  });

  test('2-4 → участника', () => {
    expect(pluralParticipants(2)).toBe('участника');
    expect(pluralParticipants(3)).toBe('участника');
    expect(pluralParticipants(4)).toBe('участника');
    expect(pluralParticipants(22)).toBe('участника');
    expect(pluralParticipants(103)).toBe('участника');
  });

  test('5-20 → участников', () => {
    expect(pluralParticipants(5)).toBe('участников');
    expect(pluralParticipants(10)).toBe('участников');
    expect(pluralParticipants(11)).toBe('участников');
    expect(pluralParticipants(12)).toBe('участников');
    expect(pluralParticipants(14)).toBe('участников');
    expect(pluralParticipants(20)).toBe('участников');
  });

  test('0 → участников (общее правило)', () => {
    expect(pluralParticipants(0)).toBe('участников');
  });
});

describe('humanSpeakerLabel', () => {
  test('owner → Я', () => {
    expect(humanSpeakerLabel('owner')).toBe('Я');
  });

  test('Speaker N → Голос N+1 (Soniox формат)', () => {
    expect(humanSpeakerLabel('Speaker 0')).toBe('Голос 1');
    expect(humanSpeakerLabel('Speaker 5')).toBe('Голос 6');
    expect(humanSpeakerLabel('Speaker 12')).toBe('Голос 13');
  });

  test('SN → Голос N+1 (сокращённый формат)', () => {
    expect(humanSpeakerLabel('S0')).toBe('Голос 1');
    expect(humanSpeakerLabel('S3')).toBe('Голос 4');
  });

  test('кастомный тег возвращается как есть', () => {
    expect(humanSpeakerLabel('Marina')).toBe('Marina');
    expect(humanSpeakerLabel('Customer 1')).toBe('Customer 1');
  });

  test('пустой / странный input — fallback на "Голос"', () => {
    expect(humanSpeakerLabel('')).toBe('Голос');
  });

  test('case-insensitive Speaker', () => {
    expect(humanSpeakerLabel('speaker 0')).toBe('Голос 1');
    expect(humanSpeakerLabel('SPEAKER 9')).toBe('Голос 10');
  });
});

// ─── findSpeakerAtTime ─────────────────────────────────────────────

function mkSpeaker(
  speaker_tag: string,
  contact_display_name: string | null,
  confirmed: boolean,
): CallSpeakerView {
  return {
    id: `cs-${speaker_tag}`,
    call_id: 'c-1',
    speaker_tag,
    contact_id: confirmed && contact_display_name ? `contact-${speaker_tag}` : null,
    contact_display_name,
    suggestion_contact_id: null,
    suggestion_contact_display_name: null,
    suggestion_score: null,
    suggestion_source: null,
    confirmed,
  };
}

const sampleRawStt = JSON.stringify({
  version: 1,
  merged: [
    { start: 0, end: 5, text: 'привет', speakerTag: 'owner' },
    { start: 5, end: 10, text: 'добрый день', speakerTag: 'S1' },
    { start: 10, end: 12, text: 'хорошо', speakerTag: 'owner' },
    { start: 12, end: 18, text: 'окей', speakerTag: 'S2' },
  ],
});

describe('findSpeakerAtTime', () => {
  test('returns owner with «Я» display by default', () => {
    const out = findSpeakerAtTime(sampleRawStt, [], 2);
    expect(out).toEqual({ tag: 'owner', displayName: 'Я', colorIdx: 0 });
  });

  test('uses contact_display_name for confirmed match', () => {
    const speakers = [mkSpeaker('S1', 'Иван', true)];
    const out = findSpeakerAtTime(sampleRawStt, speakers, 7);
    expect(out?.displayName).toBe('Иван');
    expect(out?.tag).toBe('S1');
    expect(out?.colorIdx).toBe(1);
  });

  test('falls back to humanSpeakerLabel for unconfirmed speaker', () => {
    // S2 не confirmed → fallback на человечный label "Голос 3".
    const speakers = [mkSpeaker('S2', 'Marina', false)];
    const out = findSpeakerAtTime(sampleRawStt, speakers, 14);
    expect(out?.displayName).toBe('Голос 3');
  });

  test('250ms slack за конец сегмента', () => {
    // Сегмент кончается на 5.0, slack 0.25 → 5.2 ещё owner.
    const out = findSpeakerAtTime(sampleRawStt, [], 5.2);
    expect(out?.tag).toBe('owner');
  });

  test('null at unknown time (gap)', () => {
    // Empty raw or beyond last segment + slack.
    const out = findSpeakerAtTime(sampleRawStt, [], 999);
    expect(out).toBeNull();
  });

  test('null on null/invalid raw', () => {
    expect(findSpeakerAtTime(null, [], 0)).toBeNull();
    expect(findSpeakerAtTime('not-json', [], 0)).toBeNull();
    expect(findSpeakerAtTime('{}', [], 0)).toBeNull();
  });

  test('null on NaN/Infinity time', () => {
    expect(findSpeakerAtTime(sampleRawStt, [], NaN)).toBeNull();
    expect(findSpeakerAtTime(sampleRawStt, [], Infinity)).toBeNull();
  });

  test('skips malformed segments в merged', () => {
    const corruptRaw = JSON.stringify({
      version: 1,
      merged: [
        null,
        { start: 0, end: 5 }, // missing speakerTag
        { speakerTag: 'S1', start: 5, end: 10 },
      ],
    });
    const out = findSpeakerAtTime(corruptRaw, [], 7);
    expect(out?.tag).toBe('S1');
  });
});
