import { describe, expect, test } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Badge } from './Badge';
import { Pill } from './Pill';
import { StatusDot } from './StatusDot';
import { Empty } from './Empty';

// [B17] Atelier v2 — Badge/Pill теперь рендерят inline-styled span с
// token-vars; tone проявляется через background/color CSS-vars (--accent,
// --signal etc), а не через class-suffix. Тестируем поведение, не markup.

describe('Badge/Pill/StatusDot/Empty', () => {
  test('Badge renders content', () => {
    render(<Badge tone="success">ok</Badge>);
    expect(screen.getByText('ok')).toBeInTheDocument();
  });

  test('Badge defaults to neutral tone', () => {
    render(<Badge>label</Badge>);
    const el = screen.getByText('label');
    // neutral tone uses --bg-2 background.
    expect(el.getAttribute('style') ?? '').toMatch(/bg-2|var\(--bg-2\)/);
  });

  test('Pill renders content', () => {
    render(<Pill tone="danger">err</Pill>);
    expect(screen.getByText('err')).toBeInTheDocument();
  });

  test('StatusDot pulse adds pulse class', () => {
    const { container } = render(<StatusDot tone="danger" pulse />);
    const dot = container.querySelector('.dot');
    expect(dot).not.toBeNull();
    expect(dot?.className).toContain('dot--pulse');
  });

  test('Empty renders icon/title/description/action', () => {
    render(
      <Empty
        icon={<span data-testid="icon" />}
        title="Nothing"
        description="Add stuff"
        action={<button>Add</button>}
      />,
    );
    expect(screen.getByTestId('icon')).toBeInTheDocument();
    expect(screen.getByText('Nothing')).toBeInTheDocument();
    expect(screen.getByText('Add stuff')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add' })).toBeInTheDocument();
  });

  test('Empty omits sections that are not provided', () => {
    const { container } = render(<Empty description="only desc" />);
    expect(screen.getByText('only desc')).toBeInTheDocument();
    // Empty no longer emits default emoji icon nor a placeholder title element.
    expect(screen.queryByText(/Nothing/i)).toBeNull();
    // Root element exists with .empty class.
    expect(container.querySelector('.empty')).not.toBeNull();
  });
});
