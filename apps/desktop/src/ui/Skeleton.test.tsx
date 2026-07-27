// Tests for Skeleton.tsx — shimmer placeholders for loading states.
//
// Phase 5: reduced from 10 → 2 tests. Width/height/radius/inline/style
// passthrough были prop-to-DOM vanity, не testing поведения — удалены.
// Сохранены семантически значимые: smoke (renders) + aria-hidden=true
// (важно для screen readers — placeholder не объявляется как контент).
// Composite CallRowSkeleton — pointer-events:none — оставляем, это про
// non-interactivity invariant, не markup.

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { CallRowSkeleton, Skeleton } from './Skeleton';

afterEach(() => cleanup());

describe('Skeleton', () => {
  test('renders with aria-hidden="true" (decorative, not announced)', () => {
    const { container } = render(<Skeleton />);
    const el = container.querySelector('.skeleton')!;
    expect(el).toBeInTheDocument();
    expect(el).toHaveAttribute('aria-hidden', 'true');
  });
});

describe('CallRowSkeleton', () => {
  test('renders non-interactive (pointer-events:none) container of placeholders', () => {
    const { container } = render(<CallRowSkeleton />);
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper.style.pointerEvents).toBe('none');
    const skeletons = container.querySelectorAll('.skeleton');
    expect(skeletons.length).toBeGreaterThanOrEqual(4);
  });
});
