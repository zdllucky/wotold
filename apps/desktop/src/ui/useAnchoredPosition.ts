// [B19.4] Viewport-aware placement for floating panels (dropdowns/menus/selects).
// Hand-rolled (no floating-ui dep): measures the trigger + panel after open and
// returns a vertical flip + horizontal shift + optional max-height so the panel
// never spills past the window edges / rounded corners.

import { useLayoutEffect, useState, type RefObject } from 'react';

export interface Placement {
  /** Render the panel above the trigger instead of below. */
  up: boolean;
  /** px to translateX so the panel stays within the viewport. */
  shiftX: number;
  /** Cap height when the panel can't fit on either side. */
  maxHeight?: number;
}

const GAP = 6; // panel offset from the trigger
const MARGIN = 8; // min clearance from the window edge

/**
 * @param triggerRef element the panel is anchored to (the relative wrapper is fine).
 * @param panelRef   the floating panel itself (must be mounted when `open`).
 * @param preferUp   default vertical side before measurement.
 */
export function useAnchoredPosition(
  open: boolean,
  triggerRef: RefObject<HTMLElement | null>,
  panelRef: RefObject<HTMLElement | null>,
  preferUp = false,
): Placement {
  const [placement, setPlacement] = useState<Placement>({ up: preferUp, shiftX: 0 });

  useLayoutEffect(() => {
    if (!open) {
      setPlacement({ up: preferUp, shiftX: 0 });
      return;
    }
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    if (!trigger || !panel) return;

    const tr = trigger.getBoundingClientRect();
    const pr = panel.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    // Vertical: flip to whichever side has room (prefer the requested side).
    const spaceBelow = vh - tr.bottom;
    const spaceAbove = tr.top;
    let up = preferUp;
    if (!up && pr.height + GAP + MARGIN > spaceBelow && spaceAbove > spaceBelow) up = true;
    if (up && pr.height + GAP + MARGIN > spaceAbove && spaceBelow >= spaceAbove) up = false;

    // Horizontal: clamp the panel (measured at shiftX=0) inside the viewport.
    let shiftX = 0;
    if (pr.right > vw - MARGIN) shiftX = vw - MARGIN - pr.right;
    if (pr.left + shiftX < MARGIN) shiftX = MARGIN - pr.left;

    // If it still can't fit on the chosen side, cap its height + let it scroll.
    // Always cap when overflowing (a tiny scrollable panel beats one clipped by
    // the window chrome); floor at 60px so it stays usable.
    const avail = (up ? spaceAbove : spaceBelow) - GAP - MARGIN;
    const maxHeight = pr.height > avail && avail > 0 ? Math.max(60, Math.floor(avail)) : undefined;

    setPlacement({ up, shiftX, maxHeight });
    // Re-measure only when open toggles; panel content size is stable per-open.
  }, [open, preferUp, triggerRef, panelRef]);

  return placement;
}
