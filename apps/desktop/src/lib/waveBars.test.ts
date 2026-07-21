// [UI-fix A] waveBarCount — адаптивное число баров от ширины контейнера.

import { describe, expect, it } from 'vitest';
import { DEFAULT_WAVE_BARS, waveBarCount } from './waveBars';

describe('waveBarCount', () => {
  it('returns default for unmeasured width (0 / negative / NaN)', () => {
    expect(waveBarCount(0)).toBe(DEFAULT_WAVE_BARS);
    expect(waveBarCount(-10)).toBe(DEFAULT_WAVE_BARS);
    expect(waveBarCount(Number.NaN)).toBe(DEFAULT_WAVE_BARS);
  });

  it('gives ~130 bars at the design width (585px ≈ full-width dock)', () => {
    expect(waveBarCount(585)).toBe(130);
  });

  it('scales down proportionally on narrow widths', () => {
    expect(waveBarCount(300)).toBe(67);
  });

  it('clamps to lower bound 40', () => {
    expect(waveBarCount(50)).toBe(40);
    expect(waveBarCount(1)).toBe(40);
  });

  it('clamps to upper bound 180', () => {
    expect(waveBarCount(5000)).toBe(180);
    expect(waveBarCount(810)).toBe(180);
  });
});
