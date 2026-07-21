// [W1, B21] HotkeyCapture — input-like control. Юзер кликает «Изменить»,
// нажимает комбинацию, парсер выдаёт canonical string. Reserved-список
// блокирует системные shortcut'ы (Cmd+W, Cmd+C, etc).
//
// UI (канон wk-settings.jsx HotkeyCapture): .hotkey-readout mono + sm-кнопка
// «Изменить»/«Esc», ошибка — danger-текст слева. Все строки через i18n.

import { useEffect, useRef, useState } from 'react';

import {
  captureFromEvent,
  formatHotkey,
  isReserved,
  parseHotkey,
  serializeHotkey,
  type ParsedHotkey,
} from '../utils/hotkey';
import { useI18n } from '../i18n';
import { Button } from '../ui';

export interface HotkeyCaptureProps {
  /** Current value as canonical string (`Cmd+Shift+KeyR`). Empty → default fallback. */
  value: string;
  /** Default applied when value is empty / parse fails. */
  defaultHotkey: ParsedHotkey;
  /** Called with new canonical string on user commit. */
  onChange: (canonical: string) => void;
  /** Disabled state (e.g. during settings load). */
  disabled?: boolean;
}

export function HotkeyCapture({ value, defaultHotkey, onChange, disabled }: HotkeyCaptureProps) {
  const { t } = useI18n();
  const parsed = parseHotkey(value) ?? defaultHotkey;
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stopRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!capturing) return;
    const handler = (e: KeyboardEvent) => {
      // [B21] Esc = отмена capture'а. Раньше capture-phase handler перехватывал
      // Escape раньше bubble-листенера отмены и мог НАЗНАЧИТЬ голый Esc хоткеем.
      if (e.code === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setCapturing(false);
        setError(null);
        return;
      }
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
        setError(t('settings.hotkeyNeedModifier'));
        return;
      }
      if (isReserved(h)) {
        setError(t('settings.hotkeyReserved', { combo: formatHotkey(h) }));
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
  }, [capturing, onChange, t]);

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
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {error && (
        <span
          role="alert"
          style={{ fontSize: 11.5, color: 'var(--danger-text)', fontStyle: 'italic' }}
        >
          {error}
        </span>
      )}
      <span
        className="hotkey-readout mono"
        aria-label={
          capturing ? t('settings.hotkeyCapturingAria') : t('settings.hotkeyCurrentAria')
        }
        style={
          capturing
            ? {
                background: 'var(--accent-soft)',
                color: 'var(--accent-text)',
                borderColor: 'var(--accent)',
              }
            : undefined
        }
      >
        {capturing ? '…' : formatHotkey(parsed)}
      </span>
      <Button
        variant={capturing ? 'ghost' : 'default'}
        size="sm"
        onClick={() => {
          setError(null);
          setCapturing((v) => !v);
        }}
        disabled={disabled}
      >
        {capturing ? t('settings.hotkeyCancel') : t('settings.hotkeyChange')}
      </Button>
    </div>
  );
}
