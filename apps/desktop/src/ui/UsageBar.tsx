// #48 (M7.5 follow-up): прогресс-бар использования квоты.
//
// Lightweight DS-компонент: один track + filled portion + лейбл "used/limit".
// Цвет filled зависит от percent: ok | warning >=75% | danger >=95%.
//
// [B17] Atelier v2 — token vars + inline styling, без отдельных DS-классов.

import type { CSSProperties } from 'react';

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
      return 'var(--signal)';
    case 'warning':
      return 'var(--warning)';
    case 'ok':
      return 'var(--success)';
  }
}

const trackStyle: CSSProperties = {
  position: 'relative',
  width: '100%',
  height: 6,
  background: 'var(--bg-2)',
  borderRadius: 'var(--radius-pill)',
  overflow: 'hidden',
};

export function UsageBar({ label, used, limit, format }: UsageBarProps) {
  const fmt = format ?? ((v: number) => v.toLocaleString('ru-RU'));
  const safeLimit = limit > 0 ? limit : 0;
  const pct = safeLimit === 0 ? 0 : Math.min(100, Math.round((used / safeLimit) * 100));
  const tone = pickTone(pct);

  return (
    <div
      data-tone={tone}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 6,
        fontFamily: 'var(--font-sans)',
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
        <span style={{ color: 'var(--ink)', fontWeight: 500 }}>{label}</span>
        <span
          style={{
            color: 'var(--muted)',
            fontFamily: 'var(--font-mono)',
            fontSize: 12,
          }}
        >
          {safeLimit === 0 ? (
            <span title="лимит не настроен">{fmt(used)} / ∞</span>
          ) : (
            <>
              {fmt(used)} / {fmt(safeLimit)}{' '}
              <span style={{ color: 'var(--subtle)', fontSize: 11 }}>({pct}%)</span>
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
            borderRadius: 'var(--radius-pill)',
            transition:
              'width var(--duration-normal) var(--ease-out-expo), background var(--duration-normal) var(--ease-out-expo)',
          }}
        />
      </div>
    </div>
  );
}
