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
  /** [V5.3] Вторая строка под label (мелкая, muted). Например org / role
   *  для контакта. Учитывается в search query (если оно поднято). */
  description?: ReactNode;
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
  /** [V5.3] Visible search input в верхушке dropdown'а. Фильтрует по
   *  `searchText` (или stringified label) + по `description`. */
  searchable?: boolean;
  /** Placeholder для search input'а (если searchable=true). */
  searchPlaceholder?: string;
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
  searchable = false,
  searchPlaceholder = 'Поиск…',
}: SelectProps<V>) {
  const generatedId = useId();
  const triggerId = id ?? `select-${generatedId}`;
  const listboxId = `${triggerId}-listbox`;
  const searchInputId = `${triggerId}-search`;

  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState<number>(0);
  const [query, setQuery] = useState('');
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const typeaheadRef = useRef<{ buf: string; t: number }>({ buf: '', t: 0 });

  // [V5.3] Filtered options если searchable + query. Поиск по
  // searchText/label + description (case-insensitive). Пустой query = все.
  const visibleOptions = useMemo(() => {
    if (!searchable || !query.trim()) return options;
    const q = query.trim().toLowerCase();
    return options.filter((opt) => {
      const haystack = [
        opt.searchText,
        typeof opt.label === 'string' ? opt.label : '',
        typeof opt.description === 'string' ? opt.description : '',
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase();
      return haystack.includes(q);
    });
  }, [options, query, searchable]);

  const selected = useMemo(
    () => options.find((o) => o.value === value) ?? null,
    [options, value],
  );

  // Reset highlight to selected (in visibleOptions) when opening / query
  // changes.
  useEffect(() => {
    if (open) {
      const idx = visibleOptions.findIndex((o) => o.value === value);
      setHighlight(idx >= 0 ? idx : 0);
    }
  }, [open, visibleOptions, value]);

  // [V5.3] Reset highlight to 0 при каждом изменении query (текущий highlight
  // мог выйти за пределы visibleOptions).
  useEffect(() => {
    setHighlight(0);
  }, [query]);

  // [V5.3] Reset query когда закрываем (иначе при следующем open покажется
  // прошлый поиск).
  useEffect(() => {
    if (!open) setQuery('');
  }, [open]);

  // [V5.3] Focus search input при открытии (если searchable).
  useEffect(() => {
    if (open && searchable) {
      requestAnimationFrame(() => searchInputRef.current?.focus());
    }
  }, [open, searchable]);

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
      const opt = visibleOptions[idx];
      if (!opt || opt.disabled) return;
      onChange(opt.value);
      setOpen(false);
      // Restore focus to trigger after select.
      requestAnimationFrame(() => triggerRef.current?.focus());
    },
    [visibleOptions, onChange],
  );

  const moveHighlight = useCallback(
    (dir: 1 | -1) => {
      setHighlight((prev) => {
        if (visibleOptions.length === 0) return 0;
        let next = prev;
        for (let i = 0; i < visibleOptions.length; i++) {
          next = (next + dir + visibleOptions.length) % visibleOptions.length;
          if (!visibleOptions[next]?.disabled) return next;
        }
        return prev;
      });
    },
    [visibleOptions],
  );

  const handleTypeahead = useCallback(
    (key: string) => {
      const now = Date.now();
      const state = typeaheadRef.current;
      if (now - state.t > 600) state.buf = '';
      state.buf += key.toLowerCase();
      state.t = now;
      const buf = state.buf;
      if (visibleOptions.length === 0) return;
      const startIdx = (highlight + 1) % visibleOptions.length;
      for (let off = 0; off < visibleOptions.length; off++) {
        const idx = (startIdx + off) % visibleOptions.length;
        const opt = visibleOptions[idx];
        if (!opt || opt.disabled) continue;
        const text = (opt.searchText ?? String(opt.label ?? '')).toLowerCase();
        if (text.startsWith(buf)) {
          setHighlight(idx);
          return;
        }
      }
    },
    [visibleOptions, highlight],
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
      setHighlight(visibleOptions.findIndex((o) => !o.disabled));
      return;
    }
    if (e.key === 'End') {
      e.preventDefault();
      for (let i = visibleOptions.length - 1; i >= 0; i--) {
        if (!visibleOptions[i]?.disabled) {
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
    // Typeahead отключён когда searchable=true (search input ловит letters).
    if (!searchable && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      handleTypeahead(e.key);
    }
  };

  // [V5.3] Keyboard handling для search input — ↑↓ навигация + Enter commit,
  // буквы остаются в input для фильтра. Esc закрывает.
  const onSearchKey = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
      triggerRef.current?.focus();
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
    // Прочее (буквы) — нативный input обрабатывает.
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
            maxHeight: 340,
            overflowY: 'auto',
            outline: 'none',
          }}
        >
          {searchable && (
            <div
              style={{
                padding: '6px 6px 8px',
                borderBottom: '1px solid var(--line-soft)',
                marginBottom: 4,
                position: 'sticky',
                top: 0,
                background: 'var(--paper)',
              }}
            >
              <input
                ref={searchInputRef}
                id={searchInputId}
                type="text"
                role="searchbox"
                aria-label={searchPlaceholder}
                aria-controls={listboxId}
                placeholder={searchPlaceholder}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={onSearchKey}
                style={{
                  width: '100%',
                  padding: '6px 10px',
                  border: '1px solid var(--line-soft)',
                  borderRadius: 'var(--radius-sm)',
                  background: 'var(--surface)',
                  color: 'var(--ink)',
                  fontFamily: 'var(--font-sans)',
                  fontSize: 13,
                  outline: 'none',
                }}
              />
            </div>
          )}
          {visibleOptions.length === 0 && (
            <div
              style={{
                padding: '12px 14px',
                fontSize: 13,
                color: 'var(--subtle)',
                fontFamily: 'var(--font-sans)',
                fontStyle: 'italic',
              }}
            >
              Ничего не найдено
            </div>
          )}
          {visibleOptions.map((opt, idx) => {
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
                    height: opt.description ? 28 : 14,
                    borderRadius: 2,
                    background: isSelected ? 'var(--accent)' : 'transparent',
                    alignSelf: opt.description ? 'flex-start' : 'center',
                    marginTop: opt.description ? 4 : 0,
                  }}
                />
                <span
                  style={{
                    overflow: 'hidden',
                    minWidth: 0,
                    display: 'flex',
                    flexDirection: 'column',
                    gap: 2,
                  }}
                >
                  <span
                    style={{
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {opt.label}
                  </span>
                  {opt.description && (
                    <span
                      className="muted"
                      style={{
                        fontSize: 11.5,
                        fontWeight: 400,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {opt.description}
                    </span>
                  )}
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
