// [B18.6c] Wotold v2 uikit — avatar + stacked group (.avatar / .avatar-grp from wk.css).

interface AvatarProps {
  name: string;
  color?: string;
  size?: number;
  square?: boolean;
}

function initials(name: string): string {
  if (!name) return '?';
  if (name === 'Вы') return 'Вы';
  const words = name.trim().split(/\s+/).filter(Boolean);
  const first = words[0]?.[0] ?? '';
  const second = words[1]?.[0] ?? '';
  return (first + second).toUpperCase() || '?';
}

export function Avatar({ name, color = 'var(--sp1)', size = 22, square }: AvatarProps) {
  return (
    <span
      className="avatar"
      title={name}
      style={{
        background: color,
        width: size,
        height: size,
        fontSize: Math.round(size * 0.42),
        borderRadius: square ? 'var(--r-xs)' : undefined,
      }}
    >
      {initials(name)}
    </span>
  );
}

interface AvatarGroupItem {
  name: string;
  color: string;
}

interface AvatarGroupProps {
  items: AvatarGroupItem[];
  size?: number;
  max?: number;
}

export function AvatarGroup({ items, size = 20, max = 4 }: AvatarGroupProps) {
  const extra = items.length - max;
  return (
    <span className="avatar-grp">
      {items.slice(0, max).map((it) => (
        <Avatar key={`${it.name}:${it.color}`} name={it.name} color={it.color} size={size} />
      ))}
      {extra > 0 && (
        <span
          className="avatar"
          style={{
            width: size,
            height: size,
            fontSize: Math.round(size * 0.4),
            background: 'var(--sunken)',
            color: 'var(--text-3)',
          }}
        >
          +{extra}
        </span>
      )}
    </span>
  );
}
