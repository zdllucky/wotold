import { describe, expect, test } from 'vitest';

import {
  captureFromEvent,
  DEFAULT_PAUSE_HOTKEY,
  DEFAULT_TOGGLE_HOTKEY,
  formatHotkey,
  isReserved,
  matchEvent,
  parseHotkey,
  serializeHotkey,
} from './hotkey';

describe('parseHotkey', () => {
  test('parses canonical Cmd+Shift+KeyR', () => {
    const h = parseHotkey('Cmd+Shift+KeyR');
    expect(h).toEqual({
      meta: true,
      ctrl: false,
      alt: false,
      shift: true,
      code: 'KeyR',
    });
  });

  test('case-insensitive modifier names', () => {
    expect(parseHotkey('cmd+shift+keyr')?.code).toBe('KeyR');
    expect(parseHotkey('META+SHIFT+KeyR')?.code).toBe('KeyR');
  });

  test('accepts glyph aliases', () => {
    const h = parseHotkey('⌘+⇧+KeyR');
    expect(h?.meta).toBe(true);
    expect(h?.shift).toBe(true);
  });

  test('returns null for empty / nullish', () => {
    expect(parseHotkey('')).toBeNull();
    expect(parseHotkey(null)).toBeNull();
    expect(parseHotkey(undefined)).toBeNull();
  });

  test('returns null when code is not whitelisted', () => {
    expect(parseHotkey('Cmd+Shift+NotAKey')).toBeNull();
    expect(parseHotkey('Cmd+Shift')).toBeNull(); // no code
  });
});

describe('serialize ↔ parse roundtrip', () => {
  test('Cmd+Shift+KeyR → string → ParsedHotkey identical', () => {
    const s = serializeHotkey(DEFAULT_TOGGLE_HOTKEY);
    expect(s).toBe('Cmd+Shift+KeyR');
    expect(parseHotkey(s)).toEqual(DEFAULT_TOGGLE_HOTKEY);
  });

  test('Cmd+Shift+KeyP roundtrip', () => {
    const s = serializeHotkey(DEFAULT_PAUSE_HOTKEY);
    expect(parseHotkey(s)).toEqual(DEFAULT_PAUSE_HOTKEY);
  });
});

describe('formatHotkey', () => {
  test('cmd+shift+R → ⇧⌘R', () => {
    expect(formatHotkey(DEFAULT_TOGGLE_HOTKEY)).toBe('⇧⌘R');
  });

  test('ctrl+alt+P → ⌃⌥P', () => {
    expect(
      formatHotkey({
        meta: false,
        ctrl: true,
        alt: true,
        shift: false,
        code: 'KeyP',
      }),
    ).toBe('⌃⌥P');
  });

  test('special keys get glyphs', () => {
    expect(
      formatHotkey({
        meta: true,
        ctrl: false,
        alt: false,
        shift: false,
        code: 'ArrowUp',
      }),
    ).toBe('⌘↑');
    expect(
      formatHotkey({
        meta: false,
        ctrl: false,
        alt: false,
        shift: false,
        code: 'Escape',
      }),
    ).toBe('⎋');
  });

  test('null → empty string', () => {
    expect(formatHotkey(null)).toBe('');
  });
});

describe('matchEvent', () => {
  function mkEvent(props: Partial<KeyboardEvent>): KeyboardEvent {
    return {
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      code: '',
      ...props,
    } as KeyboardEvent;
  }

  test('matches when all modifiers + code align', () => {
    const e = mkEvent({ metaKey: true, shiftKey: true, code: 'KeyR' });
    expect(matchEvent(e, DEFAULT_TOGGLE_HOTKEY)).toBe(true);
  });

  test('uses e.code so ru-layout still works (KeyR for «к»)', () => {
    // На ru-layout юзер жмёт ту же физическую клавишу — code === 'KeyR'.
    const e = mkEvent({ metaKey: true, shiftKey: true, code: 'KeyR' });
    expect(matchEvent(e, DEFAULT_TOGGLE_HOTKEY)).toBe(true);
  });

  test('rejects when wrong code', () => {
    const e = mkEvent({ metaKey: true, shiftKey: true, code: 'KeyS' });
    expect(matchEvent(e, DEFAULT_TOGGLE_HOTKEY)).toBe(false);
  });

  test('rejects when extra modifier present', () => {
    const e = mkEvent({
      metaKey: true,
      shiftKey: true,
      altKey: true,
      code: 'KeyR',
    });
    expect(matchEvent(e, DEFAULT_TOGGLE_HOTKEY)).toBe(false);
  });

  test('null hotkey never matches', () => {
    const e = mkEvent({ metaKey: true, code: 'KeyR' });
    expect(matchEvent(e, null)).toBe(false);
  });
});

describe('captureFromEvent', () => {
  function mkEvent(props: Partial<KeyboardEvent>): KeyboardEvent {
    return {
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      code: '',
      ...props,
    } as KeyboardEvent;
  }

  test('captures Cmd+Shift+R', () => {
    const e = mkEvent({ metaKey: true, shiftKey: true, code: 'KeyR' });
    expect(captureFromEvent(e)).toEqual({
      meta: true,
      ctrl: false,
      alt: false,
      shift: true,
      code: 'KeyR',
    });
  });

  test('rejects bare letter (no modifier) — typing conflict', () => {
    const e = mkEvent({ code: 'KeyR' });
    expect(captureFromEvent(e)).toBeNull();
  });

  test('F-keys allowed bare', () => {
    const e = mkEvent({ code: 'F5' });
    expect(captureFromEvent(e)?.code).toBe('F5');
  });

  test('Escape allowed bare', () => {
    const e = mkEvent({ code: 'Escape' });
    expect(captureFromEvent(e)?.code).toBe('Escape');
  });

  test('rejects non-whitelisted code', () => {
    const e = mkEvent({ metaKey: true, code: 'ContextMenu' });
    expect(captureFromEvent(e)).toBeNull();
  });
});

describe('isReserved', () => {
  test('blocks Cmd+W (close window)', () => {
    expect(
      isReserved({
        meta: true,
        ctrl: false,
        alt: false,
        shift: false,
        code: 'KeyW',
      }),
    ).toBe(true);
  });

  test('blocks Cmd+C (copy)', () => {
    expect(
      isReserved({
        meta: true,
        ctrl: false,
        alt: false,
        shift: false,
        code: 'KeyC',
      }),
    ).toBe(true);
  });

  test('Cmd+Shift+R is NOT reserved (our default)', () => {
    expect(isReserved(DEFAULT_TOGGLE_HOTKEY)).toBe(false);
  });

  test('Cmd+Shift+W is allowed (shift modifier breaks system shortcut)', () => {
    expect(
      isReserved({
        meta: true,
        ctrl: false,
        alt: false,
        shift: true,
        code: 'KeyW',
      }),
    ).toBe(false);
  });
});
