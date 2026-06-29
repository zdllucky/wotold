// [B18.6c] Wotold v2 uikit — dropdown menu (.menu/.menu-item/.menu-label/.menu-sep from wk.css).

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Icon, type IconName } from './Icon';
import { useAnchoredPosition } from './useAnchoredPosition';

interface DropdownApi {
  open: boolean;
  toggle: () => void;
  close: () => void;
}

interface DropdownProps {
  trigger: (api: DropdownApi) => ReactNode;
  children: ReactNode;
  align?: 'left' | 'right';
  up?: boolean;
  width?: number;
  block?: boolean;
}

export function Dropdown({ trigger, children, align, up, width, block }: DropdownProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const place = useAnchoredPosition(open, ref, panelRef, up);
  const toggle = () => setOpen((v) => !v);
  const close = () => setOpen(false);

  useEffect(() => {
    if (!open) return;
    function handlePointer(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') close();
    }
    document.addEventListener('mousedown', handlePointer);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handlePointer);
      document.removeEventListener('keydown', handleKey);
    };
  }, [open]);

  const pos: CSSProperties = {
    width,
    [place.up ? 'bottom' : 'top']: 'calc(100% + 6px)',
    [align ?? 'left']: 0,
    ...(place.shiftX ? { transform: `translateX(${place.shiftX}px)` } : null),
    ...(place.maxHeight ? { maxHeight: place.maxHeight, overflowY: 'auto' } : null),
  };

  return (
    <span style={{ position: 'relative', display: block ? 'flex' : 'inline-flex' }} ref={ref}>
      {trigger({ open, toggle, close })}
      {open && (
        <div
          ref={panelRef}
          className="menu fade"
          style={{ position: 'absolute', zIndex: 60, ...pos }}
          onClick={() => close()}
        >
          {children}
        </div>
      )}
    </span>
  );
}

interface MenuItemProps {
  icon?: IconName;
  children: ReactNode;
  end?: ReactNode;
  danger?: boolean;
  active?: boolean;
  disabled?: boolean;
  title?: string;
  onClick?: () => void;
}

export function MenuItem({
  icon,
  children,
  end,
  danger,
  active,
  disabled,
  title,
  onClick,
}: MenuItemProps) {
  return (
    <button
      type="button"
      className={'menu-item' + (danger ? ' menu-item--danger' : '')}
      data-active={active ? 'true' : undefined}
      disabled={disabled}
      title={title}
      style={disabled ? { opacity: 0.45, cursor: 'not-allowed' } : undefined}
      onClick={onClick}
    >
      {icon && (
        <span className="mi-ico">
          <Icon name={icon} size={15} />
        </span>
      )}
      <span style={{ flex: 1 }}>{children}</span>
      {end && <span className="mi-end">{end}</span>}
    </button>
  );
}

interface MenuLabelProps {
  children: ReactNode;
}

export function MenuLabel({ children }: MenuLabelProps) {
  return <div className="menu-label">{children}</div>;
}

export function MenuSep() {
  return <div className="menu-sep" />;
}
