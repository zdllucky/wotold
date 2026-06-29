// #48 (M7.5 follow-up): прогресс-бар использования квоты.
//
// Lightweight DS-компонент: один track + filled portion + лейбл "used/limit".
// Цвет filled зависит от percent: ok | warning >=75% | danger >=95%.
//
// [B17] Atelier v2 — token vars + inline styling, без отдельных DS-классов.

import { useSyncExternalStore, type CSSProperties } from 'react';

import { bcp47, useI18n } from '../i18n';

type Tone = 'ok' | 'warning' | 'danger';

interface UsageBarProps {
  label: string;
  used: number;
  limit: number;
  /** Кастомный formatter для значений (e.g. '120 сек', '5,000 токенов'). */
  format?: (v: number) => string;
}

function pickTone(pct: number): Tone {
  if (pct >= 95) return 'danger';
  if (pct >= 75) return 'warning';
  return 'ok';
}

function fillColor(tone: Tone): string {
  switch (tone) {
    case 'danger':
      return 'var(--danger)';
    case 'warning':
      return 'var(--warn)';
    case 'ok':
      return 'var(--ok)';
  }
}

const trackStyle: CSSProperties = {
  position: 'relative',
  width: '100%',
  height: 6,
  background: 'var(--sunken)',
  borderRadius: 'var(--r-pill)',
  overflow: 'hidden',
};

// [B17 a11y] WCAG SC 2.3.3 — respect prefers-reduced-motion для width
// transition (inline styles не reachable из CSS @media query).
function subscribeReducedMotion(callback: () => void): () => void {
  if (typeof window === 'undefined' || !window.matchMedia) {
    return () => undefined;
  }
  const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
  mq.addEventListener('change', callback);
  return () => mq.removeEventListener('change', callback);
}

function getReducedMotion(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function useReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribeReducedMotion,
    getReducedMotion,
    () => false,
  );
}

export function UsageBar({ label, used, limit, format }: UsageBarProps) {
  const { locale, t } = useI18n();
  const fmt =
    format ??
    ((v: number) => v.toLocaleString(bcp47(locale as Parameters<typeof bcp47>[0])));
  const safeLimit = limit > 0 ? limit : 0;
  const pct = safeLimit === 0 ? 0 : Math.min(100, Math.round((used / safeLimit) * 100));
  const tone = pickTone(pct);
  const reducedMotion = useReducedMotion();

  return (
    <div
      data-tone={tone}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 6,
        fontFamily: 'var(--font)',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'baseline',
          justifyContent: 'space-between',
          gap: 8,
          fontSize: 13,
        }}
      >
        <span style={{ color: 'var(--text)', fontWeight: 500 }}>{label}</span>
        <span
          style={{
            color: 'var(--text-3)',
            fontFamily: 'var(--mono)',
            fontSize: 12,
          }}
        >
          {safeLimit === 0 ? (
            <span title={t('usage.noLimit')}>{fmt(used)} / ∞</span>
          ) : (
            <>
              {fmt(used)} / {fmt(safeLimit)}{' '}
              <span style={{ color: 'var(--text-faint)', fontSize: 11 }}>({pct}%)</span>
            </>
          )}
        </span>
      </div>
      <div
        style={trackStyle}
        role="progressbar"
        aria-valuenow={pct}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label}
      >
        <div
          style={{
            height: '100%',
            width: `${pct}%`,
            background: fillColor(tone),
            borderRadius: 'var(--r-pill)',
            transition: reducedMotion
              ? 'none'
              : 'width var(--base) cubic-bezier(0.16, 1, 0.3, 1), background var(--base) cubic-bezier(0.16, 1, 0.3, 1)',
          }}
        />
      </div>
    </div>
  );
}
