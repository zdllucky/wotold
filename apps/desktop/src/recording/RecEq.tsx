// [W3] Three-bar equalizer indicator. Animates while recording, freezes flat
// when paused. Honors `prefers-reduced-motion` via CSS.
//
// [S8] When `levels` is provided (RecFloat passes useAudioLevel().mic), bars
// reflect real RMS — last 3 samples mapped to bar heights. Without levels
// (paused or no audio:level subscription) falls back to CSS-keyframed
// animation (RecStrip case).

interface RecEqProps {
  paused?: boolean;
  /** Optional rolling RMS history (0..1). Last 3 entries used as bar heights. */
  levels?: number[];
  /** Бары наследуют цвет текста (currentColor) — для красных danger-кнопок. */
  inherit?: boolean;
}

const MIN_HEIGHT = 3;
const MAX_HEIGHT = 12;

function barHeight(level: number): number {
  // Mic levels in practice rarely hit 1.0 — boost to use full bar range
  // (0..0.4 normal speech → 0..1 visual).
  const boosted = Math.min(1, level / 0.4);
  return MIN_HEIGHT + (MAX_HEIGHT - MIN_HEIGHT) * boosted;
}

export function RecEq({ paused = false, levels, inherit = false }: RecEqProps) {
  const useReal = !paused && levels !== undefined && levels.length >= 3;
  // Take the last 3 samples — most recent right-most bar.
  const recent = useReal ? levels.slice(-3) : [0, 0, 0];

  return (
    <span
      className={`rec-eq${paused ? ' rec-eq--paused' : ''}${
        useReal ? ' rec-eq--live' : ''
      }${inherit ? ' rec-eq--inherit' : ''}`}
      aria-hidden="true"
    >
      <span
        style={
          useReal
            ? { height: `${barHeight(recent[0] ?? 0)}px` }
            : undefined
        }
      />
      <span
        style={
          useReal
            ? { height: `${barHeight(recent[1] ?? 0)}px` }
            : undefined
        }
      />
      <span
        style={
          useReal
            ? { height: `${barHeight(recent[2] ?? 0)}px` }
            : undefined
        }
      />
    </span>
  );
}
