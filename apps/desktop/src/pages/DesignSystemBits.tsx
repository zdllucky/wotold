// [B17] Atomic showcase helpers for DesignSystemPage. Each is dumb-display:
// no own state, no business logic.

import type { CSSProperties, ReactNode } from 'react';

export function DSCard({
  children,
  style,
}: {
  children: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <div
      style={{
        background: 'var(--paper)',
        border: '1px solid var(--border)',
        borderRadius: 12,
        padding: '24px 28px',
        ...style,
      }}
    >
      {children}
    </div>
  );
}

interface DSSectionTitleProps {
  eyebrow: string;
  title: string;
  subtitle?: string;
}

export function DSSectionTitle({ eyebrow, title, subtitle }: DSSectionTitleProps) {
  return (
    <div style={{ marginBottom: 22 }}>
      <div className="eyebrow" style={{ marginBottom: 8 }}>
        {eyebrow}
      </div>
      <div className="title" style={{ fontSize: 28, marginBottom: 6 }}>
        {title}
      </div>
      {subtitle && (
        <div
          className="muted"
          style={{
            fontFamily: 'var(--font)',
            fontStyle: 'italic',
            fontSize: 14,
            maxWidth: 540,
            lineHeight: 1.5,
          }}
        >
          {subtitle}
        </div>
      )}
    </div>
  );
}

interface ColorSwatchProps {
  token: string;
  hex: string;
  fgVar?: boolean;
  sub?: string;
}

export function ColorSwatch({ token, hex, fgVar, sub }: ColorSwatchProps) {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 6,
        minWidth: 0,
      }}
    >
      <div
        style={{
          height: 72,
          borderRadius: 8,
          background: `var(--${token})`,
          border: '1px solid var(--border)',
          display: 'flex',
          alignItems: 'flex-end',
          padding: '8px 10px',
          color: fgVar ? 'var(--text)' : '#FFFFFF',
          fontFamily: 'var(--mono)',
          fontSize: 10.5,
          letterSpacing: '0.04em',
        }}
      >
        {hex}
      </div>
      <div
        className="mono"
        style={{
          fontSize: 11,
          color: 'var(--text)',
          letterSpacing: '0.02em',
        }}
      >
        --{token}
      </div>
      {sub && (
        <div className="muted" style={{ fontSize: 11 }}>
          {sub}
        </div>
      )}
    </div>
  );
}

interface TypeRowProps {
  label: string;
  size: string;
  sample: string;
  fam: string;
  s: number;
  w: number;
  ls: string;
  lh: number;
  italic?: boolean;
  upper?: boolean;
}

export function TypeRow({
  label,
  size,
  sample,
  fam,
  s,
  w,
  ls,
  lh,
  italic,
  upper,
}: TypeRowProps) {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '240px 1fr',
        gap: 28,
        alignItems: 'baseline',
        paddingBottom: 16,
        borderBottom: '1px solid var(--border-2)',
      }}
    >
      <div>
        <div className="small-caps" style={{ marginBottom: 4 }}>
          {label}
        </div>
        <div className="mono muted" style={{ fontSize: 10.5 }}>
          {size}
        </div>
      </div>
      <div
        style={{
          fontFamily: fam,
          fontSize: s,
          fontWeight: w,
          letterSpacing: ls,
          lineHeight: lh,
          color: 'var(--text)',
          fontStyle: italic ? 'italic' : 'normal',
          textTransform: upper ? 'uppercase' : 'none',
        }}
      >
        {sample}
      </div>
    </div>
  );
}
