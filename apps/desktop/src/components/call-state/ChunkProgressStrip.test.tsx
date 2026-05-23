import { describe, expect, test } from 'vitest';
import { render } from '@testing-library/react';

import { ChunkProgressStrip } from './ChunkProgressStrip';
import type { CallChunk } from '../../api/recording';
// useI18n() fallback context работает без Provider (см. PipelineStrip.test.tsx);
// navigator.language пиннится в ru-RU в src/test/setup.ts → строки в ru.

function chunk(idx: number, status: CallChunk['status'], startMs = idx * 600_000): CallChunk {
  return {
    chunk_idx: idx,
    status,
    start_ms: startMs,
    end_ms: status === 'pending' || status === 'processing' ? null : startMs + 600_000,
  };
}

describe('ChunkProgressStrip', () => {
  test('renders null when chunks list is empty', () => {
    const { container } = render(<ChunkProgressStrip chunks={[]} />);
    expect(container.querySelector('.proc-strip')).toBeNull();
  });

  test('macro progress reflects done / total ratio', () => {
    const chunks = [
      chunk(0, 'done'),
      chunk(1, 'done'),
      chunk(2, 'processing'),
      chunk(3, 'pending'),
    ];
    const { container } = render(<ChunkProgressStrip chunks={chunks} />);
    // 2 done / 4 total = 50%.
    expect(container.querySelector('.proc-strip-pct')?.textContent).toBe('50%');
    const fill = container.querySelector(
      '.proc-strip-rail .rail-fill',
    ) as HTMLElement | null;
    expect(fill?.style.width).toBe('50%');
  });

  test('summary detail badge counts done/total + failed marker', () => {
    const chunks = [
      chunk(0, 'done'),
      chunk(1, 'failed'),
      chunk(2, 'pending'),
    ];
    const { container } = render(<ChunkProgressStrip chunks={chunks} />);
    const summary = container.querySelector('.proc-strip-summary')?.textContent ?? '';
    // detail слот рендерит «1 / 3 · 1 ✗» (CallStateTag).
    expect(summary).toContain('1 / 3');
    expect(summary).toContain('1 ✗');
  });

  test('body rows render correct bullet classes per status', () => {
    const chunks = [
      chunk(0, 'done'),
      chunk(1, 'processing'),
      chunk(2, 'failed'),
      chunk(3, 'pending'),
    ];
    const { container } = render(
      <ChunkProgressStrip chunks={chunks} defaultOpen />,
    );
    const steps = container.querySelectorAll('.step');
    expect(steps.length).toBe(4);
    expect(steps[0]?.classList.contains('step--done')).toBe(true);
    expect(steps[1]?.classList.contains('step--active')).toBe(true);
    expect(steps[2]?.classList.contains('step--failed')).toBe(true);
    expect(steps[3]?.classList.contains('step--pending')).toBe(true);
    // Active step имеет shimmer fake loader (как у PipelineStrip).
    expect(steps[1]?.querySelector('.step-shimmer')).toBeTruthy();
  });

  test('time range mm:ss—mm:ss rendered for each chunk', () => {
    const chunks = [
      chunk(0, 'done'), // 0:00—10:00
      chunk(1, 'processing'), // 10:00—…
    ];
    const { container } = render(
      <ChunkProgressStrip chunks={chunks} defaultOpen />,
    );
    const labels = container.querySelectorAll('.step-label-text');
    expect(labels[0]?.textContent).toBe('0:00—10:00');
    // Processing chunk без end_ms → trailing ellipsis-ish.
    expect(labels[1]?.textContent).toBe('10:00—…');
  });
});
