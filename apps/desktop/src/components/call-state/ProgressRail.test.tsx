import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { ProgressRail } from './ProgressRail';

describe('ProgressRail', () => {
  test('determinate sets aria-valuenow + width', () => {
    render(<ProgressRail pct={42} ariaLabel="progress" />);
    const rail = document.querySelector('.progress-rail');
    const fill = document.querySelector('.progress-rail-fill') as HTMLElement | null;
    expect(rail?.getAttribute('aria-valuenow')).toBe('42');
    expect(rail?.getAttribute('aria-valuemax')).toBe('100');
    expect(rail?.getAttribute('aria-label')).toBe('progress');
    expect(fill?.style.width).toBe('42%');
  });

  test('clamps values <0 and >100', () => {
    const { rerender } = render(<ProgressRail pct={-50} />);
    expect(
      (document.querySelector('.progress-rail-fill') as HTMLElement | null)?.style.width,
    ).toBe('0%');
    rerender(<ProgressRail pct={150} />);
    expect(
      (document.querySelector('.progress-rail-fill') as HTMLElement | null)?.style.width,
    ).toBe('100%');
  });

  test('indeterminate adds modifier + uses aria-valuetext', () => {
    render(<ProgressRail indeterminate ariaLabel="processing" />);
    const rail = document.querySelector('.progress-rail');
    expect(rail?.classList.contains('progress-rail--indeterminate')).toBe(true);
    expect(rail?.getAttribute('aria-valuetext')).toBe('processing');
    // aria-valuenow не используется в indeterminate
    expect(rail?.getAttribute('aria-valuenow')).toBeNull();
  });

  test('defaults missing pct to 0', () => {
    render(<ProgressRail />);
    expect(
      (document.querySelector('.progress-rail-fill') as HTMLElement | null)?.style.width,
    ).toBe('0%');
  });
});
