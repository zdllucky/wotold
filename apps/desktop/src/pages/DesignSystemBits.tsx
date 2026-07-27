// [B18.6c] Wotold v2 "uikit" showroom helpers for DesignSystemPage.
// Each is dumb-display: no own state, no business logic. Mirrors the prototype
// (docs/design/wotold-v2/_reference/wk-designsystem.jsx) DsSection / DsRow / Swatch.

import type { ReactNode } from 'react';

interface DsSectionProps {
  title: string;
  note?: string;
  children: ReactNode;
}

/** Section with eyebrow-style header + bottom border (.set-eyebrow / .set-display). */
export function DsSection({ title, note, children }: DsSectionProps) {
  return (
    <section style={{ marginBottom: 'var(--s7)' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'baseline',
          gap: 10,
          marginBottom: 16,
          paddingBottom: 8,
          borderBottom: '1px solid var(--border)',
        }}
      >
        <h2 className="set-display" style={{ fontSize: 'var(--t-18)', margin: 0 }}>
          {title}
        </h2>
        {note && (
          <span style={{ fontSize: 'var(--t-12)', color: 'var(--text-faint)' }}>{note}</span>
        )}
      </div>
      {children}
    </section>
  );
}

interface DsRowProps {
  label: string;
  children: ReactNode;
}

/** Labeled demo row: mono label left, flex-wrap controls right. */
export function DsRow({ label, children }: DsRowProps) {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '130px 1fr',
        gap: 16,
        alignItems: 'center',
        padding: '9px 0',
        borderBottom: '1px solid var(--border)',
      }}
    >
      <span
        className="mono"
        style={{ fontSize: 'var(--t-11)', color: 'var(--text-faint)' }}
      >
        {label}
      </span>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>
        {children}
      </div>
    </div>
  );
}

interface SwatchProps {
  varName: string;
  label: string;
}

/** 40px color box (background: var(<varName>)) + label + mono var name. */
export function Swatch({ varName, label }: SwatchProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 5, width: 70 }}>
      <div
        style={{
          height: 40,
          borderRadius: 'var(--r-sm)',
          background: `var(${varName})`,
          border: '1px solid var(--border)',
        }}
      />
      <div style={{ fontSize: 10, fontWeight: 600, color: 'var(--text)' }}>{label}</div>
      <div
        className="mono"
        style={{ fontSize: 9, color: 'var(--text-faint)' }}
      >
        {varName}
      </div>
    </div>
  );
}
