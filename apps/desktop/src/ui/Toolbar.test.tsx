// Tests for Toolbar.tsx — page header with title, subtitle, and actions slot.

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { Toolbar } from './Toolbar';

afterEach(() => cleanup());

describe('Toolbar — title and subtitle', () => {
  test('renders title as h1 with title class', () => {
    render(<Toolbar title="My Page" />);
    const h1 = screen.getByRole('heading', { level: 1 });
    expect(h1).toHaveTextContent('My Page');
    expect(h1.className).toContain('title');
  });

  test('renders subtitle when provided', () => {
    render(<Toolbar title="Page" subtitle="sub text" />);
    expect(screen.getByText('sub text')).toBeInTheDocument();
  });

  test('no subtitle element when subtitle is omitted', () => {
    render(<Toolbar title="Page" />);
    // small-caps span should not be present
    const spans = document.querySelectorAll('.small-caps');
    expect(spans.length).toBe(0);
  });

  test('renders children when title is omitted', () => {
    render(<Toolbar><span data-testid="custom">custom</span></Toolbar>);
    expect(screen.getByTestId('custom')).toBeInTheDocument();
  });
});

describe('Toolbar — actions slot', () => {
  test('renders actions when provided', () => {
    render(<Toolbar title="T" actions={<button>Action</button>} />);
    expect(screen.getByRole('button', { name: 'Action' })).toBeInTheDocument();
  });

  test('no actions container when omitted', () => {
    const { container } = render(<Toolbar title="T" />);
    // Only one div inside Toolbar (title wrapper), no extra flex actions div
    const flexDivs = Array.from(container.querySelectorAll('div')).filter(
      (d) => d.style.display === 'flex' && d.style.flexShrink === '0',
    );
    expect(flexDivs.length).toBe(0);
  });
});

describe('Toolbar — sticky mode', () => {
  test('sticky=false has no position:sticky style', () => {
    const { container } = render(<Toolbar title="T" sticky={false} />);
    const root = container.firstChild as HTMLElement;
    expect(root.style.position).not.toBe('sticky');
  });

  test('sticky=true applies position:sticky', () => {
    const { container } = render(<Toolbar title="T" sticky />);
    const root = container.firstChild as HTMLElement;
    expect(root.style.position).toBe('sticky');
    expect(root.style.top).toBe('0px');
  });
});

describe('Toolbar — className and style passthrough', () => {
  test('forwards className to root element', () => {
    const { container } = render(<Toolbar className="custom-bar" />);
    expect(container.firstChild).toHaveClass('custom-bar');
  });

  test('merges custom style with toolbar layout style', () => {
    const { container } = render(<Toolbar style={{ gap: '999px' }} />);
    const root = container.firstChild as HTMLElement;
    // Gap is overridden by the style spread which comes after stickyStyle
    expect(root.style.gap).toBe('999px');
  });
});
