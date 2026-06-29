// [W3] Small round control used inside the RecFloat mini-widget. The visual
// glyph (pause bars, play triangle, stop square) is pure CSS — see
// `.rec-mini-btn--*` in components.css. The component itself stays markup-free
// so the same button works in any future surface.

import type { MouseEventHandler } from 'react';

export type RecMiniButtonVariant = 'pause' | 'play' | 'stop';

interface RecMiniButtonProps {
  variant: RecMiniButtonVariant;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  disabled?: boolean;
  ariaLabel: string;
  /** Pass through onMouseDown / etc when caller needs to suppress focus blur. */
  onMouseDown?: MouseEventHandler<HTMLButtonElement>;
}

export function RecMiniButton({
  variant,
  onClick,
  disabled = false,
  ariaLabel,
  onMouseDown,
}: RecMiniButtonProps) {
  return (
    <button
      type="button"
      className={`rec-mini-btn rec-mini-btn--${variant}`}
      onClick={onClick}
      onMouseDown={onMouseDown}
      disabled={disabled}
      aria-label={ariaLabel}
    />
  );
}
