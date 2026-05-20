// [B17] Shared utilities for CallDetailPage tabs (transcript / recap / tasks).

export const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

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

export function pluralParticipants(n: number): string {
  if (n === 1) return 'участник';
  if (n >= 2 && n <= 4) return 'участника';
  return 'участников';
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
