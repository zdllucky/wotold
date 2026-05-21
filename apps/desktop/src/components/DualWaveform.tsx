// [B17 V3.0] Stereo-split waveform — визуально одна дорожка под капотом two
// channels. Top half (центр → вверх) — mic в --ink. Bottom half (центр →
// вниз) — system в --accent. Single x-axis timeline синхронизирован между
// каналами (оба пришли из одного `audio:level` event с тем же timestamp).
//
// Если RMS равен 0 — бар flat у midline (1.5px минимум). Никаких суррогатов
// движения: тишина → плоская линия.

import { useEffect, useRef } from 'react';

interface DualWaveformProps {
  /** Mic rolling history 0..1. */
  mic: number[];
  /** System rolling history 0..1. */
  system: number[];
  /** Canvas height in CSS px. Делится поровну между two channels. */
  height?: number;
  /** Gap между bars in px. */
  gap?: number;
  /** Amplification multiplier на per-channel bars. */
  amp?: number;
  /** Subtle center line color (default var(--line-soft)). */
  centerColor?: string;
}

export function DualWaveform({
  mic,
  system,
  height = 220,
  gap = 2.5,
  amp = 2.6,
  centerColor = 'var(--line-soft)',
}: DualWaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Resize canvas to container.
  useEffect(() => {
    const resize = () => {
      const c = canvasRef.current;
      const wrap = containerRef.current;
      if (!c || !wrap) return;
      const rect = wrap.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      c.width = Math.max(100, Math.round(rect.width * dpr));
      c.height = Math.max(40, Math.round(rect.height * dpr));
      const ctx = c.getContext('2d');
      if (ctx) ctx.scale(dpr, dpr);
    };
    resize();
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, [height]);

  // Redraw on data change.
  useEffect(() => {
    render(canvasRef.current, mic, system, gap, amp, centerColor);
  }, [mic, system, gap, amp, centerColor]);

  return (
    <div ref={containerRef} style={{ width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        style={{ width: '100%', height: '100%', display: 'block' }}
      />
    </div>
  );
}

function render(
  canvas: HTMLCanvasElement | null,
  mic: number[],
  system: number[],
  gap: number,
  amp: number,
  centerColorVar: string,
): void {
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.width / dpr;
  const h = canvas.height / dpr;
  ctx.clearRect(0, 0, w, h);

  // Resolve CSS vars через canvas.parentElement computed style.
  const parent = canvas.parentElement;
  const css = parent ? window.getComputedStyle(parent) : null;
  // Set tempvars on parent to read --ink and --accent via parent's resolved
  // values. Если css не доступен — fallback дефолты.
  const inkColor = css?.getPropertyValue('--ink')?.trim() || '#14151A';
  const accentColor = css?.getPropertyValue('--accent')?.trim() || '#7E1F2A';
  const lineColor =
    css?.getPropertyValue(centerColorVar.replace('var(', '').replace(')', '').trim())?.trim() ||
    '#ECEAE3';

  const mid = h / 2;
  const halfH = mid;
  const count = Math.max(mic.length, system.length);
  const step = w / count;
  const barW = Math.max(1, step - gap);

  // Center divider (subtle).
  ctx.fillStyle = lineColor;
  ctx.fillRect(0, mid - 0.5, w, 1);

  // Top half — mic. Bars от mid вверх.
  ctx.fillStyle = inkColor;
  for (let i = 0; i < count; i++) {
    const v = Math.min(1, (mic[i] ?? 0) * amp);
    const barH = Math.max(1.5, v * halfH);
    const x = i * step + (step - barW) / 2;
    const y = mid - barH;
    const r = Math.min(barW / 2, 1.5);
    roundRectFlatBottom(ctx, x, y, barW, barH, r);
    ctx.fill();
  }

  // Bottom half — system. Bars от mid вниз.
  ctx.fillStyle = accentColor;
  for (let i = 0; i < count; i++) {
    const v = Math.min(1, (system[i] ?? 0) * amp);
    const barH = Math.max(1.5, v * halfH);
    const x = i * step + (step - barW) / 2;
    const y = mid;
    const r = Math.min(barW / 2, 1.5);
    roundRectFlatTop(ctx, x, y, barW, barH, r);
    ctx.fill();
  }
}

// Round только верхние углы (для top-half bars — flat side на midline).
function roundRectFlatBottom(
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
  ctx.lineTo(x + w, y + h);
  ctx.lineTo(x, y + h);
  ctx.lineTo(x, y + rr);
  ctx.quadraticCurveTo(x, y, x + rr, y);
  ctx.closePath();
}

// Round только нижние углы (для bottom-half bars — flat side на midline).
function roundRectFlatTop(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x, y);
  ctx.lineTo(x + w, y);
  ctx.lineTo(x + w, y + h - rr);
  ctx.quadraticCurveTo(x + w, y + h, x + w - rr, y + h);
  ctx.lineTo(x + rr, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - rr);
  ctx.lineTo(x, y);
  ctx.closePath();
}
