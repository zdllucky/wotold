// Tests for Skeleton.tsx — shimmer placeholders for loading states.

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { CallRowSkeleton, Skeleton } from './Skeleton';

afterEach(() => cleanup());

describe('Skeleton', () => {
  test('renders with ds-skeleton class', () => {
    const { container } = render(<Skeleton />);
    const el = container.querySelector('.ds-skeleton')!;
    expect(el).toBeInTheDocument();
  });

  test('aria-hidden="true" (decorative)', () => {
    const { container } = render(<Skeleton />);
    const el = container.querySelector('.ds-skeleton')!;
    expect(el).toHaveAttribute('aria-hidden', 'true');
  });

  test('default width is 100%', () => {
    const { container } = render(<Skeleton />);
    const el = container.querySelector('.ds-skeleton') as HTMLElement;
    expect(el.style.width).toBe('100%');
  });

  test('custom width and height are applied', () => {
    const { container } = render(<Skeleton width="8rem" height="2em" />);
    const el = container.querySelector('.ds-skeleton') as HTMLElement;
    expect(el.style.width).toBe('8rem');
    expect(el.style.height).toBe('2em');
  });

  test('custom radius is applied', () => {
    const { container } = render(<Skeleton radius="50%" />);
    const el = container.querySelector('.ds-skeleton') as HTMLElement;
    expect(el.style.borderRadius).toBe('50%');
  });

  test('inline=false renders block (data-inline=false)', () => {
    const { container } = render(<Skeleton inline={false} />);
    const el = container.querySelector('.ds-skeleton')!;
    expect(el).toHaveAttribute('data-inline', 'false');
  });

  test('inline=true sets data-inline=true', () => {
    const { container } = render(<Skeleton inline />);
    const el = container.querySelector('.ds-skeleton')!;
    expect(el).toHaveAttribute('data-inline', 'true');
  });

  test('extra style is merged', () => {
    const { container } = render(<Skeleton style={{ opacity: 0.5 }} />);
    const el = container.querySelector('.ds-skeleton') as HTMLElement;
    expect(el.style.opacity).toBe('0.5');
  });
});

describe('CallRowSkeleton', () => {
  test('renders multiple skeleton elements', () => {
    const { container } = render(<CallRowSkeleton />);
    const skeletons = container.querySelectorAll('.ds-skeleton');
    expect(skeletons.length).toBeGreaterThanOrEqual(4);
  });

  test('has no interactive elements (pointer-events:none container)', () => {
    const { container } = render(<CallRowSkeleton />);
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper.style.pointerEvents).toBe('none');
  });
});
