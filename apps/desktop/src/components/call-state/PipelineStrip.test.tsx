import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { PipelineStrip } from './PipelineStrip';
import type { CallProgress } from '../../types/callState';
// useI18n() fallback ctx срабатывает без Provider'а; navigator.language
// пиннится в ru-RU в src/test/setup.ts, поэтому stage-метки/etaSec идут
// на русском. Никакого I18nProvider, чтобы не дёргать Tauri invoke в jsdom.

const baseProgress: CallProgress = {
  pct: 64,
  step: 3,
  stageLabel: 'Распознаём речь',
  etaSec: 25,
};

describe('PipelineStrip', () => {
  test('renders summary with stage label and macro progress bar', () => {
    const { container } = render(<PipelineStrip progress={baseProgress} />);
    expect(container.querySelector('.proc-strip')).toBeTruthy();
    const summary = container.querySelector('.proc-strip-summary');
    expect(summary?.textContent).toContain('Распознаём речь');
    // [V9] Real macro progress. baseProgress = step 3, pct 64
    // → ((3-1) + 0.64)/5 * 100 = 52.8% → rounded 53.
    expect(container.querySelector('.proc-strip-pct')?.textContent).toBe('53%');
    const fill = container.querySelector(
      '.proc-strip-rail .progress-rail-fill',
    ) as HTMLElement | null;
    expect(fill?.style.width).toBe('53%');
  });

  test('renders 5 step rows in expanded body', () => {
    const { container } = render(
      <PipelineStrip progress={baseProgress} defaultOpen />,
    );
    const steps = container.querySelectorAll('.step');
    expect(steps.length).toBe(5);
  });

  test('step states: done < step < pending', () => {
    const { container } = render(
      <PipelineStrip progress={baseProgress} defaultOpen />,
    );
    const steps = container.querySelectorAll('.step');
    // step 1, 2 = done; step 3 = active; step 4, 5 = pending
    expect(steps[0]?.classList.contains('step--done')).toBe(true);
    expect(steps[1]?.classList.contains('step--done')).toBe(true);
    expect(steps[2]?.classList.contains('step--active')).toBe(true);
    expect(steps[3]?.classList.contains('step--pending')).toBe(true);
    expect(steps[4]?.classList.contains('step--pending')).toBe(true);
  });

  test('active step shows caret + shimmer fake loader', () => {
    const { container } = render(
      <PipelineStrip progress={baseProgress} defaultOpen />,
    );
    const active = container.querySelector('.step--active');
    expect(active?.querySelector('.caret')).toBeTruthy();
    // [V6.9] No within-step %, instead `.step-shimmer` fake loader
    // (browser-style indeterminate animation).
    expect(active?.querySelector('.step-shimmer')).toBeTruthy();
  });

  test('done steps show ✓ checkmark', () => {
    const { container } = render(
      <PipelineStrip progress={baseProgress} defaultOpen />,
    );
    const done = container.querySelectorAll('.step--done');
    expect(done.length).toBe(2);
    done.forEach((step) => {
      expect(step.querySelector('.step-bullet')?.textContent).toBe('✓');
    });
  });

  test('etaSec rendered when provided', () => {
    const { container } = render(<PipelineStrip progress={baseProgress} />);
    expect(container.querySelector('.proc-strip-summary')?.textContent).toMatch(
      /25\s*сек/,
    );
  });

  test('etaSec omitted when undefined', () => {
    const noEta: CallProgress = { ...baseProgress, etaSec: undefined };
    const { container } = render(<PipelineStrip progress={noEta} />);
    expect(
      container.querySelector('.proc-strip-summary')?.textContent,
    ).not.toMatch(/~\d+/);
  });
});
