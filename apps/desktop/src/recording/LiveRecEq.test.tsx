// Smoke tests for LiveRecEq — live audio waveform wrapper (RecEq + useAudioLevel).

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

vi.mock('../hooks/useAudioLevel', () => ({
  useAudioLevel: () => ({
    mic: [0.1, 0.2, 0.3],
    system: [0, 0, 0],
    lastUpdate: 0,
    connected: true,
  }),
}));

import { LiveRecEq } from './LiveRecEq';

afterEach(() => cleanup());

describe('LiveRecEq', () => {
  test('renders the .rec-eq equalizer with 3 bars', () => {
    render(<LiveRecEq />);
    const eq = document.querySelector('.rec-eq');
    expect(eq).toBeTruthy();
    expect(eq!.querySelectorAll('span').length).toBe(3);
  });

  test('inherit adds .rec-eq--inherit (currentColor bars on danger buttons)', () => {
    render(<LiveRecEq inherit />);
    expect(document.querySelector('.rec-eq--inherit')).toBeTruthy();
  });

  test('paused freezes (.rec-eq--paused, no live)', () => {
    render(<LiveRecEq paused />);
    expect(document.querySelector('.rec-eq--paused')).toBeTruthy();
    expect(document.querySelector('.rec-eq--live')).toBeNull();
  });
});
