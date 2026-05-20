// @vitest-environment jsdom
import { describe, expect, test } from 'vitest';
import { render, screen } from '@testing-library/react';
import { UsageBar } from './UsageBar';

describe('UsageBar', () => {
  test('renders 0% bar при нулевом used', () => {
    render(<UsageBar label="STT" used={0} limit={3600} />);
    const bar = screen.getByRole('progressbar', { name: 'STT' });
    expect(bar.getAttribute('aria-valuenow')).toBe('0');
    expect(screen.getByText(/0 \/ 3 600/)).toBeTruthy();
  });

  test('clamps percentage at 100 when used > limit', () => {
    render(<UsageBar label="LLM" used={250_000} limit={200_000} />);
    const bar = screen.getByRole('progressbar', { name: 'LLM' });
    expect(bar.getAttribute('aria-valuenow')).toBe('100');
  });

  test('tone=danger при >=95%', () => {
    const { container } = render(<UsageBar label="STT" used={3500} limit={3600} />);
    const root = container.querySelector('.ds-usagebar');
    expect(root?.getAttribute('data-tone')).toBe('danger');
  });

  test('tone=warning при 75-94%', () => {
    const { container } = render(<UsageBar label="STT" used={2700} limit={3600} />);
    const root = container.querySelector('.ds-usagebar');
    expect(root?.getAttribute('data-tone')).toBe('warning');
  });

  test('tone=ok при <75%', () => {
    const { container } = render(<UsageBar label="STT" used={100} limit={3600} />);
    const root = container.querySelector('.ds-usagebar');
    expect(root?.getAttribute('data-tone')).toBe('ok');
  });

  test('displays ∞ when limit=0 (не настроен)', () => {
    render(<UsageBar label="STT" used={120} limit={0} />);
    expect(screen.getByText(/120 \/ ∞/)).toBeTruthy();
  });

  test('uses custom format function', () => {
    render(
      <UsageBar
        label="STT"
        used={120}
        limit={3600}
        format={(v) => `${v}s`}
      />,
    );
    expect(screen.getByText(/120s \/ 3600s/)).toBeTruthy();
  });
});
