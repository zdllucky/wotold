// [P5.3] extractSamples per-track lookup tests.
// Fixes anonymous mic speakers silence — heuristic `OWNER_TAG ? micSrc :
// systemSrc` шлёт `speaker:N` post-P1.2 на systemSrc вместо micSrc.

import { describe, expect, test } from 'vitest';
import { extractSamples } from './SpeakersSection';

const MIC_SRC = 'tauri://mic';
const SYS_SRC = 'tauri://system';

function makeRaw(opts: {
  mic?: Array<{ tag: string; start: number; end: number; text: string }>;
  system?: Array<{ tag: string; start: number; end: number; text: string }>;
  merged?: Array<{ tag: string; start: number; end: number; text: string }>;
}): string {
  const toSegments = (
    segs: Array<{ tag: string; start: number; end: number; text: string }> | undefined,
  ) =>
    (segs ?? []).map((s) => ({
      speakerTag: s.tag,
      start: s.start,
      end: s.end,
      text: s.text,
    }));
  const doc: Record<string, unknown> = {};
  if (opts.mic) doc.mic = { segments: toSegments(opts.mic) };
  if (opts.system) doc.system = { segments: toSegments(opts.system) };
  if (opts.merged) doc.merged = toSegments(opts.merged);
  return JSON.stringify(doc);
}

describe('extractSamples per-track (P5.3)', () => {
  test('owner на mic dorozhke → micSrc', () => {
    const json = makeRaw({
      mic: [{ tag: 'owner', start: 0, end: 3, text: 'Привет, это я говорю' }],
      system: [],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.get('owner')?.src).toBe(MIC_SRC);
  });

  test('system speaker → systemSrc', () => {
    const json = makeRaw({
      mic: [],
      system: [{ tag: 'speaker:0', start: 0, end: 3, text: 'А это собеседник на колонках' }],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.get('speaker:0')?.src).toBe(SYS_SRC);
  });

  test('anonymous mic speaker (post-P1.2) → micSrc вместо systemSrc', () => {
    // Это был root bug: `speaker:1` на mic-дорожке (P1.2 диаризация
    // выделила второй голос на микрофоне), старая heuristic шлёт на
    // systemSrc → тишина. P5.3 ловит из mic.segments → micSrc.
    const json = makeRaw({
      mic: [
        { tag: 'owner', start: 0, end: 5, text: 'Я говорю первый довольно длинно' },
        { tag: 'speaker:1', start: 6, end: 9, text: 'А я второй голос на mic' },
      ],
      system: [],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.get('speaker:1')?.src).toBe(MIC_SRC);
  });

  test('speaker в обеих track → pick longer text', () => {
    const json = makeRaw({
      mic: [{ tag: 'speaker:0', start: 0, end: 2, text: 'Короткий' }],
      system: [
        { tag: 'speaker:0', start: 5, end: 10, text: 'Гораздо более длинный кусок текста' },
      ],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.get('speaker:0')?.src).toBe(SYS_SRC);
  });

  test('legacy merged-only JSON (без mic/system) → backwards-compat heuristic', () => {
    const json = makeRaw({
      merged: [
        { tag: 'owner', start: 0, end: 3, text: 'Owner на mic-дорожке' },
        { tag: 'speaker:0', start: 5, end: 8, text: 'Speaker:0 на system-дорожке' },
      ],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.get('owner')?.src).toBe(MIC_SRC);
    expect(result.get('speaker:0')?.src).toBe(SYS_SRC);
  });

  test('пустой json → empty map', () => {
    expect(extractSamples(null, MIC_SRC, SYS_SRC).size).toBe(0);
    expect(extractSamples('{}', MIC_SRC, SYS_SRC).size).toBe(0);
    expect(extractSamples('not json', MIC_SRC, SYS_SRC).size).toBe(0);
  });

  test('сегменты с text < MIN_SAMPLE_LEN отсеиваются', () => {
    const json = makeRaw({
      mic: [
        { tag: 'owner', start: 0, end: 1, text: 'a' }, // too short
        { tag: 'owner', start: 1, end: 2, text: 'b' },
      ],
    });
    const result = extractSamples(json, MIC_SRC, SYS_SRC);
    expect(result.has('owner')).toBe(false);
  });
});
