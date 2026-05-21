// [B17] Live waveform — реал-тайм визуализация микрофона через Web Audio API.
//
// AnalyserNode → byte time domain → RMS → rolling buffer (140 bars).
// Canvas redraw в requestAnimationFrame. При unmount/active=false закрываем
// AudioContext и stop track'и stream.
//
// Системный звук (loopback) Web Audio не доступен напрямую — для system lane
// есть SeededAnimatedWaveform fallback (rotating seed, выглядит «живым»).

import { useEffect, useRef } from 'react';

interface LiveWaveformProps {
  /** When true, opens AudioContext + acquires getUserMedia.
   *  Pass `false` для cleanup без unmount. */
  active: boolean;
  /** CSS color string. Supports var(--*). */
  color?: string;
  /** Canvas height (CSS px). Width fills parent. */
  height?: number;
  /** Bar count in the rolling buffer. */
  count?: number;
  /** Gap between bars in px. */
  gap?: number;
  /** Amplification multiplier (1 = raw RMS, 1.5 = brighter). */
  amp?: number;
}

export function LiveWaveform({
  active,
  color = 'currentColor',
  height = 110,
  count = 140,
  gap = 2.5,
  amp = 2.4,
}: LiveWaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let audioCtx: AudioContext | null = null;
    let analyser: AnalyserNode | null = null;
    let source: MediaStreamAudioSourceNode | null = null;
    let stream: MediaStream | null = null;
    let rafId: number | null = null;
    const bars: number[] = new Array(count).fill(0);
    let last = performance.now();

    (async () => {
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: false,
          },
        });
        if (cancelled) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const Ctx = window.AudioContext ?? (window as any).webkitAudioContext;
        audioCtx = new Ctx();
        source = audioCtx.createMediaStreamSource(stream);
        analyser = audioCtx.createAnalyser();
        analyser.fftSize = 1024;
        analyser.smoothingTimeConstant = 0.5;
        source.connect(analyser);
        const buf = new Uint8Array(analyser.frequencyBinCount);

        const draw = () => {
          if (cancelled || !analyser) return;
          analyser.getByteTimeDomainData(buf);
          let sum = 0;
          for (let i = 0; i < buf.length; i++) {
            const v = (buf[i]! - 128) / 128;
            sum += v * v;
          }
          const rms = Math.sqrt(sum / buf.length);
          // Throttle to ~60Hz max; subsample to 24Hz for smoother bars.
          const now = performance.now();
          if (now - last > 16) {
            // shift left, push new sample
            for (let i = 0; i < count - 1; i++) bars[i] = bars[i + 1]!;
            bars[count - 1] = rms;
            last = now;
          }
          render(canvasRef.current, bars, color, gap, amp);
          rafId = requestAnimationFrame(draw);
        };
        draw();
      } catch (e) {
        console.warn('LiveWaveform: getUserMedia failed', e);
      }
    })();

    return () => {
      cancelled = true;
      if (rafId !== null) cancelAnimationFrame(rafId);
      try {
        source?.disconnect();
      } catch {
        /* ignore */
      }
      try {
        stream?.getTracks().forEach((t) => t.stop());
      } catch {
        /* ignore */
      }
      void audioCtx?.close().catch(() => undefined);
    };
  }, [active, color, count, gap, amp]);

  // Resize canvas to fit container on mount + window resize.
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

  return (
    <div
      ref={containerRef}
      style={{ width: '100%', height: '100%', color }}
    >
      <canvas
        ref={canvasRef}
        style={{ width: '100%', height: '100%', display: 'block' }}
      />
    </div>
  );
}

// [B17] Synthetic-driven scrolling waveform — для системного звука, который
// браузер не может захватить через Web Audio. Bars двигаются на основе
// time + multi-octave sine "noise". Не привязан к real аудио, но визуально
// «живой», без статичности.
interface SyntheticWaveformProps {
  active: boolean;
  color?: string;
  height?: number;
  count?: number;
  gap?: number;
  /** Baseline activity intensity (0..1). Higher = louder-looking. */
  intensity?: number;
}

export function SyntheticWaveform({
  active,
  color = 'currentColor',
  height = 110,
  count = 140,
  gap = 2.5,
  intensity = 0.45,
}: SyntheticWaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let rafId: number | null = null;
    const bars: number[] = new Array(count).fill(0);
    let last = performance.now();
    const t0 = performance.now();

    const tick = () => {
      if (cancelled) return;
      const now = performance.now();
      if (now - last > 50) {
        // Shift left, push new synthetic sample.
        for (let i = 0; i < count - 1; i++) bars[i] = bars[i + 1]!;
        const t = (now - t0) / 1000;
        // Three octaves of sin + small white noise.
        const v =
          intensity *
          (0.55 +
            0.3 * Math.sin(t * 2.1) +
            0.18 * Math.sin(t * 5.3 + 1.7) +
            0.12 * Math.sin(t * 11.2 + 0.4) +
            (Math.random() - 0.5) * 0.2);
        bars[count - 1] = Math.max(0, Math.min(1, v));
        last = now;
      }
      render(canvasRef.current, bars, color, gap, 1);
      rafId = requestAnimationFrame(tick);
    };
    tick();
    return () => {
      cancelled = true;
      if (rafId !== null) cancelAnimationFrame(rafId);
    };
  }, [active, color, count, gap, intensity]);

  // Resize canvas to container.
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
  color: string,
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
  // Use computed color (supports var(--*) via CSS color resolution).
  // Canvas doesn't read CSS vars directly — read from computed parent style.
  const cssColor = canvas.parentElement
    ? window.getComputedStyle(canvas.parentElement).color
    : color;
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
