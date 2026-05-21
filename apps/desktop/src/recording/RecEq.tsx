// [W3] Three-bar equalizer indicator. Animates while recording, freezes flat
// when paused. Honors `prefers-reduced-motion` via CSS.

interface RecEqProps {
  paused?: boolean;
}

export function RecEq({ paused = false }: RecEqProps) {
  return (
    <span
      className={`rec-eq${paused ? ' rec-eq--paused' : ''}`}
      aria-hidden="true"
    >
      <span />
      <span />
      <span />
    </span>
  );
}
