import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { OpenQuestionsBlock } from './OpenQuestionsBlock';
import type { OpenQuestion } from '../../api/calls';

function openQ(overrides: Partial<OpenQuestion> = {}): OpenQuestion {
  return {
    id: 'q1',
    call_id: 'c1',
    text: 'Should we offer a trial?',
    raised_by: null,
    evidence_quote: null,
    evidence_speaker: null,
    evidence_start_ms: null,
    order_idx: 0,
    ...overrides,
  };
}

describe('OpenQuestionsBlock', () => {
  test('renders null when empty', () => {
    const { container } = render(<OpenQuestionsBlock openQuestions={[]} />);
    expect(container.firstChild).toBeNull();
  });

  test('renders section with title + items', () => {
    const { container } = render(
      <OpenQuestionsBlock
        openQuestions={[openQ(), openQ({ id: 'q2', text: 'SSO support?' })]}
      />,
    );
    expect(container.querySelector('.open-questions-block')).toBeTruthy();
    expect(container.querySelector('.v2-block-title')?.textContent).toContain('Открытые');
    expect(container.querySelectorAll('.open-question-row')).toHaveLength(2);
  });

  test('shows raisedBy chip when provided', () => {
    const { container } = render(
      <OpenQuestionsBlock openQuestions={[openQ({ raised_by: 'Bob' })]} />,
    );
    expect(container.textContent).toContain('Bob');
    expect(container.textContent).toContain('поднял'); // ru i18n
  });

  test('hides raisedBy chip when null', () => {
    const { container } = render(<OpenQuestionsBlock openQuestions={[openQ()]} />);
    expect(container.querySelector('.open-question-raised-by')).toBeNull();
  });

  test('shows EvidenceTooltip trigger when evidence_quote present', () => {
    const { container } = render(
      <OpenQuestionsBlock
        openQuestions={[openQ({ evidence_quote: 'we should think about trial' })]}
      />,
    );
    expect(container.querySelector('.evidence-trigger')).toBeTruthy();
  });
});
