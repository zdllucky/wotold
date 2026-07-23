// [B27.5] Tooltip — портальная подсказка вместо CSS `.tip::after`.
// Тот ломался на краях: жёсткое центрирование + white-space:nowrap + клиппинг
// overflow-контейнеров («уть список чатов»). Здесь: portal в body,
// position:fixed от rect триггера, clamp к viewport, флип top<->bottom.
// Показ: hover с задержкой / focus сразу; скрытие: leave / blur / Esc / click.

import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';

export type TooltipSide = 'top' | 'bottom' | 'right';

interface TooltipProps {
  content: string;
  side?: TooltipSide;
  delayMs?: number;
  children: ReactNode;
}

const VIEWPORT_PAD = 8;
const GAP = 6;

function place(
  side: TooltipSide,
  trigger: DOMRect,
  tip: { width: number; height: number },
): { left: number; top: number } {
  let left: number;
  let top: number;
  if (side === 'right') {
    left = trigger.right + GAP;
    top = trigger.top + trigger.height / 2 - tip.height / 2;
    if (left + tip.width > window.innerWidth - VIEWPORT_PAD) {
      left = trigger.left - GAP - tip.width; // флип влево
    }
  } else {
    left = trigger.left + trigger.width / 2 - tip.width / 2;
    top = side === 'top' ? trigger.top - GAP - tip.height : trigger.bottom + GAP;
    if (side === 'top' && top < VIEWPORT_PAD) top = trigger.bottom + GAP; // флип вниз
    if (side === 'bottom' && top + tip.height > window.innerHeight - VIEWPORT_PAD) {
      top = trigger.top - GAP - tip.height; // флип вверх
    }
  }
  left = Math.min(Math.max(left, VIEWPORT_PAD), window.innerWidth - VIEWPORT_PAD - tip.width);
  top = Math.min(Math.max(top, VIEWPORT_PAD), window.innerHeight - VIEWPORT_PAD - tip.height);
  return { left, top };
}

export function Tooltip({ content, side = 'top', delayMs = 300, children }: TooltipProps) {
  const wrapRef = useRef<HTMLSpanElement | null>(null);
  const tipRef = useRef<HTMLDivElement | null>(null);
  const timerRef = useRef<number | null>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: -9999, top: -9999 });

  const cancelTimer = () => {
    if (timerRef.current != null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };
  const show = () => setOpen(true);
  const hide = () => {
    cancelTimer();
    setOpen(false);
  };
  const showDelayed = () => {
    cancelTimer();
    timerRef.current = window.setTimeout(show, delayMs);
  };

  useEffect(() => cancelTimer, []);

  // Позиция после измерения тултипа (портал уже в DOM).
  useLayoutEffect(() => {
    if (!open) return;
    const wrap = wrapRef.current;
    const tip = tipRef.current;
    if (!wrap || !tip) return;
    const r = tip.getBoundingClientRect();
    setPos(place(side, wrap.getBoundingClientRect(), { width: r.width, height: r.height }));
  }, [open, side, content]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') hide();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  return (
    <span
      ref={wrapRef}
      style={{ display: 'inline-flex' }}
      onMouseEnter={showDelayed}
      onMouseLeave={hide}
      onFocusCapture={show}
      onBlurCapture={hide}
      onClickCapture={hide}
    >
      {children}
      {open &&
        createPortal(
          <div
            ref={tipRef}
            role="tooltip"
            className="tooltip fade"
            style={{
              position: 'fixed',
              left: pos.left,
              top: pos.top,
              zIndex: 80,
              pointerEvents: 'none',
            }}
          >
            {content}
          </div>,
          document.body,
        )}
    </span>
  );
}
