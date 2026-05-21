// [W1] HotkeyCapture — input-like control. Юзер кликает «Записать», нажимает
// комбинацию, парсер выдаёт canonical string. Reserved-список блокирует
// системные shortcut'ы (Cmd+W, Cmd+C, etc).
//
// UI: read-only span с glyph label (⌘⇧R) + кнопка [Записать] / [✓ Готово].

import { useEffect, useRef, useState, type ReactNode } from 'react';

import {
  captureFromEvent,
  formatHotkey,
  isReserved,
  parseHotkey,
  serializeHotkey,
  type ParsedHotkey,
} from '../utils/hotkey';

export interface HotkeyCaptureProps {
  /** Current value as canonical string (`Cmd+Shift+KeyR`). Empty → default fallback. */
  value: string;
  /** Default applied when value is empty / parse fails. */
  defaultHotkey: ParsedHotkey;
  /** Called with new canonical string on user commit. */
  onChange: (canonical: string) => void;
  /** «Записать» button label. */
  captureLabel?: ReactNode;
  /** Disabled state (e.g. during settings load). */
  disabled?: boolean;
}

export function HotkeyCapture({
  value,
  defaultHotkey,
  onChange,
  captureLabel = 'Записать',
  disabled,
}: HotkeyCaptureProps) {
  const parsed = parseHotkey(value) ?? defaultHotkey;
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stopRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!capturing) return;
    const handler = (e: KeyboardEvent) => {
      // Игнорируем bare modifier press (Cmd alone не делает hotkey).
      if (
        e.code === 'MetaLeft' ||
        e.code === 'MetaRight' ||
        e.code === 'ShiftLeft' ||
        e.code === 'ShiftRight' ||
        e.code === 'ControlLeft' ||
        e.code === 'ControlRight' ||
        e.code === 'AltLeft' ||
        e.code === 'AltRight'
      ) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const h = captureFromEvent(e);
      if (!h) {
        setError('Нужен модификатор (⌘/⌃/⌥) или F-клавиша');
        return;
      }
      if (isReserved(h)) {
        setError(`${formatHotkey(h)} зарезервирована OS — выбери другую`);
        return;
      }
      onChange(serializeHotkey(h));
      setCapturing(false);
      setError(null);
    };
    window.addEventListener('keydown', handler, { capture: true });
    stopRef.current = () =>
      window.removeEventListener('keydown', handler, { capture: true });
    return () => stopRef.current?.();
  }, [capturing, onChange]);

  // Esc отмена capture'а
  useEffect(() => {
    if (!capturing) return;
    const escHandler = (e: KeyboardEvent) => {
      if (e.code === 'Escape') {
        setCapturing(false);
        setError(null);
      }
    };
    window.addEventListener('keydown', escHandler);
    return () => window.removeEventListener('keydown', escHandler);
  }, [capturing]);

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
      <span
        className="mono"
        aria-label={capturing ? 'Записываем комбинацию…' : 'Текущая горячая клавиша'}
        style={{
          minWidth: 100,
          padding: '6px 12px',
          background: capturing ? 'var(--accent-soft)' : 'var(--bg-2)',
          color: capturing ? 'var(--accent)' : 'var(--ink)',
          border: `1px solid ${
            capturing ? 'var(--accent)' : 'var(--line)'
          }`,
          borderRadius: 'var(--radius-md)',
          fontSize: 14,
          textAlign: 'center',
          fontVariantNumeric: 'tabular-nums',
        }}
      >
        {capturing ? '…' : formatHotkey(parsed)}
      </span>
      <button
        type="button"
        className={`btn btn--sm ${capturing ? 'btn--quiet' : 'btn--ghost'}`}
        onClick={() => {
          setError(null);
          setCapturing((v) => !v);
        }}
        disabled={disabled}
      >
        {capturing ? 'Отмена (Esc)' : captureLabel}
      </button>
      {error && (
        <span
          role="alert"
          style={{
            fontSize: 12,
            color: 'var(--signal)',
            fontStyle: 'italic',
          }}
        >
          {error}
        </span>
      )}
    </div>
  );
}
