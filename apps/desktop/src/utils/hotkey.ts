// [W1] Hotkey parser / formatter / matcher.
//
// Раньше ⌘⇧R было захардкожено в HomePage (включая cyrillic alt 'к' для
// ru-layout). Теперь юзер может переопределить toggle и pause hotkey'и в
// Settings → Запись. Этот модуль — single source of truth для:
//   - canonical string format (e.g. 'Cmd+Shift+R') для persistence в DB
//   - human glyph string (⌘⇧R) для UI label
//   - match'а KeyboardEvent
//   - capture из live KeyboardEvent (для UI «нажмите комбинацию»)
//
// Конструктивно: используем `e.code` (layout-independent) когда возможно,
// fallback на `e.key.toLowerCase()` для редких клавиш без code mapping.

export interface ParsedHotkey {
  /** Cmd / Win key (`e.metaKey`). На macOS ⌘, на Windows ⊞. */
  meta: boolean;
  /** Ctrl (`e.ctrlKey`). */
  ctrl: boolean;
  /** Alt / Option (`e.altKey`). */
  alt: boolean;
  /** Shift (`e.shiftKey`). */
  shift: boolean;
  /** Canonical key id: `KeyR` (codeword) или `Tab` / `Escape` / `F1`-`F12` / `Space` / etc. */
  code: string;
}

/** Whitelist клавиш которые имеют смысл как hotkey. */
export const ALLOWED_KEYS = new Set<string>([
  ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').map((c) => `Key${c}`),
  ...'0123456789'.split('').map((d) => `Digit${d}`),
  'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12',
  'Space', 'Tab', 'Enter', 'Escape', 'Backspace',
  'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight',
  'Comma', 'Period', 'Slash', 'Semicolon', 'Quote', 'Backslash',
  'BracketLeft', 'BracketRight', 'Minus', 'Equal',
]);

/** Системные комбинации которые нельзя override'ить — отдадим OS. */
const RESERVED: Array<Pick<ParsedHotkey, 'meta' | 'ctrl' | 'alt' | 'shift' | 'code'>> = [
  // macOS system
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyW' }, // close
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyQ' }, // quit
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyM' }, // minimize
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyH' }, // hide
  { meta: true, ctrl: false, alt: false, shift: false, code: 'Tab' }, // app switcher
  { meta: true, ctrl: false, alt: false, shift: false, code: 'Space' }, // spotlight
  // Common editor shortcuts — не дадим override чтобы copy/paste работало
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyC' },
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyV' },
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyX' },
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyA' },
  { meta: true, ctrl: false, alt: false, shift: false, code: 'KeyZ' },
];

/** Case-insensitive lookup для normalize'а `keyr` → `KeyR`. */
const CODE_NORMALIZE: Map<string, string> = (() => {
  const m = new Map<string, string>();
  for (const k of ALLOWED_KEYS) m.set(k.toLowerCase(), k);
  return m;
})();

/**
 * Parse canonical string `Cmd+Shift+KeyR` → ParsedHotkey.
 * Возвращает null если строка невалидна. Используется при load'е из БД.
 * Case-insensitive для модификаторов И code'а.
 */
export function parseHotkey(s: string | null | undefined): ParsedHotkey | null {
  if (!s) return null;
  const parts = s.split('+').map((p) => p.trim());
  if (parts.length === 0) return null;

  const result: ParsedHotkey = {
    meta: false,
    ctrl: false,
    alt: false,
    shift: false,
    code: '',
  };
  for (const part of parts) {
    const lower = part.toLowerCase();
    if (lower === 'cmd' || lower === 'meta' || lower === 'super' || lower === '⌘') {
      result.meta = true;
    } else if (lower === 'ctrl' || lower === 'control') {
      result.ctrl = true;
    } else if (lower === 'alt' || lower === 'option' || lower === '⌥') {
      result.alt = true;
    } else if (lower === 'shift' || lower === '⇧') {
      result.shift = true;
    } else {
      // Normalize code к canonical case (KeyR, ArrowUp, F5).
      const normalized = CODE_NORMALIZE.get(lower);
      if (!normalized) return null;
      result.code = normalized;
    }
  }
  if (!result.code) return null;
  return result;
}

/** Сериализация для DB: `Cmd+Shift+KeyR`. Stable порядок модификаторов. */
export function serializeHotkey(h: ParsedHotkey): string {
  const parts: string[] = [];
  if (h.meta) parts.push('Cmd');
  if (h.ctrl) parts.push('Ctrl');
  if (h.alt) parts.push('Alt');
  if (h.shift) parts.push('Shift');
  parts.push(h.code);
  return parts.join('+');
}

/** UI label: `⌘⇧R`. Используем glyphs для краткости. */
export function formatHotkey(h: ParsedHotkey | null): string {
  if (!h) return '';
  const parts: string[] = [];
  if (h.ctrl) parts.push('⌃');
  if (h.alt) parts.push('⌥');
  if (h.shift) parts.push('⇧');
  if (h.meta) parts.push('⌘');
  parts.push(prettyCode(h.code));
  return parts.join('');
}

function prettyCode(code: string): string {
  // KeyR → R, Digit5 → 5, ArrowUp → ↑
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  const map: Record<string, string> = {
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Space: '␣',
    Tab: '⇥',
    Enter: '↵',
    Escape: '⎋',
    Backspace: '⌫',
    Comma: ',',
    Period: '.',
    Slash: '/',
    Semicolon: ';',
    Quote: "'",
    Backslash: '\\',
    BracketLeft: '[',
    BracketRight: ']',
    Minus: '-',
    Equal: '=',
  };
  return map[code] ?? code;
}

/** True если KeyboardEvent матчит hotkey. Использует `e.code` — layout-independent
 *  (ru-keyboard 'к' тоже даст `code === 'KeyR'`). */
export function matchEvent(e: KeyboardEvent, h: ParsedHotkey | null): boolean {
  if (!h) return false;
  return (
    e.metaKey === h.meta &&
    e.ctrlKey === h.ctrl &&
    e.altKey === h.alt &&
    e.shiftKey === h.shift &&
    e.code === h.code
  );
}

/** Из live KeyboardEvent → ParsedHotkey, для UI «нажмите комбинацию».
 *  Возвращает null если код не whitelist'нут или комбинация без модификатора
 *  (одинокая буква не годится — конфликт с typing). */
export function captureFromEvent(e: KeyboardEvent): ParsedHotkey | null {
  if (!ALLOWED_KEYS.has(e.code)) return null;
  const h: ParsedHotkey = {
    meta: e.metaKey,
    ctrl: e.ctrlKey,
    alt: e.altKey,
    shift: e.shiftKey,
    code: e.code,
  };
  // F-keys и Esc допустимы без модификаторов (как в большинстве систем).
  const isFunctionKey = /^F\d+$/.test(h.code) || h.code === 'Escape';
  const hasModifier = h.meta || h.ctrl || h.alt;
  if (!hasModifier && !isFunctionKey) return null;
  return h;
}

/** Reserved-список (cmd+W, cmd+Q, cmd+C, etc) — UI block'ит установку этих. */
export function isReserved(h: ParsedHotkey): boolean {
  return RESERVED.some(
    (r) =>
      r.meta === h.meta &&
      r.ctrl === h.ctrl &&
      r.alt === h.alt &&
      r.shift === h.shift &&
      r.code === h.code,
  );
}

/** Default values. */
export const DEFAULT_TOGGLE_HOTKEY: ParsedHotkey = {
  meta: true,
  ctrl: false,
  alt: false,
  shift: true,
  code: 'KeyR',
};

export const DEFAULT_PAUSE_HOTKEY: ParsedHotkey = {
  meta: true,
  ctrl: false,
  alt: false,
  shift: true,
  code: 'KeyP',
};
