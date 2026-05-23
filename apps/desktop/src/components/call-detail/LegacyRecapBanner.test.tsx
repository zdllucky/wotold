import { describe, expect, test, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { LegacyRecapBanner } from './LegacyRecapBanner';

describe('LegacyRecapBanner', () => {
  test('renders title + hint + button in idle state', () => {
    const { container } = render(<LegacyRecapBanner busy={false} onUpgrade={() => {}} />);
    const banner = container.querySelector('.activity-strip.legacy-recap-banner');
    expect(banner).toBeTruthy();
    const button = screen.getByRole('button');
    expect(button).toBeTruthy();
    expect(button.getAttribute('disabled')).toBeNull();
    expect(button.getAttribute('aria-busy')).toBe('false');
  });

  test('click fires onUpgrade', () => {
    const onUpgrade = vi.fn();
    render(<LegacyRecapBanner busy={false} onUpgrade={onUpgrade} />);
    fireEvent.click(screen.getByRole('button'));
    expect(onUpgrade).toHaveBeenCalledTimes(1);
  });

  test('busy=true disables button + flips label + sets aria-busy', () => {
    render(<LegacyRecapBanner busy={true} onUpgrade={() => {}} />);
    const button = screen.getByRole('button');
    expect(button.getAttribute('disabled')).not.toBeNull();
    expect(button.getAttribute('aria-busy')).toBe('true');
  });

  test('busy=true: click does not fire onUpgrade', () => {
    const onUpgrade = vi.fn();
    render(<LegacyRecapBanner busy={true} onUpgrade={onUpgrade} />);
    fireEvent.click(screen.getByRole('button'));
    expect(onUpgrade).not.toHaveBeenCalled();
  });
});
