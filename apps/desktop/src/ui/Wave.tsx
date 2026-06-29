// [B18.6c] Wotold v2 uikit — animated waveform bars (.wave from wk.css).

interface WaveProps {
  bars?: number;
  color?: string;
}

export function Wave({ bars = 5, color = 'var(--accent)' }: WaveProps) {
  return (
    <span className="wave" style={{ color }}>
      {Array.from({ length: bars }).map((_, i) => (
        <i key={i} style={{ animationDelay: `${i * 0.1}s` }} />
      ))}
    </span>
  );
}
