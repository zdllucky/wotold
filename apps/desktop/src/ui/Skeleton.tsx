// [B16] DS Skeleton — заменяет голый <p>Загрузка…</p> на shimmer-стрипы.
// Использовать в loading-states pages, чтобы пользователь видел структуру
// контента ещё до загрузки.

import type { CSSProperties } from 'react';
import './ui.css';

interface SkeletonProps {
  /** ширина в CSS-юнитах (100% | 12rem | 60ch). По умолчанию 100%. */
  width?: string;
  /** высота. По умолчанию 1em (строка текста). */
  height?: string;
  /** скругление. По умолчанию --radius-md. */
  radius?: string;
  /** inline вместо block (для inline-skeleton рядом с другим текстом). */
  inline?: boolean;
  style?: CSSProperties;
}

export function Skeleton({
  width = '100%',
  height = '1em',
  radius,
  inline = false,
  style,
}: SkeletonProps) {
  return (
    <span
      className="ds-skeleton"
      data-inline={inline ? 'true' : 'false'}
      aria-hidden="true"
      style={{
        width,
        height,
        borderRadius: radius ?? 'var(--radius-md)',
        ...style,
      }}
    />
  );
}

/** Skeleton-строка списка звонков для CallsPage loading state. */
export function CallRowSkeleton() {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '110px 1fr 110px 60px',
        gap: 18,
        padding: '14px 0',
        borderTop: '1px solid var(--line-soft)',
        alignItems: 'baseline',
        pointerEvents: 'none',
      }}
    >
      <Skeleton width="3.5rem" height="0.9em" />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-1)',
        }}
      >
        <Skeleton width="14rem" height="1em" />
        <Skeleton width="8rem" height="0.7em" />
      </div>
      <Skeleton width="3rem" height="0.8em" />
      <Skeleton width="2.5rem" height="0.8em" style={{ marginLeft: 'auto' }} />
    </div>
  );
}
