// [B20.7] ParticipantRow — smoke RTL: один голос → кнопка отвязки; несколько
// голосов → dropdown со строками по голосу; unbind вызывает handler с id.

import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { ParticipantRow } from './ParticipantRow';
import type { ConfirmedGroup } from './participantGroups';
import type { CallSpeakerView } from '../../api/speakers';

function speaker(id: string, tag: string): CallSpeakerView {
  return {
    id,
    call_id: 'call-1',
    speaker_tag: tag,
    contact_id: 'c1',
    contact_display_name: 'Глеб Гусак',
    suggestion_contact_id: null,
    suggestion_contact_display_name: null,
    suggestion_score: null,
    suggestion_source: null,
    confirmed: true,
    auto_bound_at: null,
  };
}

function group(speakers: CallSpeakerView[]): ConfirmedGroup {
  return { key: 'c1', displayName: 'Глеб Гусак', speakers };
}

const noSamples = new Map<string, null>();

describe('ParticipantRow', () => {
  it('single voice: renders unbind icon button, no dropdown trigger', () => {
    const onUnbind = vi.fn();
    render(
      <ParticipantRow
        group={group([speaker('cs-1', 'speaker:1')])}
        color="var(--sp2)"
        samplesByTag={noSamples}
        onUnbind={onUnbind}
      />,
    );
    const unbind = screen.getByRole('button', { name: /отвязать/i });
    fireEvent.click(unbind);
    expect(onUnbind).toHaveBeenCalledWith('cs-1');
    expect(screen.queryByRole('button', { name: /голоса участника/i })).toBeNull();
  });

  it('multiple voices: dropdown trigger opens per-voice rows with unbind', () => {
    const onUnbind = vi.fn();
    render(
      <ParticipantRow
        group={group([speaker('cs-1', 'speaker:1'), speaker('cs-2', 'speaker:2')])}
        color="var(--sp2)"
        samplesByTag={noSamples}
        onUnbind={onUnbind}
      />,
    );
    const trigger = screen.getByRole('button', { name: /голоса участника/i });
    fireEvent.click(trigger);
    const unbinds = screen.getAllByRole('button', { name: /отвязать/i });
    expect(unbinds).toHaveLength(2);
    fireEvent.click(unbinds[1]!);
    expect(onUnbind).toHaveBeenCalledWith('cs-2');
  });

  it('voices count note shown for multi-voice group', () => {
    render(
      <ParticipantRow
        group={group([speaker('cs-1', 'speaker:1'), speaker('cs-2', 'speaker:2')])}
        color="var(--sp2)"
        samplesByTag={noSamples}
        onUnbind={() => {}}
      />,
    );
    expect(screen.getByText(/2/)).toBeTruthy();
  });

  it('sample button disabled when no sample for voice', () => {
    render(
      <ParticipantRow
        group={group([speaker('cs-1', 'speaker:1'), speaker('cs-2', 'speaker:2')])}
        color="var(--sp2)"
        samplesByTag={noSamples}
        onUnbind={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /голоса участника/i }));
    const plays = screen.getAllByRole('button', { name: /послушать сэмпл/i });
    expect(plays).toHaveLength(2);
    expect((plays[0] as HTMLButtonElement).disabled).toBe(true);
  });
});
