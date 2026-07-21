// [F3] RecapThinking — smoke RTL: лейблы по kind, состояния шагов,
// разворачиваемое превью, a11y (aria-live), пустой steps → null.

import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { RecapThinking } from './RecapThinking';
import type { RecapStepEvent } from '../../api/recording';

function step(over: Partial<RecapStepEvent>): RecapStepEvent {
  return {
    call_id: 'c1',
    step_idx: 0,
    total_steps: 5,
    kind: 'classify',
    status: 'done',
    chunk_no: null,
    chunk_total: null,
    preview: null,
    ...over,
  };
}

describe('RecapThinking', () => {
  it('renders nothing for empty steps', () => {
    const { container } = render(<RecapThinking steps={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders per-kind labels with refine chunk interpolation', () => {
    render(
      <RecapThinking
        steps={[
          step({ step_idx: 0, kind: 'classify', status: 'done' }),
          step({
            step_idx: 1,
            kind: 'refine',
            status: 'started',
            chunk_no: 1,
            chunk_total: 3,
          }),
        ]}
      />,
    );
    // ru/en fallback вне провайдера (locale-detect) — проверяем что chunk_no
    // и chunk_total интерполировались в refine-лейбл («часть 1 из 3»).
    expect(screen.getByText(/1.+3|3.+1/)).toBeTruthy();
    const labels = screen.getAllByTitle(/.+/);
    expect(labels.length).toBeGreaterThanOrEqual(2);
  });

  it('marks failed step with step--failed class', () => {
    const { container } = render(
      <RecapThinking
        steps={[
          step({
            step_idx: 1,
            kind: 'refine',
            status: 'failed',
            chunk_no: 2,
            chunk_total: 3,
          }),
        ]}
      />,
    );
    expect(container.querySelector('.step--failed')).toBeTruthy();
  });

  it('renders expandable preview with title and key points', () => {
    const { container } = render(
      <RecapThinking
        steps={[
          step({
            step_idx: 2,
            kind: 'refine',
            status: 'done',
            chunk_no: 2,
            chunk_total: 4,
            preview: { title: 'После части 2', key_points: ['точка А', 'точка Б'] },
          }),
        ]}
      />,
    );
    expect(container.querySelector('.recap-think-preview')).toBeTruthy();
    expect(screen.getByText('После части 2')).toBeTruthy();
    expect(screen.getByText('точка А')).toBeTruthy();
    expect(screen.getByText('точка Б')).toBeTruthy();
  });

  it('renders rotating chevron marker in the summary (inline style)', () => {
    const { container } = render(
      <RecapThinking steps={[step({ step_idx: 0, status: 'started' })]} />,
    );
    expect(container.querySelector('.recap-think-chevron')).toBeTruthy();
  });

  it('has aria-live polite on step list and counts done steps', () => {
    const { container } = render(
      <RecapThinking
        steps={[
          step({ step_idx: 0, status: 'done' }),
          step({ step_idx: 1, kind: 'generate', status: 'started' }),
        ]}
      />,
    );
    const live = container.querySelector('[aria-live="polite"]');
    expect(live).toBeTruthy();
    expect(screen.getByText('1 / 5')).toBeTruthy();
  });

  it('shows ellipsis for unknown total (total_steps=0)', () => {
    render(
      <RecapThinking
        steps={[step({ step_idx: 0, kind: 'classify', status: 'started', total_steps: 0 })]}
      />,
    );
    expect(screen.getByText('0 / …')).toBeTruthy();
  });
});
