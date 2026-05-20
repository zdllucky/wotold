// [B17] WCAG 2.2 modal focus trap.
//
// Per motion-ui skill (Modal Essentials):
//   - Trap Tab cycling inside the modal root
//   - Esc closes (caller passes onClose)
//   - On open: focus first focusable; on close: restore previously focused element
//   - Optional scroll-lock на body пока модал открыт
//
// Использование:
//   const ref = useRef<HTMLDivElement>(null);
//   useFocusTrap(ref, isOpen, onClose);
//   return isOpen ? <div ref={ref} role="dialog" aria-modal>…</div> : null;

import { useEffect, type RefObject } from 'react';

const FOCUSABLE_SELECTOR =
  'a[href], area[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), iframe, object, embed, [tabindex]:not([tabindex="-1"])';

export interface FocusTrapOptions {
  /** Закрыть по Esc. Передай undefined чтобы не реагировать на ESC. */
  onClose?: () => void;
  /** Лочить scroll <body> пока модал открыт. По умолчанию true. */
  lockScroll?: boolean;
  /** Восстановить фокус на elемент, который был активен до открытия. По умолчанию true. */
  restoreFocus?: boolean;
}

export function useFocusTrap(
  ref: RefObject<HTMLElement | null>,
  active: boolean,
  options: FocusTrapOptions = {},
): void {
  const { onClose, lockScroll = true, restoreFocus = true } = options;

  useEffect(() => {
    if (!active || !ref.current) return;

    const el = ref.current;
    const previouslyFocused = document.activeElement as HTMLElement | null;

    // Initial focus — первый focusable элемент внутри.
    const initialFocusables = el.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
    if (initialFocusables.length > 0) {
      initialFocusables[0]?.focus();
    } else {
      // Если внутри нет focusable — фокусируем сам root (tabIndex=-1 не нужен,
      // но focus() сработает только если у него есть tabindex). Skip.
    }

    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        if (onClose) {
          e.preventDefault();
          onClose();
        }
        return;
      }
      if (e.key !== 'Tab') return;

      // Пересчитываем focusables каждый раз — DOM может меняться.
      const focusables = el.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusables.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const activeEl = document.activeElement as HTMLElement | null;

      if (e.shiftKey && activeEl === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && activeEl === last) {
        e.preventDefault();
        first?.focus();
      }
    }

    el.addEventListener('keydown', handleKey);

    let previousOverflow = '';
    if (lockScroll) {
      previousOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
    }

    return () => {
      el.removeEventListener('keydown', handleKey);
      if (lockScroll) {
        document.body.style.overflow = previousOverflow;
      }
      if (restoreFocus && previouslyFocused && typeof previouslyFocused.focus === 'function') {
        // Восстанавливаем фокус только если кто-то ещё не его перехватил.
        if (document.activeElement === document.body || el.contains(document.activeElement)) {
          previouslyFocused.focus();
        }
      }
    };
  }, [active, ref, onClose, lockScroll, restoreFocus]);
}
