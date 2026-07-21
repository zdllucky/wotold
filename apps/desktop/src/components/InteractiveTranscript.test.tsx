// Tests for InteractiveTranscript.tsx — pure helpers + rendering + interactions.
//
// Pure functions (parseRawStt, groupBySpeaker, hashTag, colorVarFor,
// formatTimecode, buildLabelMap, buildUnconfirmedSet) are tested via
// observable component output since they are not exported.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { InteractiveTranscript } from './InteractiveTranscript';
import type { CallSpeakerView } from '../api/speakers';

afterEach(() => cleanup());

// ─── Helpers ─────────────────────────────────────────────────────────────────

function mkRawStt(
  segments: Array<{ start: number; end: number; text: string; speakerTag: string }>,
) {
  return JSON.stringify({ version: 1, merged: segments });
}

function mkSpeaker(
  tag: string,
  name: string | null = null,
  confirmed = true,
): CallSpeakerView {
  return {
    id: `cs-${tag}`,
    call_id: 'c-1',
    speaker_tag: tag,
    contact_id: confirmed && name ? `contact-${tag}` : null,
    contact_display_name: name,
    suggestion_contact_id: null,
    suggestion_contact_display_name: null,
    suggestion_score: null,
    suggestion_source: null,
    confirmed,
    auto_bound_at: null,
  };
}

const BASIC_SEGMENTS = [
  { start: 0, end: 3, text: 'Hello world', speakerTag: 'owner' },
  { start: 3, end: 6, text: 'Good morning', speakerTag: 'S1' },
  { start: 6, end: 9, text: 'How are you?', speakerTag: 'owner' },
];

const BASIC_RAW = mkRawStt(BASIC_SEGMENTS);

// ─── Fallback states ─────────────────────────────────────────────────────────

describe('InteractiveTranscript — fallback states', () => {
  test('shows empty state when rawSttJson is null and no fallback', () => {
    render(<InteractiveTranscript rawSttJson={null} fallbackMd={null} />);
    expect(screen.getByText(/транскрипт ещё не готов/i)).toBeInTheDocument();
  });

  test('renders fallback markdown when no raw_stt', () => {
    render(
      <InteractiveTranscript
        rawSttJson={null}
        fallbackMd="## Transcript\n\nSome **bold** text"
      />,
    );
    // ReactMarkdown renders h2 and bold text
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Transcript');
  });

  test('shows empty state when raw_stt has empty merged array', () => {
    render(
      <InteractiveTranscript
        rawSttJson={JSON.stringify({ version: 1, merged: [] })}
        fallbackMd={null}
      />,
    );
    expect(screen.getByText(/транскрипт ещё не готов/i)).toBeInTheDocument();
  });

  test('falls back to markdown when raw_stt merged is empty but fallback exists', () => {
    render(
      <InteractiveTranscript
        rawSttJson={JSON.stringify({ version: 1, merged: [] })}
        fallbackMd="## Fallback"
      />,
    );
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Fallback');
  });

  test('shows empty state when raw_stt is corrupt JSON', () => {
    render(
      <InteractiveTranscript rawSttJson="not-valid-json" fallbackMd={null} />,
    );
    expect(screen.getByText(/транскрипт ещё не готов/i)).toBeInTheDocument();
  });

  test('handles raw_stt with no version field → falls back', () => {
    const noVersion = JSON.stringify({ merged: BASIC_SEGMENTS });
    render(<InteractiveTranscript rawSttJson={noVersion} fallbackMd={null} />);
    expect(screen.getByText(/транскрипт ещё не готов/i)).toBeInTheDocument();
  });
});

// ─── Rendering segments ───────────────────────────────────────────────────────

describe('InteractiveTranscript — segment rendering', () => {
  test('renders transcript container with rows', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    const rows = document.querySelectorAll('.turn');
    // 3 segments but owner segments are grouped: [owner], [S1], [owner]
    // consecutive owner segs are NOT adjacent so they stay separate
    expect(rows.length).toBe(3);
  });

  test('renders segment text content', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    expect(screen.getByText('Hello world')).toBeInTheDocument();
    expect(screen.getByText('Good morning')).toBeInTheDocument();
  });

  test('owner tag shows "Я" as speaker label', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    const speakerLabels = document.querySelectorAll('.turn-sp');
    // First row is owner → "Я"
    expect(speakerLabels[0]?.textContent?.trim()).toMatch(/^Я/);
  });

  test('groups consecutive same-speaker segments', () => {
    const raw = mkRawStt([
      { start: 0, end: 2, text: 'First part', speakerTag: 'S1' },
      { start: 2, end: 4, text: 'Second part', speakerTag: 'S1' },
      { start: 4, end: 6, text: 'Other', speakerTag: 'owner' },
    ]);
    render(<InteractiveTranscript rawSttJson={raw} fallbackMd={null} />);
    const rows = document.querySelectorAll('.turn');
    // S1+S1 grouped → 2 rows total
    expect(rows.length).toBe(2);
    // Grouped text joined with space
    expect(screen.getByText('First part Second part')).toBeInTheDocument();
  });

  test('timecode rendered for each group', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    // group start = 0.0 → "00:00"
    expect(screen.getByText('00:00')).toBeInTheDocument();
  });

  test('renders timecode in h:mm:ss format for long segments', () => {
    const raw = mkRawStt([
      { start: 3661, end: 3665, text: 'Late segment', speakerTag: 'S1' },
    ]);
    render(<InteractiveTranscript rawSttJson={raw} fallbackMd={null} />);
    expect(screen.getByText('1:01:01')).toBeInTheDocument();
  });
});

// ─── Speaker labels ───────────────────────────────────────────────────────────

describe('InteractiveTranscript — speaker labels', () => {
  test('confirmed speaker shows contact display name', () => {
    const speakers = [mkSpeaker('S1', 'Ivan Petrov', true)];
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        speakers={speakers}
      />,
    );
    // First name only from "Ivan Petrov"
    const speakerNodes = document.querySelectorAll('.turn-sp');
    const s1Node = Array.from(speakerNodes).find((n) =>
      n.textContent?.includes('Ivan'),
    );
    expect(s1Node).toBeTruthy();
  });

  test('unconfirmed speaker shows humanSpeakerLabel ("Голос 2") not raw tag', () => {
    // V5.3: humanSpeakerLabel переводит S1 → "Голос 2" (1-based).
    const speakers = [mkSpeaker('S1', 'Marina', false)];
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        speakers={speakers}
      />,
    );
    const speakerNodes = document.querySelectorAll('.turn-sp');
    const golos2Node = Array.from(speakerNodes).find((n) =>
      n.textContent?.includes('Голос 2'),
    );
    expect(golos2Node).toBeTruthy();
    // И НЕ должно быть raw "S1" в speaker labels (теперь humanized).
    const rawTagNode = Array.from(speakerNodes).find(
      (n) => n.textContent?.trim() === 'S1',
    );
    expect(rawTagNode).toBeUndefined();
  });
});

// ─── Seek interaction ─────────────────────────────────────────────────────────

describe('InteractiveTranscript — seek', () => {
  test('row click calls onSeek with segment start', () => {
    const onSeek = vi.fn();
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        onSeek={onSeek}
      />,
    );
    const rows = document.querySelectorAll('.turn');
    fireEvent.click(rows[1]!); // S1 starts at 3
    expect(onSeek).toHaveBeenCalledWith(3);
  });

  test('row has button role when onSeek provided', () => {
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        onSeek={vi.fn()}
      />,
    );
    const rows = document.querySelectorAll('[role="button"]');
    expect(rows.length).toBe(3);
  });

  test('row has no role when onSeek not provided', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    const rows = document.querySelectorAll('[role="button"]');
    expect(rows.length).toBe(0);
  });

  test('Enter key on row triggers onSeek', () => {
    const onSeek = vi.fn();
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        onSeek={onSeek}
      />,
    );
    const rows = document.querySelectorAll('[role="button"]');
    fireEvent.keyDown(rows[0]!, { key: 'Enter' });
    expect(onSeek).toHaveBeenCalledWith(0);
  });

  test('Space key on row triggers onSeek', () => {
    const onSeek = vi.fn();
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        onSeek={onSeek}
      />,
    );
    const rows = document.querySelectorAll('[role="button"]');
    fireEvent.keyDown(rows[0]!, { key: ' ' });
    expect(onSeek).toHaveBeenCalledWith(0);
  });
});

// ─── Active row highlight ─────────────────────────────────────────────────────

describe('InteractiveTranscript — active row', () => {
  test('active row gets .turn--active class for currentTime in range', () => {
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        currentTime={1.5} // within owner 0..3
      />,
    );
    const rows = document.querySelectorAll('.turn') as NodeListOf<HTMLElement>;
    expect(rows[0]!.classList.contains('turn--active')).toBe(true);
  });

  test('no active row when currentTime is undefined', () => {
    render(<InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} />);
    const rows = document.querySelectorAll('.turn') as NodeListOf<HTMLElement>;
    for (const row of rows) {
      expect(row.classList.contains('turn--active')).toBe(false);
    }
  });

  test('no active row when currentTime does not match any segment', () => {
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        currentTime={999}
      />,
    );
    const rows = document.querySelectorAll('.turn') as NodeListOf<HTMLElement>;
    for (const row of rows) {
      expect(row.classList.contains('turn--active')).toBe(false);
    }
  });

  // [B20.9] Смежная граница резолвится в СЛЕДУЮЩУЮ реплику (off-by-one фикс).
  test('shared boundary highlights the next turn, not the previous', () => {
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        currentTime={3} // граница owner(0..3) / S1(3..6) → S1
      />,
    );
    const rows = document.querySelectorAll('.turn') as NodeListOf<HTMLElement>;
    expect(rows[0]!.classList.contains('turn--active')).toBe(false);
    expect(rows[1]!.classList.contains('turn--active')).toBe(true);
  });
});

// ─── [B20.8] Follow-режим ────────────────────────────────────────────────────

describe('InteractiveTranscript — follow mode', () => {
  // setup.ts стабит scrollIntoView на HTMLElement.prototype — спаим его же.
  const spyScroll = () => vi.spyOn(window.HTMLElement.prototype, 'scrollIntoView');

  test('autoscrolls active row when follow=true (default)', () => {
    const spy = spyScroll();
    render(
      <InteractiveTranscript rawSttJson={BASIC_RAW} fallbackMd={null} currentTime={1} />,
    );
    expect(spy).toHaveBeenCalled();
    spy.mockRestore();
  });

  test('does not autoscroll when follow=false', () => {
    const spy = spyScroll();
    render(
      <InteractiveTranscript
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        currentTime={1}
        follow={false}
      />,
    );
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  test('scrollToActive via ref works regardless of follow', () => {
    const spy = spyScroll();
    const ref = { current: null as null | { scrollToActive: () => void } };
    render(
      <InteractiveTranscript
        ref={ref}
        rawSttJson={BASIC_RAW}
        fallbackMd={null}
        currentTime={1}
        follow={false}
      />,
    );
    expect(spy).not.toHaveBeenCalled();
    ref.current!.scrollToActive();
    expect(spy).toHaveBeenCalledOnce();
    spy.mockRestore();
  });
});

// ─── Malformed segments ───────────────────────────────────────────────────────

describe('InteractiveTranscript — malformed data', () => {
  test('filters out segments with missing fields', () => {
    const raw = JSON.stringify({
      version: 1,
      merged: [
        null,
        { start: 0, end: 5 }, // no text or speakerTag
        { start: 0, end: 5, text: 'valid', speakerTag: 'S1' },
      ],
    });
    render(<InteractiveTranscript rawSttJson={raw} fallbackMd={null} />);
    expect(screen.getByText('valid')).toBeInTheDocument();
    const rows = document.querySelectorAll('.turn');
    expect(rows.length).toBe(1);
  });

  test('segment with empty text renders ellipsis placeholder', () => {
    const raw = mkRawStt([
      { start: 0, end: 2, text: '   ', speakerTag: 'S1' },
    ]);
    render(<InteractiveTranscript rawSttJson={raw} fallbackMd={null} />);
    expect(screen.getByText('…')).toBeInTheDocument();
  });
});
