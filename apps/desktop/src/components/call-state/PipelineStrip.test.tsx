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
  test('renders summary with stage label and percentage', () => {
    const { container } = render(<PipelineStrip progress={baseProgress} />);
    expect(container.querySelector('.proc-strip')).toBeTruthy();
    const summary = container.querySelector('.proc-strip-summary');
    expect(summary?.textContent).toContain('Распознаём речь');
    expect(container.querySelector('.proc-strip-pct')?.textContent).toBe('64%');
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

  test('active step shows caret + percent meta', () => {
    const { container } = render(
      <PipelineStrip progress={baseProgress} defaultOpen />,
    );
    const active = container.querySelector('.step--active');
    expect(active?.querySelector('.caret')).toBeTruthy();
    expect(active?.querySelector('.step-meta')?.textContent).toContain('64%');
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
