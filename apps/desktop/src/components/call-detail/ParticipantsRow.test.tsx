// [Bug-fix] Vitest для ParticipantsRow — anonymous speakers сценарий.
// Раньше unconfirmed спикеры были полностью скрыты (filter требовал
// contact_display_name); теперь распознанные sortformer'ом анонимные
// голоса видны как dashed chip "Спикер N" с click → bind callback.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import { ParticipantsRow } from './ParticipantsRow';
import type { CallSpeakerView } from '../../api/speakers';

// useI18n() имеет fallback вне Provider — берёт locale из navigator.language
// (test/setup.ts пинит на ru-RU). Поэтому Provider не нужен.
function spk(partial: Partial<CallSpeakerView>): CallSpeakerView {
  return {
    id: 'sp-' + Math.random().toString(36).slice(2, 8),
    call_id: 'c1',
    speaker_tag: 'speaker:0',
    contact_id: null,
    contact_display_name: null,
    suggestion_contact_id: null,
    suggestion_contact_display_name: null,
    suggestion_score: null,
    suggestion_source: null,
    confirmed: false,
    auto_bound_at: null,
    ...partial,
  };
}

function renderWithI18n(ui: React.ReactElement) {
  return render(ui);
}

describe('ParticipantsRow', () => {
  afterEach(() => cleanup());

  test('owner only — показывает named chip, без anonymous', () => {
    const speakers = [
      spk({
        speaker_tag: 'owner',
        contact_id: 'c1',
        contact_display_name: 'Дамир',
        confirmed: true,
      }),
    ];
    renderWithI18n(<ParticipantsRow speakers={speakers} />);
    expect(screen.getByText('Дамир')).toBeTruthy();
    expect(screen.queryByText(/Спикер/)).toBeNull();
    expect(screen.getByText(/· 1 участник$/)).toBeTruthy();
  });

  test('owner + 1 anonymous — оба видны, count=2', () => {
    const speakers = [
      spk({
        speaker_tag: 'owner',
        contact_id: 'c1',
        contact_display_name: 'Дамир',
        confirmed: true,
      }),
      spk({
        speaker_tag: 'speaker:1',
        contact_id: null,
        contact_display_name: null,
        confirmed: false,
      }),
    ];
    renderWithI18n(<ParticipantsRow speakers={speakers} />);
    expect(screen.getByText('Дамир')).toBeTruthy();
    expect(screen.getByText('Спикер 1')).toBeTruthy();
    expect(screen.getByText(/· 2 участника/)).toBeTruthy();
  });

  test('speaker:unknown НЕ показываем как anonymous', () => {
    const speakers = [
      spk({
        speaker_tag: 'owner',
        contact_id: 'c1',
        contact_display_name: 'Дамир',
        confirmed: true,
      }),
      spk({ speaker_tag: 'speaker:unknown' }),
    ];
    renderWithI18n(<ParticipantsRow speakers={speakers} />);
    expect(screen.queryByText(/Спикер/)).toBeNull();
    expect(screen.getByText(/· 1 участник/)).toBeTruthy();
  });

  test('click на anonymous chip — вызывается callback', () => {
    const onConfirm = vi.fn();
    const speakers = [
      spk({
        speaker_tag: 'owner',
        contact_id: 'c1',
        contact_display_name: 'Дамир',
        confirmed: true,
      }),
      spk({ speaker_tag: 'speaker:1', id: 'anon-1' }),
    ];
    renderWithI18n(
      <ParticipantsRow speakers={speakers} onConfirmAnonymous={onConfirm} />,
    );
    fireEvent.click(screen.getByText('Спикер 1'));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onConfirm.mock.calls[0]![0].speaker_tag).toBe('speaker:1');
  });

  test('dedupe анонимных по speaker_tag — повторяющийся tag даёт 1 chip', () => {
    const speakers = [
      spk({ speaker_tag: 'speaker:1', id: 'a' }),
      spk({ speaker_tag: 'speaker:1', id: 'b' }),
      spk({ speaker_tag: 'speaker:2', id: 'c' }),
    ];
    renderWithI18n(<ParticipantsRow speakers={speakers} />);
    expect(screen.getByText('Спикер 1')).toBeTruthy();
    expect(screen.getByText('Спикер 2')).toBeTruthy();
    expect(screen.getAllByText(/^Спикер /)).toHaveLength(2);
  });
});
