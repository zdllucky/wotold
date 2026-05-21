// [B17 V3.7] Custom Select per Atelier v2 — заменяет native <select>.
//
// Trigger выглядит как .input--box. Dropdown — floating panel над/под
// trigger, paper bg + line border + shadow-2. Selected опция — accent
// stripe слева + bold. Hover — bg-2.
//
// Keyboard:
//   - Space/Enter/Down arrow — открыть
//   - Up/Down — навигация highlighted
//   - Enter — select highlighted
//   - Esc — закрыть, focus trigger
//   - Tab — закрыть + переход
//   - Letter typing — jump first match
//
// A11y: combobox + listbox roles, aria-activedescendant, aria-expanded.
// Click outside через mousedown listener.

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from 'react';

export interface SelectOption<V extends string = string> {
  value: V;
  label: ReactNode;
  /** Опциональный hint справа от label. */
  hint?: ReactNode;
  /** Тег для keyboard-typeahead (если label — ReactNode). По умолчанию = stringified label. */
  searchText?: string;
  disabled?: boolean;
}

interface SelectProps<V extends string = string> {
  value: V;
  options: ReadonlyArray<SelectOption<V>>;
  onChange: (v: V) => void;
  /** Когда нет выбранного значения, показывается placeholder text. */
  placeholder?: string;
  disabled?: boolean;
  /** Передаётся в input id для label htmlFor binding. */
  id?: string;
  /** Width override. По умолчанию 100%. */
  width?: number | string;
  className?: string;
  style?: CSSProperties;
  ariaLabel?: string;
}

export function Select<V extends string = string>({
  value,
  options,
  onChange,
  placeholder = '— не выбран —',
  disabled = false,
  id,
  width = '100%',
  className,
  style,
  ariaLabel,
}: SelectProps<V>) {
  const generatedId = useId();
  const triggerId = id ?? `select-${generatedId}`;
  const listboxId = `${triggerId}-listbox`;

  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState<number>(() =>
    Math.max(0, options.findIndex((o) => o.value === value)),
  );
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const typeaheadRef = useRef<{ buf: string; t: number }>({ buf: '', t: 0 });

  const selected = useMemo(
    () => options.find((o) => o.value === value) ?? null,
    [options, value],
  );

  // Reset highlight to selected when opening.
  useEffect(() => {
    if (open) {
      const idx = options.findIndex((o) => o.value === value);
      setHighlight(idx >= 0 ? idx : 0);
    }
  }, [open, options, value]);

  // Outside click closes.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (triggerRef.current?.contains(target)) return;
      if (panelRef.current?.contains(target)) return;
      setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  // Scroll highlighted option into view.
  useEffect(() => {
    if (!open) return;
    const panel = panelRef.current;
    if (!panel) return;
    const el = panel.querySelector<HTMLElement>(`[data-idx="${highlight}"]`);
    if (el) {
      el.scrollIntoView({ block: 'nearest' });
    }
  }, [highlight, open]);

  const commit = useCallback(
    (idx: number) => {
      const opt = options[idx];
      if (!opt || opt.disabled) return;
      onChange(opt.value);
      setOpen(false);
      // Restore focus to trigger after select.
      requestAnimationFrame(() => triggerRef.current?.focus());
    },
    [options, onChange],
  );

  const moveHighlight = useCallback(
    (dir: 1 | -1) => {
      setHighlight((prev) => {
        let next = prev;
        for (let i = 0; i < options.length; i++) {
          next = (next + dir + options.length) % options.length;
          if (!options[next]?.disabled) return next;
        }
        return prev;
      });
    },
    [options],
  );

  const handleTypeahead = useCallback(
    (key: string) => {
      const now = Date.now();
      const state = typeaheadRef.current;
      if (now - state.t > 600) state.buf = '';
      state.buf += key.toLowerCase();
      state.t = now;
      const buf = state.buf;
      const startIdx = (highlight + 1) % options.length;
      for (let off = 0; off < options.length; off++) {
        const idx = (startIdx + off) % options.length;
        const opt = options[idx];
        if (!opt || opt.disabled) continue;
        const text = (opt.searchText ?? String(opt.label ?? '')).toLowerCase();
        if (text.startsWith(buf)) {
          setHighlight(idx);
          return;
        }
      }
    },
    [options, highlight],
  );

  const onTriggerKey = (e: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (disabled) return;
    if (!open) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
        e.preventDefault();
        setOpen(true);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      commit(highlight);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      moveHighlight(1);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      moveHighlight(-1);
      return;
    }
    if (e.key === 'Home') {
      e.preventDefault();
      setHighlight(options.findIndex((o) => !o.disabled));
      return;
    }
    if (e.key === 'End') {
      e.preventDefault();
      for (let i = options.length - 1; i >= 0; i--) {
        if (!options[i]?.disabled) {
          setHighlight(i);
          break;
        }
      }
      return;
    }
    if (e.key === 'Tab') {
      setOpen(false);
      return;
    }
    if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      handleTypeahead(e.key);
    }
  };

  const triggerStyle: CSSProperties = {
    width,
    border: '1px solid var(--line-strong)',
    borderRadius: 'var(--radius-sm)',
    padding: '10px 36px 10px 12px',
    fontFamily: 'var(--font-sans)',
    fontSize: 14,
    background: 'var(--surface)',
    color: 'var(--ink)',
    cursor: disabled ? 'not-allowed' : 'pointer',
    opacity: disabled ? 0.5 : 1,
    textAlign: 'left',
    position: 'relative',
    outline: 'none',
    letterSpacing: '-0.005em',
    transition: 'border-color var(--duration-fast)',
    ...style,
  };

  const triggerOpenStyle: CSSProperties = open
    ? { borderColor: 'var(--accent)' }
    : {};

  return (
    <div
      className={className}
      style={{ position: 'relative', width: width === '100%' ? '100%' : 'auto' }}
    >
      <button
        ref={triggerRef}
        id={triggerId}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-activedescendant={
          open ? `${listboxId}-opt-${highlight}` : undefined
        }
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => !disabled && setOpen((o) => !o)}
        onKeyDown={onTriggerKey}
        style={{ ...triggerStyle, ...triggerOpenStyle }}
      >
        <span
          style={{
            display: 'block',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            color: selected ? 'var(--ink)' : 'var(--subtle)',
          }}
        >
          {selected ? selected.label : placeholder}
        </span>
        <Caret open={open} />
      </button>

      {open && (
        <div
          ref={panelRef}
          id={listboxId}
          role="listbox"
          aria-labelledby={triggerId}
          style={{
            position: 'absolute',
            top: 'calc(100% + 6px)',
            left: 0,
            right: 0,
            zIndex: 30,
            background: 'var(--paper)',
            border: '1px solid var(--line)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-2)',
            padding: 4,
            maxHeight: 280,
            overflowY: 'auto',
            outline: 'none',
            // Edit mode focus ring через :focus-within ловится по CSS — но
            // здесь focus остаётся на trigger, panel сама не фокусится.
          }}
        >
          {options.map((opt, idx) => {
            const isSelected = opt.value === value;
            const isHighlight = idx === highlight;
            return (
              <div
                key={String(opt.value) || `idx-${idx}`}
                id={`${listboxId}-opt-${idx}`}
                role="option"
                aria-selected={isSelected}
                aria-disabled={opt.disabled || undefined}
                data-idx={idx}
                onMouseDown={(e) => {
                  if (opt.disabled) {
                    e.preventDefault();
                    return;
                  }
                  e.preventDefault();
                  commit(idx);
                }}
                onMouseEnter={() => !opt.disabled && setHighlight(idx)}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '4px 1fr auto',
                  gap: 8,
                  alignItems: 'center',
                  padding: '8px 10px 8px 6px',
                  borderRadius: 'var(--radius-sm)',
                  cursor: opt.disabled ? 'not-allowed' : 'pointer',
                  background: isHighlight ? 'var(--bg-2)' : 'transparent',
                  color: opt.disabled ? 'var(--subtle)' : 'var(--ink)',
                  fontSize: 13.5,
                  fontFamily: 'var(--font-sans)',
                  letterSpacing: '-0.005em',
                  fontWeight: isSelected ? 600 : 500,
                  transition: 'background var(--duration-fast)',
                }}
              >
                {/* Accent stripe for selected — column 1. */}
                <span
                  aria-hidden
                  style={{
                    width: 3,
                    height: 14,
                    borderRadius: 2,
                    background: isSelected
                      ? 'var(--accent)'
                      : 'transparent',
                  }}
                />
                <span
                  style={{
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {opt.label}
                </span>
                {opt.hint && (
                  <span
                    className="muted mono"
                    style={{
                      fontSize: 10.5,
                      letterSpacing: '0.04em',
                      flexShrink: 0,
                    }}
                  >
                    {opt.hint}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Caret({ open }: { open: boolean }) {
  return (
    <svg
      aria-hidden
      width="10"
      height="10"
      viewBox="0 0 10 10"
      style={{
        position: 'absolute',
        right: 12,
        top: '50%',
        transform: `translateY(-50%) rotate(${open ? 180 : 0}deg)`,
        transition: 'transform var(--duration-fast) var(--ease-out-expo)',
        color: 'var(--muted)',
        pointerEvents: 'none',
      }}
    >
      <path
        d="M1.5 3.5L5 7L8.5 3.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
