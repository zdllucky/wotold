import { describe, expect, test, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { EvidenceTooltip } from './EvidenceTooltip';

describe('EvidenceTooltip', () => {
  test('renders trigger, popover hidden by default', () => {
    render(
      <EvidenceTooltip quote="we agreed on 499">
        <span>💬</span>
      </EvidenceTooltip>,
    );
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  test('hover opens popover, mouseleave closes it', () => {
    render(
      <EvidenceTooltip quote="we agreed on 499">
        <span>💬</span>
      </EvidenceTooltip>,
    );
    const wrapper = screen.getByRole('button').parentElement!;
    fireEvent.mouseEnter(wrapper);
    expect(screen.getByRole('tooltip').textContent).toContain('we agreed on 499');
    fireEvent.mouseLeave(wrapper);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  test('click makes popover sticky — mouseleave does not close', () => {
    render(
      <EvidenceTooltip quote="we agreed on 499">
        <span>💬</span>
      </EvidenceTooltip>,
    );
    const trigger = screen.getByRole('button');
    fireEvent.click(trigger);
    expect(screen.getByRole('tooltip')).toBeTruthy();
    const wrapper = trigger.parentElement!;
    fireEvent.mouseLeave(wrapper);
    // Sticky — popover остаётся.
    expect(screen.getByRole('tooltip')).toBeTruthy();
  });

  test('jump-to-moment button visible only когда startMs + callback', () => {
    const onJump = vi.fn();
    render(
      <EvidenceTooltip
        quote="we agreed on 499"
        startMs={12_500}
        onJumpToTranscript={onJump}
      >
        <span>💬</span>
      </EvidenceTooltip>,
    );
    fireEvent.click(screen.getByRole('button', { name: /from transcript|расшифровки|транскриптт/i }));
    const jumpBtn = screen.getByRole('button', { name: /jump|момент|сәт/i });
    expect(jumpBtn).toBeTruthy();
    fireEvent.click(jumpBtn);
    expect(onJump).toHaveBeenCalledWith(12_500);
  });

  test('renders speaker label when provided', () => {
    render(
      <EvidenceTooltip quote="x" speaker="Alice">
        <span>💬</span>
      </EvidenceTooltip>,
    );
    fireEvent.mouseEnter(screen.getByRole('button').parentElement!);
    expect(screen.getByRole('tooltip').textContent).toContain('Alice');
  });

  test('formats timestamp as mm:ss', () => {
    render(
      <EvidenceTooltip quote="x" startMs={65_000}>
        <span>💬</span>
      </EvidenceTooltip>,
    );
    fireEvent.mouseEnter(screen.getByRole('button').parentElement!);
    expect(screen.getByRole('tooltip').textContent).toContain('1:05');
  });
});
