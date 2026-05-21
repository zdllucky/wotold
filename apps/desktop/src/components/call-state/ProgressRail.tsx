// [V6.1] ProgressRail — 2px ink bar.
//
// Determinate (передаётся pct 0-100) или indeterminate (sliding sweep
// когда прогресс неизвестен). Pure presentational.

export interface ProgressRailProps {
  /** 0..100. Clamp'ится автоматически. Игнорируется если indeterminate=true. */
  pct?: number;
  indeterminate?: boolean;
  /** Accessibility — label для screen reader'а. */
  ariaLabel?: string;
}

export function ProgressRail({ pct, indeterminate, ariaLabel }: ProgressRailProps) {
  if (indeterminate) {
    return (
      <div
        className="rail rail--indeterminate"
        role="progressbar"
        aria-label={ariaLabel}
        aria-valuetext="processing"
      >
        <div className="rail-fill" />
      </div>
    );
  }
  const w = Math.max(0, Math.min(100, pct ?? 0));
  return (
    <div
      className="rail"
      role="progressbar"
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(w)}
    >
      <div className="rail-fill" style={{ width: `${w}%` }} />
    </div>
  );
}
