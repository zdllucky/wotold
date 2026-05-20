// [B17] Deterministic seeded waveform — bars из shared.jsx reference handoff.
// Используется в recording state (2 lanes), sticky scrubber на CallDetail,
// MiniWave для voice samples row и speaker confirm modal.

import type { CSSProperties } from 'react';

interface Bar {
  x: number;
  h: number;
}

// Tiny seeded LCG — deterministic per seed, не зависит от Math.random.
function bars(seed: number, count: number, width: number, height: number, amp = 0.9): Bar[] {
  let s = seed >>> 0;
  const rand = (): number => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return (s & 0xffff) / 0xffff;
  };
  const mid = height / 2;
  const step = width / count;
  const out: Bar[] = [];
  for (let i = 0; i < count; i++) {
    const env =
      0.35 +
      0.65 *
        Math.abs(
          Math.sin((i / count) * Math.PI * 2.7) *
            Math.cos((i / count) * Math.PI * 1.3),
        );
    const r = (rand() * 2 - 1) * amp * env;
    const h = Math.max(2, Math.abs(r) * mid);
    out.push({ x: i * step + step / 2, h });
  }
  return out;
}

interface WaveformProps {
  seed?: number;
  /** CSS color string — supports var(--*) tokens. */
  color?: string;
  height?: number;
  width?: number;
  count?: number;
  gap?: number;
  opacity?: number;
  style?: CSSProperties;
}

export function Waveform({
  seed = 1,
  color = 'currentColor',
  height = 80,
  width = 800,
  count = 120,
  gap = 1.5,
  opacity = 1,
  style,
}: WaveformProps) {
  const data = bars(seed, count, width, height);
  const barW = Math.max(1, width / count - gap);
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      aria-hidden="true"
      style={{ width: '100%', height: '100%', display: 'block', opacity, ...style }}
    >
      {data.map((b, i) => (
        <rect
          key={i}
          x={b.x - barW / 2}
          y={height / 2 - b.h}
          width={barW}
          height={b.h * 2}
          rx={Math.min(barW / 2, 1.5)}
          fill={color}
        />
      ))}
    </svg>
  );
}

interface MiniWaveProps {
  seed?: number;
  color?: string;
  width?: number;
  height?: number;
  count?: number;
}

/** In-line transcript / voice sample mini waveform (~100×18 px). */
export function MiniWave({
  seed = 4,
  color = 'currentColor',
  width = 100,
  height = 18,
  count = 28,
}: MiniWaveProps) {
  return (
    <Waveform
      seed={seed}
      color={color}
      width={width}
      height={height}
      count={count}
      gap={1}
    />
  );
}
