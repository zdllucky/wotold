// [P-fix10] Typewriter reveal — прогрессивно «печатает» готовый текст по буквам
// (эффект «ИИ печатает»). Это REVEAL по готовности, не стриминг генерации.
//
// Длительность ≈ фиксированная (REVEAL_MS) независимо от длины — короткий recap
// и длинный печатаются за одно время, не затягивая. `enabled=false` или
// prefers-reduced-motion → текст сразу целиком (анимация только украшает,
// доступность не страдает).

import { useEffect, useRef, useState } from 'react';

/** Целевая длительность полного reveal (мс). */
const REVEAL_MS = 1800;
/** Шаг тика (мс) ~60fps. */
const TICK_MS = 16;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export interface TypewriterState {
  /** Видимая часть текста (растёт до полного). */
  shown: string;
  /** true когда reveal завершён (или анимация выключена). */
  done: boolean;
}

/**
 * Прогрессивный reveal `text`. При смене `text` анимация перезапускается.
 * @param enabled включить анимацию (иначе текст сразу целиком).
 */
export function useTypewriter(text: string, enabled: boolean): TypewriterState {
  const [shown, setShown] = useState(enabled ? '' : text);
  const [done, setDone] = useState(!enabled);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }

    if (!enabled || prefersReducedMotion() || text.length === 0) {
      setShown(text);
      setDone(true);
      return;
    }

    setShown('');
    setDone(false);
    const steps = Math.max(1, Math.round(REVEAL_MS / TICK_MS));
    const perStep = Math.max(1, Math.ceil(text.length / steps));
    let i = 0;
    timerRef.current = setInterval(() => {
      i = Math.min(text.length, i + perStep);
      setShown(text.slice(0, i));
      if (i >= text.length) {
        if (timerRef.current) {
          clearInterval(timerRef.current);
          timerRef.current = null;
        }
        setDone(true);
      }
    }, TICK_MS);

    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [text, enabled]);

  return { shown, done };
}
