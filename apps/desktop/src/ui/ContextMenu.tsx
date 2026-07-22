// [B20.5] ContextMenu — курсор-позиционированное меню (ПКМ) поверх .menu
// токенов uikit. Portal в body + position:fixed в точке клика, clamp к
// viewport (флип вверх/влево у краёв). Закрытие: Escape / клик снаружи /
// клик по пункту (bubble, зеркалит Dropdown). Children = MenuItem/MenuSep.

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

export interface ContextMenuPos {
  x: number;
  y: number;
}

interface ContextMenuProps {
  pos: ContextMenuPos;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}

const VIEWPORT_PAD = 8;

export function ContextMenu({ pos, onClose, children, width = 190 }: ContextMenuProps) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [place, setPlace] = useState<{ left: number; top: number }>({ left: pos.x, top: pos.y });

  // Clamp к viewport после измерения панели.
  useLayoutEffect(() => {
    const el = panelRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    let left = pos.x;
    let top = pos.y;
    if (left + r.width > window.innerWidth - VIEWPORT_PAD) {
      left = Math.max(VIEWPORT_PAD, pos.x - r.width);
    }
    if (top + r.height > window.innerHeight - VIEWPORT_PAD) {
      top = Math.max(VIEWPORT_PAD, pos.y - r.height);
    }
    setPlace({ left, top });
  }, [pos.x, pos.y]);

  // Фокус на первый пункт — клавиатурная навигация с ходу.
  useEffect(() => {
    panelRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, []);

  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={panelRef}
      className="menu fade"
      role="menu"
      style={{ position: 'fixed', zIndex: 60, width, left: place.left, top: place.top }}
      onClick={onClose}
      onContextMenu={(e) => e.preventDefault()}
    >
      {children}
    </div>,
    document.body,
  );
}
