// Smoke tests for AudioScrubber — Wotold v2 .player-dock / .player.

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { AudioScrubber } from './AudioScrubber';
import type { CallAudioHandle } from '../hooks/useCallAudio';

afterEach(() => cleanup());

function mockAudio(over: Partial<CallAudioHandle> = {}): CallAudioHandle {
  return {
    playing: false,
    currentTime: 25,
    duration: 100,
    bothMissing: false,
    ready: true,
    peaks: null,
    togglePlay: vi.fn(),
    seek: vi.fn(),
    ...over,
  } as unknown as CallAudioHandle;
}

describe('AudioScrubber', () => {
  test('renders .player with play button and 130 bars', () => {
    render(<AudioScrubber audio={mockAudio()} seed={7} />);
    expect(document.querySelector('.player-dock')).toBeTruthy();
    expect(document.querySelector('.player')).toBeTruthy();
    expect(document.querySelector('.player-play')).toBeTruthy();
    expect(document.querySelectorAll('.player-wave > i').length).toBe(130);
    expect(document.querySelector('.player-head')).toBeTruthy();
  });

  test('renders bars from real peaks when present', () => {
    const peaks = Array.from({ length: 200 }, () => 0.5);
    render(<AudioScrubber audio={mockAudio({ peaks })} seed={7} />);
    expect(document.querySelectorAll('.player-wave > i').length).toBe(130);
  });

  test('returns null when both tracks missing', () => {
    const { container } = render(
      <AudioScrubber audio={mockAudio({ bothMissing: true })} seed={7} />,
    );
    expect(container.firstChild).toBeNull();
  });

  test('returns null when disabled', () => {
    const { container } = render(
      <AudioScrubber audio={mockAudio()} seed={7} enabled={false} />,
    );
    expect(container.firstChild).toBeNull();
  });
});
