import { describe, expect, test } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Badge } from './Badge';
import { Pill } from './Pill';
import { StatusDot } from './StatusDot';
import { Empty } from './Empty';

describe('Badge/Pill/StatusDot/Empty', () => {
  test('Badge applies tone class', () => {
    render(<Badge tone="success">ok</Badge>);
    const el = screen.getByText('ok');
    expect(el.className).toContain('ds-badge--success');
  });

  test('Badge defaults to neutral tone', () => {
    render(<Badge>label</Badge>);
    expect(screen.getByText('label').className).toContain('ds-badge--neutral');
  });

  test('Pill applies tone class', () => {
    render(<Pill tone="danger">err</Pill>);
    expect(screen.getByText('err').className).toContain('ds-pill--danger');
  });

  test('StatusDot pulse adds pulse modifier', () => {
    const { container } = render(<StatusDot tone="danger" pulse />);
    const dot = container.querySelector('.ds-statusdot');
    expect(dot).not.toBeNull();
    expect(dot?.className).toContain('ds-statusdot--danger');
    expect(dot?.className).toContain('ds-statusdot--pulse');
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
    expect(container.querySelector('.ds-empty-title')).toBeNull();
    expect(container.querySelector('.ds-empty-icon')).toBeNull();
  });
});
