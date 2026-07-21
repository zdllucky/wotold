// [B21] Wotold v2 uikit — determinate progress bar (.progress from wk.css).
// Единственный канонный трек для загрузок моделей / квоты — заменяет
// самописные inline-бары (VoiceModel/Onboarding) и legacy UsageBar в Settings.

import type { CSSProperties } from 'react';

interface ProgressProps {
  /** 0..100; клампится. */
  value: number;
  ariaLabel?: string;
  style?: CSSProperties;
}

export function Progress({ value, ariaLabel, style }: ProgressProps) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div
      className="progress"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(pct)}
      aria-label={ariaLabel}
      style={style}
    >
      <i style={{ width: `${pct}%` }} />
    </div>
  );
}
