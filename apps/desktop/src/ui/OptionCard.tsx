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
  // [B21.6] Roving tabindex. Внутри radiogroup табом входят в группу один раз —
  // на выбранный вариант, — а переключаются стрелками. Пока все карточки были
  // обычными кнопками, Tab обходил каждую: на трёх пресетах это три остановки
  // вместо одной, и клавиатурный пользователь не мог понять, что это один
  // выбор из набора, а не три независимые кнопки.
  const onRadioKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (!radio) return;
    const forward = e.key === 'ArrowRight' || e.key === 'ArrowDown';
    const back = e.key === 'ArrowLeft' || e.key === 'ArrowUp';
    if (!forward && !back) return;
    const group = e.currentTarget.closest('[role="radiogroup"]');
    if (!group) return;
    const items = Array.from(
      group.querySelectorAll<HTMLButtonElement>('[role="radio"]:not([disabled])'),
    );
    const from = items.indexOf(e.currentTarget);
    if (from < 0 || items.length < 2) return;
    e.preventDefault();
    // По паттерну WAI-ARIA стрелка и перемещает фокус, и выбирает вариант.
    const next = items[(from + (forward ? 1 : -1) + items.length) % items.length];
    next?.focus();
    next?.click();
  };

  return (
    <button
      type="button"
      role={radio ? 'radio' : undefined}
      aria-checked={radio ? !!active : undefined}
      tabIndex={radio && !active ? -1 : undefined}
      className="optioncard"
      data-active={active ? 'true' : undefined}
      disabled={disabled}
      onClick={onClick}
      onKeyDown={radio ? onRadioKeyDown : undefined}
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
