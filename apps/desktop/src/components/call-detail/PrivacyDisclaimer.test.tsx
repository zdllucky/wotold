import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { PrivacyDisclaimer } from './PrivacyDisclaimer';

describe('PrivacyDisclaimer', () => {
  test('renders banner with title + body', () => {
    const { container } = render(<PrivacyDisclaimer />);
    const banner = container.querySelector('.privacy-disclaimer');
    expect(banner).toBeTruthy();
    expect(banner?.getAttribute('role')).toBe('note');
    expect(container.querySelector('.privacy-disclaimer-title')?.textContent).toContain('1:1');
    expect(container.querySelector('.privacy-disclaimer-body')?.textContent).toBeTruthy();
  });

  test('contains lock emoji in title', () => {
    const { container } = render(<PrivacyDisclaimer />);
    expect(container.textContent).toContain('🔒');
  });
});
