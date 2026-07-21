// [B20.9] Тесты границ активной группы транскрипта. Ключевой кейс: смежные
// группы делят границу (g[i].end === g[i+1].start) — seek на начало следующей
// группы должен подсвечивать ЕЁ, а не предыдущую (старый inclusive-скан
// возвращал первую группу с t <= end → off-by-one).

import { describe, expect, test } from 'vitest';
import { findActiveGroupIdx, SEEK_EPS } from './transcriptActive';

const ranges = [
  { start: 0, end: 10 },
  { start: 10, end: 25 }, // общая граница с предыдущей
  { start: 30, end: 40 }, // gap 25..30
];

describe('findActiveGroupIdx', () => {
  test('пустой массив → -1', () => {
    expect(findActiveGroupIdx([], 5)).toBe(-1);
  });

  test('t до первой группы → -1', () => {
    expect(findActiveGroupIdx(ranges, -1)).toBe(-1);
  });

  test('внутри группы → её индекс', () => {
    expect(findActiveGroupIdx(ranges, 5)).toBe(0);
    expect(findActiveGroupIdx(ranges, 17)).toBe(1);
  });

  test('общая граница → СЛЕДУЮЩАЯ группа (фикс off-by-one)', () => {
    expect(findActiveGroupIdx(ranges, 10)).toBe(1);
  });

  test('gap между группами → -1', () => {
    expect(findActiveGroupIdx(ranges, 27)).toBe(-1);
  });

  test('начало группы после gap → она', () => {
    expect(findActiveGroupIdx(ranges, 30)).toBe(2);
  });

  test('конец последней группы inclusive → последняя', () => {
    expect(findActiveGroupIdx(ranges, 40)).toBe(2);
  });

  test('за концом последней → -1', () => {
    expect(findActiveGroupIdx(ranges, 40.2)).toBe(-1);
  });

  test('float-clamp чуть ниже start (epsilon) → группа всё равно активна', () => {
    // audio element может отдать 9.999999 после seek(10)
    expect(findActiveGroupIdx(ranges, 10 - SEEK_EPS / 2)).toBe(1);
  });

  test('единственная группа: границы inclusive с обеих сторон', () => {
    const one = [{ start: 2, end: 8 }];
    expect(findActiveGroupIdx(one, 2)).toBe(0);
    expect(findActiveGroupIdx(one, 8)).toBe(0);
    expect(findActiveGroupIdx(one, 8.1)).toBe(-1);
  });
});
