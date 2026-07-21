// [UI-fix A] Ширина content-box элемента через ResizeObserver.
//
// Возвращает 0 до первого замера. В окружениях без ResizeObserver
// (jsdom-тесты) — no-op, остаётся 0: потребитель обязан иметь fallback.
// Ширина округляется — RO шлёт дробные contentRect и без округления
// возможен рендер-луп на полупиксельном джиттере.

import { useEffect, useState, type RefObject } from 'react';

export function useElementWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = ref.current;
    if (typeof ResizeObserver === 'undefined' || !el) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0;
      setWidth(Math.round(w));
    });
    ro.observe(el);
    // Initial measure — RO может не сработать синхронно.
    setWidth(Math.round(el.getBoundingClientRect().width));
    return () => ro.disconnect();
  }, [ref]);

  return width;
}
