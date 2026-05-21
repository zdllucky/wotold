// Tests for Card.tsx — thin wrapper providing .card, .card--raised, .card--inset variants.

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { Card } from './Card';

afterEach(() => cleanup());

describe('Card — variants', () => {
  test('default variant renders with class "card"', () => {
    render(<Card>content</Card>);
    // The Card itself is the div containing "content"
    const card = screen.getByText('content').closest('div')!;
    expect(card.className).toContain('card');
    expect(card.className).not.toContain('card--inset');
    expect(card.className).not.toContain('card--raised');
  });

  test('sunken variant adds card--inset class', () => {
    render(<Card variant="sunken">inside</Card>);
    const card = screen.getByText('inside').closest('div')!;
    expect(card.className).toContain('card--inset');
  });

  test('raised variant adds card--raised class', () => {
    render(<Card variant="raised">raised</Card>);
    const card = screen.getByText('raised').closest('div')!;
    expect(card.className).toContain('card--raised');
  });

  test('compact prop adds inline padding style', () => {
    render(<Card compact>compact</Card>);
    const card = screen.getByText('compact').closest('div')!;
    expect(card.style.padding).toBeTruthy();
  });
});

// Phase 5: dropped vanity tests
// - "extra className is forwarded" — React {...rest} idiom, not behavior
// - "children render inside card" — React.children default, not behavior

describe('Card.Header', () => {
  test('renders children with flex layout', () => {
    render(
      <Card>
        <Card.Header data-testid="hdr">header text</Card.Header>
      </Card>,
    );
    const hdr = screen.getByTestId('hdr');
    expect(hdr).toBeInTheDocument();
    expect(hdr.style.display).toBe('flex');
  });
});

describe('Card.Title', () => {
  test('renders as h3 with title class', () => {
    render(
      <Card>
        <Card.Title>My Title</Card.Title>
      </Card>,
    );
    const h3 = screen.getByRole('heading', { level: 3 });
    expect(h3).toHaveTextContent('My Title');
    expect(h3.className).toContain('title');
  });
  // Phase 5: dropped "custom className is appended" — pure prop passthrough.
});
