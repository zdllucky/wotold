// [P11.2] ChunkFailureAccordion — рендерится только при failed chunks,
// collapsed by default, retry button optimistically переключается.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { ChunkFailureAccordion } from './ChunkFailureAccordion';
import type { CallChunk } from '../../api/recording';

afterEach(() => cleanup());
// useI18n() fallback (no Provider) → setup.ts pin'нит navigator.language=ru-RU.

function chunk(idx: number, status: CallChunk['status']): CallChunk {
  return {
    chunk_idx: idx,
    status,
    start_ms: idx * 600_000,
    end_ms: status === 'pending' || status === 'processing' ? null : (idx + 1) * 600_000,
  };
}

describe('ChunkFailureAccordion', () => {
  it('renders null when no failed chunks', () => {
    const { container } = render(
      <ChunkFailureAccordion
        chunks={[chunk(0, 'done'), chunk(1, 'done')]}
        onRetryChunk={() => {}}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders accordion summary with failed count', () => {
    const { container } = render(
      <ChunkFailureAccordion
        chunks={[chunk(0, 'done'), chunk(1, 'failed')]}
        onRetryChunk={() => {}}
      />,
    );
    const summary = container.querySelector('summary');
    expect(summary?.textContent ?? '').toMatch(/не удалось распознать сегменты/i);
    expect(summary?.textContent ?? '').toMatch(/1 \/ 2/);
  });

  it('shows retry button when expanded', () => {
    const { container } = render(
      <ChunkFailureAccordion
        chunks={[chunk(0, 'done'), chunk(1, 'failed')]}
        onRetryChunk={() => {}}
        defaultOpen
      />,
    );
    const buttons = container.querySelectorAll('button');
    expect(buttons.length).toBe(1);
    expect(buttons[0]?.textContent ?? '').toMatch(/повторить/i);
  });

  it('calls onRetryChunk with chunk_idx on retry click', () => {
    const onRetry = vi.fn();
    const { container } = render(
      <ChunkFailureAccordion
        chunks={[chunk(0, 'failed'), chunk(1, 'failed')]}
        onRetryChunk={onRetry}
        defaultOpen
      />,
    );
    const buttons = container.querySelectorAll('button');
    const secondButton = buttons[1];
    if (!secondButton) throw new Error('expected 2 retry buttons');
    fireEvent.click(secondButton);
    expect(onRetry).toHaveBeenCalledWith(1);
  });

  it('switches retry button to retrying state optimistically', () => {
    const { container } = render(
      <ChunkFailureAccordion
        chunks={[chunk(0, 'failed')]}
        onRetryChunk={vi.fn()}
        defaultOpen
      />,
    );
    const button = container.querySelector('button')!;
    fireEvent.click(button);
    expect(button.textContent ?? '').toMatch(/повторяем/i);
    expect(button.hasAttribute('disabled')).toBe(true);
  });
});
