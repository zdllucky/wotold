// ─────────────────────────────────────────────────────────────
// Wotold v2 · Icon — line icon set (24 viewBox, 1.6 stroke, currentColor).
// Port of ~/Downloads/Wotold v2/uikit-icons.jsx (ROADMAP §B18.0).
// Usage: <Icon name="mic" size={16} />
// ─────────────────────────────────────────────────────────────
import type { CSSProperties, ReactNode } from 'react';

const ICONS = {
  record: <circle cx="12" cy="12" r="6" fill="currentColor" stroke="none" />,
  stop: <rect x="7" y="7" width="10" height="10" rx="2.5" fill="currentColor" stroke="none" />,
  pause: (
    <>
      <rect x="8" y="6" width="3" height="12" rx="1" fill="currentColor" stroke="none" />
      <rect x="13" y="6" width="3" height="12" rx="1" fill="currentColor" stroke="none" />
    </>
  ),
  play: <path d="M8 5.5v13l11-6.5z" fill="currentColor" stroke="none" />,
  mic: (
    <>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21M9 21h6" />
    </>
  ),
  headphones: (
    <>
      <path d="M4 13v-1a8 8 0 0 1 16 0v1" />
      <rect x="3" y="13" width="4" height="6" rx="1.5" />
      <rect x="17" y="13" width="4" height="6" rx="1.5" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="6.5" />
      <path d="M16 16l4 4" />
    </>
  ),
  command: <path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19 12a7 7 0 0 0-.1-1.3l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2.3-1.3L13.8 2h-3.6l-.4 2.5A7 7 0 0 0 7.5 5.8l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5 12c0 .4 0 .9.1 1.3l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2.3 1.3l.4 2.5h3.6l.4-2.5a7 7 0 0 0 2.3-1.3l2.3 1 2-3.4-2-1.5c.1-.4.1-.9.1-1.3z" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 20a8 8 0 0 1 16 0" />
    </>
  ),
  users: (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M3 19a6 6 0 0 1 12 0" />
      <path d="M16 5.5a3.5 3.5 0 0 1 0 7M17.5 13.5A6 6 0 0 1 21 19" />
    </>
  ),
  inbox: (
    <>
      <path d="M3 13l3-8h12l3 8v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />
      <path d="M3 13h5l1.5 2.5h5L16 13h5" />
    </>
  ),
  phone: <path d="M5 4h3.5l1.8 4.5-2.3 1.4a12 12 0 0 0 6.1 6.1l1.4-2.3L20 15.5V19a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2z" />,
  doc: (
    <>
      <path d="M6 3h8l4 4v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z" />
      <path d="M14 3v4h4M8.5 12.5h7M8.5 16h5" />
    </>
  ),
  sparkle: <path d="M12 3l1.7 5.1a3 3 0 0 0 2.2 2.2L21 12l-5.1 1.7a3 3 0 0 0-2.2 2.2L12 21l-1.7-5.1a3 3 0 0 0-2.2-2.2L3 12l5.1-1.7a3 3 0 0 0 2.2-2.2z" />,
  // [B24.1] Ассистент — из uikit-icons.jsx хендоффа (Wotold v2 · Ассистент).
  chat: (
    <path
      d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v8a2.5 2.5 0 0 1-2.5 2.5H11l-4.5 3.6V17h-.5A2.5 2.5 0 0 1 3.5 14.5z"
      transform="translate(.5 0)"
    />
  ),
  chevronDown: <path d="M5 9l7 7 7-7" />,
  chevronRight: <path d="M9 5l7 7-7 7" />,
  chevronLeft: <path d="M15 5l-7 7 7 7" />,
  chevronUpDown: <path d="M8 9l4-4 4 4M8 15l4 4 4-4" />,
  check: <path d="M5 12.5l4.5 4.5L19 7" />,
  x: <path d="M6 6l12 12M18 6L6 18" />,
  dots: (
    <>
      <circle cx="6" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="18" cy="12" r="1.4" fill="currentColor" stroke="none" />
    </>
  ),
  // [B29.7] Канонная урна: крышка + ручка + прямоугольное тело со слабым
  // сужением + рёбра. Прежняя воронка без рёбер читалась как стрелка вверх.
  trash: (
    <>
      <path d="M4 7h16" />
      <path d="M9.5 7V5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v2" />
      <path d="M6.5 7l.8 12.5a1.5 1.5 0 0 0 1.5 1.5h6.4a1.5 1.5 0 0 0 1.5-1.5L17.5 7" />
      <path d="M10 11v6M14 11v6" />
    </>
  ),
  download: (
    <>
      <path d="M12 3v12M7.5 10.5L12 15l4.5-4.5" />
      <path d="M5 20h14" />
    </>
  ),
  upload: (
    <>
      <path d="M12 21V9M7.5 13.5L12 9l4.5 4.5" />
      <path d="M5 4h14" />
    </>
  ),
  folder: <path d="M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />,
  refresh: (
    <>
      <path d="M4 12a8 8 0 0 1 13.7-5.6L20 8M20 4v4h-4" />
      <path d="M20 12a8 8 0 0 1-13.7 5.6L4 16M4 20v-4h4" />
    </>
  ),
  filter: <path d="M4 5h16l-6 7v6l-4 2v-8z" />,
  sort: <path d="M7 5v14M7 19l-3-3M7 5l3 3M17 5v14M17 5l-3 3M17 19l3-3" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  calendar: (
    <>
      <rect x="4" y="5" width="16" height="16" rx="2" />
      <path d="M4 9h16M8 3v4M16 3v4" />
    </>
  ),
  cpu: (
    <>
      <rect x="7" y="7" width="10" height="10" rx="1.5" />
      <path d="M10 7V4M14 7V4M10 20v-3M14 20v-3M7 10H4M7 14H4M20 10h-3M20 14h-3" />
    </>
  ),
  cloud: <path d="M7 18a4 4 0 0 1-.5-8A5.5 5.5 0 0 1 17 9.5a3.5 3.5 0 0 1 0 8.5z" />,
  key: (
    <>
      <circle cx="8" cy="8" r="3.5" />
      <path d="M10.5 10.5L19 19M16 16l2-2M14 18l1.5-1.5" />
    </>
  ),
  shield: (
    <>
      <path d="M12 3l7 3v5c0 4.4-3 7.7-7 9-4-1.3-7-4.6-7-9V6z" />
      <path d="M9 12l2 2 4-4" />
    </>
  ),
  wifiOff: <path d="M5 8.5a13 13 0 0 1 4.5-2.4M19 8.5a13 13 0 0 0-5-2.4M8 12a8 8 0 0 1 3-1.6M16 12a8 8 0 0 0-2-1.1M10 15.2a3.5 3.5 0 0 1 4 0M12 18.6v.1M3 3l18 18" />,
  alert: (
    <>
      <path d="M12 4L2.5 20.5h19z" />
      <path d="M12 10v5M12 18v.4" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 11v5M12 8v.4" />
    </>
  ),
  checkCircle: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M8 12.3l2.6 2.6L16 9.4" />
    </>
  ),
  arrowRight: <path d="M4 12h15M13 6l6 6-6 6" />,
  arrowUp: <path d="M12 20V5M6 11l6-6 6 6" />,
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4 12H2M22 12h-2M5.5 5.5L4 4M20 20l-1.5-1.5M18.5 5.5L20 4M4 20l1.5-1.5" />
    </>
  ),
  moon: <path d="M20 13.5A8 8 0 1 1 10.5 4a6.5 6.5 0 0 0 9.5 9.5z" />,
  external: (
    <>
      <path d="M14 4h6v6M20 4l-9 9" />
      <path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" />
    </>
  ),
  copy: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15a2 2 0 0 1-1-1.7V6a2 2 0 0 1 2-2h7A2 2 0 0 1 16 5" />
    </>
  ),
  edit: <path d="M5 19l-1 1 1-4L16 5a2 2 0 0 1 3 3L8 19z" />,
  tag: (
    <>
      <path d="M3 12V5a2 2 0 0 1 2-2h7l9 9-9 9z" />
      <circle cx="8" cy="8" r="1.4" fill="currentColor" stroke="none" />
    </>
  ),
  sidebar: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M9 4v16" />
    </>
  ),
  waveform: <path d="M3 12h2M7 12V8M11 12V4M15 12V7M19 12v-3M21 12h0M7 12v4M11 12v8M15 12v5M19 12v3" />,
  scissors: (
    <>
      <circle cx="6" cy="7" r="2.4" />
      <circle cx="6" cy="17" r="2.4" />
      <path d="M8 8.5L20 18M8 15.5L20 6" />
    </>
  ),
  bolt: <path d="M13 3L5 13h5l-1 8 8-10h-5z" />,
  send: <path d="M5 12l15-7-7 15-2-6z" />,
  globe: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M3.5 12h17M12 3.5c2.5 2.3 4 5.4 4 8.5s-1.5 6.2-4 8.5c-2.5-2.3-4-5.4-4-8.5s1.5-6.2 4-8.5z" />
    </>
  ),
  link: (
    <>
      <path d="M9 15l6-6" />
      <path d="M11 7l1-1a4 4 0 0 1 6 6l-2 2M13 17l-1 1a4 4 0 0 1-6-6l2-2" />
    </>
  ),
  list: <path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01" />,
  grid: (
    <>
      <rect x="4" y="4" width="7" height="7" rx="1.5" />
      <rect x="13" y="4" width="7" height="7" rx="1.5" />
      <rect x="4" y="13" width="7" height="7" rx="1.5" />
      <rect x="13" y="13" width="7" height="7" rx="1.5" />
    </>
  ),
  calendarWeek: (
    <>
      <rect x="3" y="5" width="18" height="15" rx="2" />
      <path d="M3 10h18M8 3v4M16 3v4M8 14v3M12 14v3M16 14v3" />
    </>
  ),
  code: <path d="M8 7l-5 5 5 5M16 7l5 5-5 5M13.5 4l-3 16" />,
  pip: (
    <>
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <rect x="12" y="11" width="7" height="5" rx="1" />
    </>
  ),
  lock: (
    <>
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </>
  ),
  // [B20.8] Crosshair «к текущему участку» — follow-кнопка плеера.
  locate: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v4M12 18v4M2 12h4M18 12h4" />
    </>
  ),
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof ICONS;

interface IconProps {
  name: IconName;
  size?: number;
  stroke?: number;
  style?: CSSProperties;
  className?: string;
  title?: string;
}

export function Icon({ name, size = 16, stroke = 1.6, style, className, title }: IconProps) {
  const glyph = ICONS[name];
  if (!glyph) {
    return (
      <span
        style={{ display: 'inline-block', width: size, height: size, ...style }}
        aria-hidden="true"
      />
    );
  }
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flexShrink: 0, display: 'inline-block', verticalAlign: 'middle', ...style }}
      role={title ? 'img' : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : 'true'}
    >
      {title && <title>{title}</title>}
      {glyph}
    </svg>
  );
}
