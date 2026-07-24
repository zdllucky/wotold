/* eslint-disable */
// WOTOLD UIKIT · components — every screen composes ONLY from these.
const { useState, useEffect, useRef, useCallback, createContext, useContext } = React;

// ── Button ──
function Btn({ variant = 'default', size, block, icon, iconRight, children, ...p }) {
  return (
    <button className={`btn btn--${variant}`} data-size={size} data-block={block ? 'true' : undefined} {...p}>
      {icon && <Icon name={icon} size={size === 'sm' ? 14 : 15} />}
      {children}
      {iconRight && <Icon name={iconRight} size={size === 'sm' ? 14 : 15} />}
    </button>
  );
}

// ── IconButton (optional tooltip) ──
function IconBtn({ icon, label, size, active, tip, tipSide, iconSize, ...p }) {
  const s = iconSize || (size === 'sm' ? 15 : size === 'lg' ? 18 : 16);
  return (
    <button className={`iconbtn${tip ? ' tip' : ''}${tip && tipSide === 'right' ? ' tip--right' : ''}`} data-size={size} data-active={active ? 'true' : undefined}
      data-tip={tip} aria-label={label || tip} {...p}>
      <Icon name={icon} size={s} />
    </button>
  );
}

// ── Chip ──
function Chip({ tone = 'neutral', icon, size, children, ...p }) {
  const Tag = p.onClick ? 'button' : 'span';
  const cls = tone === 'neutral' ? '' : `chip--${tone}`;
  return (
    <Tag className={`chip ${cls}`} data-size={size} {...p}>
      {icon && <Icon name={icon} size={size === 'sm' ? 11 : 12} />}
      {children}
    </Tag>
  );
}

// ── Avatar ──
function initials(name) {
  if (!name) return '?';
  if (name === 'Вы') return 'Вы';
  const parts = name.trim().split(/\s+/);
  return (parts[0][0] + (parts[1] ? parts[1][0] : '')).toUpperCase();
}
function Avatar({ name, color = 'var(--sp1)', size = 22, square }) {
  return (
    <span className="avatar" style={{ background: color, width: size, height: size, fontSize: Math.round(size * 0.42), borderRadius: square ? 'var(--r-xs)' : undefined }} title={name}>
      {initials(name)}
    </span>
  );
}
function AvatarGroup({ items, size = 20, max = 4, on }) {
  const shown = items.slice(0, max);
  const extra = items.length - shown.length;
  return (
    <span className="avatar-grp" data-on={on}>
      {shown.map((it, i) => <Avatar key={i} name={it.name} color={it.color} size={size} />)}
      {extra > 0 && <span className="avatar" style={{ width: size, height: size, fontSize: Math.round(size * 0.4), background: 'var(--sunken)', color: 'var(--text-3)' }}>+{extra}</span>}
    </span>
  );
}

// ── Dot ──
function Dot({ color = 'var(--accent)', ring, pulse, size = 8 }) {
  return <span className={`dot${ring ? ' dot--ring' : ''}${pulse ? ' dot--pulse' : ''}`} style={{ width: size, height: size, color, background: ring ? 'transparent' : color }} />;
}

// ── Kbd ──
function Kbd({ children }) { return <span className="kbd">{children}</span>; }

// ── Input ──
function Input({ icon, value, onChange, size, rightSlot, inputRef, ...p }) {
  return (
    <label className="input" data-size={size}>
      {icon && <Icon name={icon} size={15} className="iico" />}
      <input ref={inputRef} value={value} onChange={onChange} {...p} />
      {rightSlot}
    </label>
  );
}
function Textarea({ value, onChange, rows = 3, ...p }) {
  return <textarea className="input" value={value} onChange={onChange} rows={rows} {...p} />;
}

// ── Field ──
function Field({ label, hint, children }) {
  return (
    <label className="field">
      {label && <span className="field-label">{label}</span>}
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  );
}

// ── Segmented ──
function Segmented({ options, value, onChange, size }) {
  return (
    <div className="seg" data-size={size}>
      {options.map((o) => (
        <button key={o.value} data-active={value === o.value} onClick={() => onChange(o.value)}>
          {o.icon && <Icon name={o.icon} size={14} />}{o.label}
        </button>
      ))}
    </div>
  );
}

// ── Switch ──
function Switch({ checked, onChange }) {
  return <button className="switch" data-on={checked ? 'true' : 'false'} role="switch" aria-checked={checked} onClick={() => onChange(!checked)} />;
}

// ── Tabs ──
function Tabs({ tabs, value, onChange }) {
  return (
    <div className="tabs">
      {tabs.map((t) => (
        <button key={t.value} className="tab" data-active={value === t.value} onClick={() => onChange(t.value)}>
          {t.icon && <Icon name={t.icon} size={14} />}{t.label}
          {t.count != null && <span className="tab-count">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

// ── Nav ──
function NavItem({ icon, label, active, meta, onClick, leading }) {
  return (
    <button className="navitem" data-active={active ? 'true' : undefined} onClick={onClick}>
      {leading || (icon && <span className="nav-ico"><Icon name={icon} size={16} /></span>)}
      <span className="nav-label">{label}</span>
      {meta != null && <span className="nav-meta">{meta}</span>}
    </button>
  );
}
function SecLabel({ children, action }) {
  return <div className="sec-label"><span>{children}</span>{action}</div>;
}

// ── Dropdown / Menu ──
function Dropdown({ trigger, children, align = 'left', up, width = 220, menuStyle, block }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  useEffect(() => {
    if (!open) return;
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const k = (e) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', h); document.addEventListener('keydown', k);
    return () => { document.removeEventListener('mousedown', h); document.removeEventListener('keydown', k); };
  }, [open]);
  const pos = { width, [up ? 'bottom' : 'top']: 'calc(100% + 6px)', [align]: 0 };
  return (
    <span style={{ position: 'relative', display: block ? 'flex' : 'inline-flex', width: block ? '100%' : undefined, minWidth: block ? 0 : undefined }} ref={ref}>
      {trigger({ open, toggle: () => setOpen((o) => !o), close: () => setOpen(false) })}
      {open && (
        <div className="menu fade" style={{ ...pos, ...menuStyle }} onClick={() => setTimeout(() => setOpen(false), 0)}>
          {children}
        </div>
      )}
    </span>
  );
}
function MenuItem({ icon, children, end, danger, active, onClick }) {
  return (
    <button className={`menu-item${danger ? ' menu-item--danger' : ''}`} data-active={active ? 'true' : undefined} onClick={onClick}>
      {icon && <span className="mi-ico"><Icon name={icon} size={15} /></span>}
      <span style={{ flex: 1 }}>{children}</span>
      {end && <span className="mi-end">{end}</span>}
    </button>
  );
}
function MenuSep() { return <div className="menu-sep" />; }
function MenuLabel({ children }) { return <div className="menu-label">{children}</div>; }

// ── Modal ──
function Modal({ open, onClose, title, children, footer, width }) {
  useEffect(() => {
    if (!open) return;
    const k = (e) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', k);
    return () => document.removeEventListener('keydown', k);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="overlay fade" onMouseDown={onClose}>
      <div className="modal fade-up" style={width ? { width } : null} onMouseDown={(e) => e.stopPropagation()}>
        {title && <div className="modal-head"><div className="modal-title">{title}</div></div>}
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

// ── Panel ──
function Panel({ raised, children, style, className = '', ...p }) {
  return <div className={`panel${raised ? ' panel--raised' : ''} ${className}`} style={style} {...p}>{children}</div>;
}

// ── Progress ──
function Progress({ value }) { return <div className="progress"><i style={{ width: `${Math.max(0, Math.min(100, value))}%` }} /></div>; }

// ── Empty ──
function Empty({ icon, title, desc, action }) {
  return (
    <div className="empty">
      {icon && <div className="empty-ico"><Icon name={icon} size={22} /></div>}
      <div className="empty-title">{title}</div>
      {desc && <div style={{ maxWidth: 320 }}>{desc}</div>}
      {action && <div style={{ marginTop: 4 }}>{action}</div>}
    </div>
  );
}

// ── Wave ──
function Wave({ bars = 5, color = 'currentColor', height = 18 }) {
  return <span className="wave" style={{ color, height }}>{Array.from({ length: bars }).map((_, i) => <i key={i} style={{ animationDelay: `${i * 0.1}s` }} />)}</span>;
}

// ── Select ──
function Select({ value, options, onChange, width = 240, placeholder, disabled }) {
  const cur = options.find((o) => o.value === value);
  return (
    <div style={{ width, maxWidth: '100%' }}>
      <Dropdown block width={width} trigger={({ toggle }) => (
        <button className="select-trigger" onClick={toggle} disabled={disabled} style={{ opacity: disabled ? .5 : 1 }}>
          <span className="u-trunc" style={{ flex: 1, textAlign: 'left', color: cur ? 'var(--text)' : 'var(--text-faint)' }}>{cur ? cur.label : (placeholder || 'Выбрать…')}</span>
          <Icon name="chevronUpDown" size={14} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />
        </button>
      )}>
        {options.map((o) => <MenuItem key={o.value} active={o.value === value} onClick={() => onChange(o.value)} end={o.value === value ? <Icon name="check" size={14} /> : null}>{o.label}</MenuItem>)}
      </Dropdown>
    </div>
  );
}

// ── QualityDots ──
function QualityDots({ level, max = 3 }) {
  return <span style={{ display: 'inline-flex', gap: 3 }}>{Array.from({ length: max }).map((_, i) => <span key={i} style={{ width: 5, height: 5, borderRadius: '50%', background: i < level ? 'var(--accent)' : 'var(--border-strong)' }} />)}</span>;
}

// ── OptionCard (selectable radio card) ──
function OptionCard({ active, icon, title, sub, badge, quality, meta, trailing, onClick, disabled }) {
  return (
    <button onClick={onClick} disabled={disabled} className="optioncard" data-active={active ? 'true' : undefined}>
      {icon && <span className="optioncard-ico"><Icon name={icon} size={17} /></span>}
      <span className="optioncard-main">
        <span className="optioncard-head">
          {active && <Dot color="var(--ok)" />}
          <b>{title}</b>
          {badge && <Chip size="sm" tone="accent">{badge}</Chip>}
          {(active || trailing) && <span style={{ marginLeft: 'auto', display: 'inline-flex', alignItems: 'center', gap: 6 }}>{trailing}{active && <Icon name="check" size={15} style={{ color: 'var(--accent-text)' }} />}</span>}
        </span>
        {sub && <span className="optioncard-sub">{sub}</span>}
        {(meta != null || quality != null) && <span className="optioncard-meta">{quality != null && <QualityDots level={quality} />}{meta}</span>}
      </span>
    </button>
  );
}

// ── SettingRow (label + hint + trailing control) ──
function SettingRow({ label, hint, control, disabled, children }) {
  return (
    <div className="setting-row" style={{ opacity: disabled ? .6 : 1 }}>
      <div className="setting-row-text">
        <div className="setting-row-label">{label}</div>
        {hint && <div className="field-hint" style={{ marginTop: 5 }}>{hint}</div>}
      </div>
      {control || children}
    </div>
  );
}

Object.assign(window, {
  Btn, IconBtn, Chip, Avatar, AvatarGroup, Dot, Kbd, Input, Textarea, Field,
  Segmented, Switch, Tabs, NavItem, SecLabel, Dropdown, MenuItem, MenuSep, MenuLabel,
  Modal, Panel, Progress, Empty, Wave, initials,
  Select, QualityDots, OptionCard, SettingRow,
});
