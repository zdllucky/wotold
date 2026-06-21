import { describe, expect, test } from 'vitest';

import { bucketPeaks } from './audioPeaks';

// sampleRate=1 → startSec/endSec мапятся прямо в индексы сэмплов.
describe('bucketPeaks', () => {
  test('full range — макс по бакетам, нормализация 0..1', () => {
    const ch = [0, 0.5, 1, 0.25];
    expect(bucketPeaks(ch, 1, 0, 4, 2)).toEqual([0.5, 1]);
  });

  test('sub-range выделяет верные сэмплы', () => {
    const ch = [0, 0.5, 1, 0.25];
    // [2,4): сэмплы 1 и 0.25 → max-нормализация по диапазону → [1, 0.25].
    expect(bucketPeaks(ch, 1, 2, 4, 2)).toEqual([1, 0.25]);
  });

  test('берёт abs (отрицательные амплитуды)', () => {
    const ch = [-1, -0.5, 0.25, 0];
    expect(bucketPeaks(ch, 1, 0, 4, 2)).toEqual([1, 0.25]);
  });

  test('end<=start → нули', () => {
    expect(bucketPeaks([0.1, 0.2, 0.3], 1, 2, 2, 2)).toEqual([0, 0]);
  });

  test('count<=0 → пустой массив', () => {
    expect(bucketPeaks([0.1, 0.2], 1, 0, 2, 0)).toEqual([]);
  });

  test('тишина (все нули) → нули без деления на ноль', () => {
    expect(bucketPeaks([0, 0, 0, 0], 1, 0, 4, 2)).toEqual([0, 0]);
  });
});
