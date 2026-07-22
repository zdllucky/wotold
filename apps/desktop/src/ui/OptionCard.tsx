// [B18.6c] Wotold v2 uikit — selectable option card (.optioncard from wk.css).

import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icon';
import { Dot } from './Dot';
import { Chip } from './Chip';

interface QualityDotsProps {
  level: number;
  max?: number;
}

export function QualityDots({ level, max = 3 }: QualityDotsProps) {
  return (
    <span style={{ display: 'inline-flex', gap: 3 }}>
      {Array.from({ length: max }).map((_, i) => (
        <span
          key={i}
          style={{
            width: 5,
            height: 5,
            borderRadius: '50%',
            background: i < level ? 'var(--accent)' : 'var(--border-strong)',
          }}
        />
      ))}
    </span>
  );
}

interface OptionCardProps {
  active?: boolean;
  icon?: IconName;
  title: string;
  sub?: string;
  badge?: string;
  quality?: number;
  meta?: ReactNode;
  trailing?: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  /** [B21] Внутри radiogroup — role=radio + aria-checked (Settings/Onboarding). */
  radio?: boolean;
}

export function OptionCard({
  active,
  icon,
  title,
  sub,
  badge,
  quality,
  meta,
  trailing,
  onClick,
  disabled,
  radio,
}: OptionCardProps) {
  return (
    <button
      type="button"
      role={radio ? 'radio' : undefined}
      aria-checked={radio ? !!active : undefined}
      className="optioncard"
      data-active={active ? 'true' : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      {icon && (
        <span className="optioncard-ico">
          <Icon name={icon} size={17} />
        </span>
      )}
      <span className="optioncard-main">
        <span className="optioncard-head">
          {active && <Dot color="var(--ok)" />}
          <b>{title}</b>
          {badge && (
            <Chip size="sm" tone="accent">
              {badge}
            </Chip>
          )}
          {(active || trailing) && (
            <span style={{ marginLeft: 'auto', display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              {trailing}
              {active && <Icon name="check" size={15} style={{ color: 'var(--accent-text)' }} />}
            </span>
          )}
        </span>
        {sub && <span className="optioncard-sub">{sub}</span>}
        {(meta != null || quality != null) && (
          <span className="optioncard-meta">
            {quality != null && <QualityDots level={quality} />}
            {meta}
          </span>
        )}
      </span>
    </button>
  );
}
