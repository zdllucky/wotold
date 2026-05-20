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
    <div className="call-row" style={{ pointerEvents: 'none' }}>
      <Skeleton width="2rem" height="2rem" radius="var(--radius-pill)" />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 'var(--space-1)' }}>
        <Skeleton width="9rem" height="0.95em" />
        <Skeleton width="14rem" height="0.8em" />
      </div>
      <Skeleton width="3rem" height="0.8em" />
    </div>
  );
}
