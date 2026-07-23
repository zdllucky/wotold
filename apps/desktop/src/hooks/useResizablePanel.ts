// [B29.5a] Переиспользуемая resize+collapse панель — обобщение паттерна
// B26.11 (панель чатов ассистента) и рейла App.tsx: drag правой грани с
// clamp, авто-collapse ниже порога, persist в localStorage.

import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseResizablePanelOpts {
  min: number;
  max: number;
  defaultWidth: number;
  /** Drag ниже этой ширины → авто-collapse (B26.11). */
  collapseAt: number;
  /** localStorage-ключ ширины (число px). */
  widthKey: string;
  /** localStorage-ключ collapse ('1'/'0'). */
  collapsedKey: string;
}

export interface ResizablePanel {
  width: number;
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  /** onMouseDown хэндла `.panel-resize`. */
  onResizeStart: (e: React.MouseEvent) => void;
}

/** Ширина свёрнутой панели (совпадает с CSS .side-list[data-collapsed]). */
const COLLAPSED_W = 48;

/** Чистый clamp ширины (юнит-тестируется; наследник clampChatsWidth B26.11). */
export function clampPanelWidth(w: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, w));
}

function readSavedWidth(key: string, min: number, max: number, fallback: number): number {
  try {
    const v = parseInt(localStorage.getItem(key) ?? '', 10);
    return v >= min && v <= max ? v : fallback;
  } catch {
    return fallback;
  }
}

function readSavedCollapsed(key: string): boolean {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

export function useResizablePanel(opts: UseResizablePanelOpts): ResizablePanel {
  const { min, max, defaultWidth, collapseAt, widthKey, collapsedKey } = opts;
  const [width, setWidth] = useState<number>(() =>
    readSavedWidth(widthKey, min, max, defaultWidth),
  );
  const [collapsed, setCollapsed] = useState<boolean>(() => readSavedCollapsed(collapsedKey));

  useEffect(() => {
    try {
      localStorage.setItem(widthKey, String(width));
      localStorage.setItem(collapsedKey, collapsed ? '1' : '0');
    } catch {
      // localStorage недоступен — не критично
    }
  }, [width, collapsed, widthKey, collapsedKey]);

  // [ревью B29 HIGH] Активный drag: страницы с панелями конditionally-рендерятся
  // (смена view хоткеем посреди драга) — без unmount-cleanup листенеры и
  // cursor/userSelect на body зависали бы до следующего mouseup.
  const activeDragEnd = useRef<(() => void) | null>(null);
  useEffect(() => () => activeDragEnd.current?.(), []);

  // Drag правой грани в ОБЕ стороны (канон рейла App.tsx onResizeStart +
  // onExpandResize): из развёрнутого — сжатие с авто-collapse; из свёрнутого
  // (48px) — вытягивание разворачивает при w > collapseAt (и обратно, пока
  // кнопка мыши зажата — [B30.5]).
  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const sx = e.clientX;
      const fromCollapsed = collapsed;
      const sw = fromCollapsed ? COLLAPSED_W : width;
      const end = () => {
        document.removeEventListener('mousemove', move);
        document.removeEventListener('mouseup', end);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        activeDragEnd.current = null;
      };
      const move = (ev: MouseEvent) => {
        const w = sw + (ev.clientX - sx);
        if (fromCollapsed) {
          if (w > collapseAt) {
            setCollapsed(false);
            setWidth(clampPanelWidth(w, min, max));
          } else {
            setCollapsed(true);
          }
          return;
        }
        if (w < collapseAt) {
          setCollapsed(true);
          end();
          return;
        }
        setWidth(clampPanelWidth(w, min, max));
      };
      document.addEventListener('mousemove', move);
      document.addEventListener('mouseup', end);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      activeDragEnd.current = end;
    },
    [width, collapsed, min, max, collapseAt],
  );

  return { width, collapsed, setCollapsed, onResizeStart };
}
