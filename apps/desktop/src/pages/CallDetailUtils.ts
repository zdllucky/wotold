// Shared utilities for CallDetailPage tabs (transcript / recap / tasks).

// Canonical speaker-avatar palette — Wotold v2 theme-aware tokens (light/dark
// values in tokens.css). Single source of truth: all speaker/contact avatars
// import this so the same speaker is the same colour on every screen. Used only
// in DOM inline-style backgrounds (no canvas), so CSS vars resolve correctly.
export const SP_COLORS = ['var(--sp1)', 'var(--sp2)', 'var(--sp3)', 'var(--sp4)', 'var(--sp5)'];

export function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
  );
}

export function formatDur(sec: number): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}

export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function hashId(id: string): number {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) | 0;
  return Math.abs(h) % 1000;
}
