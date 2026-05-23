import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { DecisionsBlock } from './DecisionsBlock';
import type { Decision } from '../../api/calls';

function decision(overrides: Partial<Decision> = {}): Decision {
  return {
    id: 'd1',
    call_id: 'c1',
    text: 'Lock enterprise tier at $499',
    evidence_quote: null,
    evidence_speaker: null,
    evidence_start_ms: null,
    evidence_end_ms: null,
    confidence: null,
    order_idx: 0,
    ...overrides,
  };
}

describe('DecisionsBlock', () => {
  test('renders null when empty', () => {
    const { container } = render(<DecisionsBlock decisions={[]} />);
    expect(container.firstChild).toBeNull();
  });

  test('renders section with title + items', () => {
    const { container } = render(
      <DecisionsBlock decisions={[decision(), decision({ id: 'd2', text: 'Launch beta' })]} />,
    );
    expect(container.querySelector('.decisions-block')).toBeTruthy();
    expect(container.querySelector('.v2-block-title')?.textContent).toContain('Решения');
    expect(container.querySelectorAll('.decision-row')).toHaveLength(2);
  });

  test('shows confidence badge when confidence < 0.7', () => {
    const { container } = render(
      <DecisionsBlock decisions={[decision({ confidence: 0.5 })]} />,
    );
    expect(container.querySelector('.confidence-low')).toBeTruthy();
  });

  test('hides confidence badge when confidence ≥ 0.7', () => {
    const { container } = render(
      <DecisionsBlock decisions={[decision({ confidence: 0.9 })]} />,
    );
    expect(container.querySelector('.confidence-low')).toBeNull();
  });

  test('shows EvidenceTooltip trigger when evidence_quote present', () => {
    const { container } = render(
      <DecisionsBlock
        decisions={[decision({ evidence_quote: 'we agreed on 499 dollars' })]}
      />,
    );
    expect(container.querySelector('.evidence-trigger')).toBeTruthy();
  });
});
