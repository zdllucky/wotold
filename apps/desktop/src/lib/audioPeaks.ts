// Web Audio WAV → амплитудные peaks (0..1). Общий модуль для главного плеера
// (useCallAudio, full-file) и скраб-дорожки сэмпла (SpeakerCard, sub-range).
// Чистый `bucketPeaks` вынесен отдельно для unit-тестов (Web Audio decode в
// jsdom недоступен).

/**
 * Даунсэмпл моно-канала на участке `[startSec, endSec)` в `count`
 * нормализованных (0..1) peaks. Чистая функция — не зависит от Web Audio.
 */
export function bucketPeaks(
  channel: Float32Array | number[],
  sampleRate: number,
  startSec: number,
  endSec: number,
  count: number,
): number[] {
  const peaks = new Array<number>(Math.max(0, count)).fill(0);
  if (count <= 0 || sampleRate <= 0) return peaks;
  const len = channel.length;
  const from = Math.max(0, Math.min(len, Math.floor(startSec * sampleRate)));
  const to = Math.max(from, Math.min(len, Math.floor(endSec * sampleRate)));
  const span = to - from;
  if (span <= 0) return peaks;
  const bucket = Math.max(1, Math.floor(span / count));
  let maxAbs = 0;
  for (let i = 0; i < count; i++) {
    const s = from + i * bucket;
    const e = Math.min(to, s + bucket);
    let peak = 0;
    for (let j = s; j < e; j++) {
      const v = Math.abs(channel[j] ?? 0);
      if (v > peak) peak = v;
    }
    peaks[i] = peak;
    if (peak > maxAbs) maxAbs = peak;
  }
  if (maxAbs > 0) {
    for (let i = 0; i < count; i++) peaks[i]! /= maxAbs;
  }
  return peaks;
}

/** Fetch + decode WAV → AudioBuffer. Кидает на network / decode error. */
async function decodeBuffer(url: string): Promise<AudioBuffer> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`fetch ${response.status}`);
  const buffer = await response.arrayBuffer();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Ctx = window.AudioContext ?? (window as any).webkitAudioContext;
  const ctx: AudioContext = new Ctx();
  try {
    return await ctx.decodeAudioData(buffer.slice(0));
  } finally {
    void ctx.close().catch(() => undefined);
  }
}

/** Peaks по всему файлу (0..1), `count` баров. */
export async function decodeWavPeaks(url: string, count: number): Promise<number[]> {
  const audio = await decodeBuffer(url);
  return bucketPeaks(audio.getChannelData(0), audio.sampleRate, 0, audio.duration, count);
}

/** Peaks по участку `[startSec, endSec)` (0..1), `count` баров — для скраб-дорожки сэмпла. */
export async function decodeWavPeaksRange(
  url: string,
  count: number,
  startSec: number,
  endSec: number,
): Promise<number[]> {
  const audio = await decodeBuffer(url);
  return bucketPeaks(audio.getChannelData(0), audio.sampleRate, startSec, endSec, count);
}
