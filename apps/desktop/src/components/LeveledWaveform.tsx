// [B14] Canvas-based waveform driven by external level history. Каждый bar
// — один RMS sample (0..1). Используется в recording state из useAudioLevel
// hook. Не делает getUserMedia / синтетику сам — данные приходят из props.

import { useEffect, useRef } from 'react';

interface LeveledWaveformProps {
  /** Rolling history of RMS values, 0..1. Length = bar count. */
  data: number[];
  /** CSS color string — supports var(--*). Resolved через computed parent style. */
  color?: string;
  /** Canvas height (CSS px). Width fills parent. */
  height?: number;
  /** Gap между bars in px. */
  gap?: number;
  /** Amplification multiplier (1 = raw, 2.5 = brighter). */
  amp?: number;
}

export function LeveledWaveform({
  data,
  color = 'currentColor',
  height = 110,
  gap = 2.5,
  amp = 2.6,
}: LeveledWaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Resize canvas to container (with HiDPI scaling).
  useEffect(() => {
    const resize = () => {
      const c = canvasRef.current;
      const wrap = containerRef.current;
      if (!c || !wrap) return;
      const rect = wrap.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      c.width = Math.max(100, Math.round(rect.width * dpr));
      c.height = Math.max(20, Math.round(rect.height * dpr));
      const ctx = c.getContext('2d');
      if (ctx) ctx.scale(dpr, dpr);
    };
    resize();
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, [height]);

  // Redraw on data change.
  useEffect(() => {
    render(canvasRef.current, data, gap, amp);
  }, [data, gap, amp]);

  return (
    <div ref={containerRef} style={{ width: '100%', height: '100%', color }}>
      <canvas
        ref={canvasRef}
        style={{ width: '100%', height: '100%', display: 'block' }}
      />
    </div>
  );
}

function render(
  canvas: HTMLCanvasElement | null,
  bars: number[],
  gap: number,
  amp: number,
): void {
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.width / dpr;
  const h = canvas.height / dpr;
  ctx.clearRect(0, 0, w, h);
  const parent = canvas.parentElement;
  const cssColor = parent ? window.getComputedStyle(parent).color : '#000';
  ctx.fillStyle = cssColor;
  const count = bars.length;
  const step = w / count;
  const barW = Math.max(1, step - gap);
  const mid = h / 2;
  for (let i = 0; i < count; i++) {
    const v = Math.min(1, bars[i]! * amp);
    const barH = Math.max(1.5, v * mid);
    const x = i * step + (step - barW) / 2;
    const y = mid - barH;
    const r = Math.min(barW / 2, 1.5);
    roundRect(ctx, x, y, barW, barH * 2, r);
    ctx.fill();
  }
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.lineTo(x + w - rr, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + rr);
  ctx.lineTo(x + w, y + h - rr);
  ctx.quadraticCurveTo(x + w, y + h, x + w - rr, y + h);
  ctx.lineTo(x + rr, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - rr);
  ctx.lineTo(x, y + rr);
  ctx.quadraticCurveTo(x, y, x + rr, y);
  ctx.closePath();
}
