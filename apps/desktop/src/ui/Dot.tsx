// [B18.6c] Wotold v2 uikit — status dot (.dot from wk.css).

interface DotProps {
  color?: string;
  ring?: boolean;
  pulse?: boolean;
  size?: number;
}

export function Dot({ color = 'var(--accent)', ring, pulse, size = 8 }: DotProps) {
  return (
    <span
      className={'dot' + (ring ? ' dot--ring' : '') + (pulse ? ' dot--pulse' : '')}
      style={{
        width: size,
        height: size,
        color,
        background: ring ? 'transparent' : color,
      }}
    />
  );
}
