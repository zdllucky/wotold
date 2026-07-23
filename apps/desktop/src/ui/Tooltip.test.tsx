// [B27.5] Tooltip: hover с задержкой, портал в body, focus-показ, Esc/leave.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';

import { Tooltip } from './Tooltip';

describe('Tooltip', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function renderTip() {
    return render(
      <Tooltip content="Свернуть список чатов">
        <button type="button">trigger</button>
      </Tooltip>,
    );
  }

  it('hover показывает после задержки, портал в body; leave скрывает', () => {
    renderTip();
    const wrap = screen.getByText('trigger').parentElement as HTMLElement;
    fireEvent.mouseEnter(wrap);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    act(() => vi.advanceTimersByTime(300));
    const tip = screen.getByRole('tooltip');
    expect(tip).toHaveTextContent('Свернуть список чатов');
    expect(tip.parentElement).toBe(document.body);

    fireEvent.mouseLeave(wrap);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('focus показывает сразу, Esc скрывает', () => {
    renderTip();
    fireEvent.focus(screen.getByText('trigger'));
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('клик по триггеру скрывает (не висит над меню)', () => {
    renderTip();
    const btn = screen.getByText('trigger');
    fireEvent.focus(btn);
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.click(btn);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('unmount до истечения задержки не падает', () => {
    const { unmount } = renderTip();
    fireEvent.mouseEnter(screen.getByText('trigger').parentElement as HTMLElement);
    unmount();
    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });
});
