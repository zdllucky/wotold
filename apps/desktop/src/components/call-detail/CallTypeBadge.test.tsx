import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { CallTypeBadge } from './CallTypeBadge';

describe('CallTypeBadge', () => {
  test('renders null when callType is null', () => {
    const { container } = render(<CallTypeBadge callType={null} />);
    expect(container.firstChild).toBeNull();
  });

  test('renders null when callType is "other" with low confidence', () => {
    const { container } = render(
      <CallTypeBadge callType="other" confidence={0.3} />,
    );
    expect(container.firstChild).toBeNull();
  });

  test('renders chip with i18n label for sales_discovery', () => {
    const { container } = render(
      <CallTypeBadge callType="sales_discovery" confidence={0.92} />,
    );
    const chip = container.querySelector('.call-type-chip');
    expect(chip).toBeTruthy();
    expect(chip?.textContent).toContain('Discovery');
  });

  test('one_on_one uses warning dot variant (privacy-sensitive)', () => {
    const { container } = render(<CallTypeBadge callType="one_on_one" />);
    const dot = container.querySelector('.engine-chip-dot');
    expect(dot?.classList.contains('dot--warning')).toBe(true);
  });

  test('standup uses muted dot variant', () => {
    const { container } = render(<CallTypeBadge callType="standup" />);
    const dot = container.querySelector('.engine-chip-dot');
    expect(dot?.classList.contains('dot--muted')).toBe(true);
  });

  test('"other" with high confidence still renders generic chip', () => {
    const { container } = render(
      <CallTypeBadge callType="other" confidence={0.9} />,
    );
    expect(container.querySelector('.call-type-chip')).toBeTruthy();
  });
});
