import { describe, expect, test, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Button } from './Button';

describe('Button', () => {
  test('renders children', () => {
    render(<Button>Click me</Button>);
    expect(screen.getByRole('button', { name: 'Click me' })).toBeInTheDocument();
  });

  test('default type is button', () => {
    render(<Button>x</Button>);
    expect(screen.getByRole('button')).toHaveAttribute('type', 'button');
  });

  // [B17] variant/size mapped to .btn--primary / .btn--lg.
  // wk.css sizes via [data-size]; 'md' default omits the attr.
  test('applies variant class + data-size', () => {
    render(
      <Button variant="primary" size="lg">
        Save
      </Button>,
    );
    const btn = screen.getByRole('button');
    expect(btn.className).toContain('btn--primary');
    expect(btn.getAttribute('data-size')).toBe('lg');
  });

  // [B17] `pill` prop is a legacy no-op — кнопки всегда radius-sm.
  // Сохраняем data-busy assertion.
  test('busy sets data-busy', () => {
    render(
      <Button busy pill>
        Loading
      </Button>,
    );
    const btn = screen.getByRole('button');
    expect(btn).toHaveAttribute('data-busy', 'true');
    expect(btn.className).toContain('btn');
  });

  test('disabled prop disables click', async () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        x
      </Button>,
    );
    await userEvent.click(screen.getByRole('button'));
    expect(onClick).not.toHaveBeenCalled();
  });

  test('fires onClick when enabled', async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>x</Button>);
    await userEvent.click(screen.getByRole('button'));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  test('renders leading/trailing decorations', () => {
    render(
      <Button leading={<span data-testid="lead" />} trailing={<span data-testid="trail" />}>
        Body
      </Button>,
    );
    expect(screen.getByTestId('lead')).toBeInTheDocument();
    expect(screen.getByTestId('trail')).toBeInTheDocument();
  });
});
