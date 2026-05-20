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

  test('applies variant + size classes', () => {
    render(
      <Button variant="primary" size="lg">
        Save
      </Button>,
    );
    const btn = screen.getByRole('button');
    expect(btn.className).toContain('ds-button--variant-primary');
    expect(btn.className).toContain('ds-button--size-lg');
  });

  test('busy sets data-busy and pill adds modifier', () => {
    render(
      <Button busy pill>
        Loading
      </Button>,
    );
    const btn = screen.getByRole('button');
    expect(btn).toHaveAttribute('data-busy', 'true');
    expect(btn.className).toContain('ds-button--pill');
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
