// [UI-fix B/C] Smoke: week time-grid (позиционирование чипов, today, onOpen)
// + month padding. События строятся от реального new Date() — InboxWeek/Month
// используют реальное «сегодня».

import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import type { Call } from '../api/recording';
import { InboxMonth, InboxWeek } from './InboxCalendarViews';

afterEach(() => cleanup());

const t = ((k: string) => k) as never;

function callAt(hour: number, minute: number, over: Partial<Call> = {}): Call {
  const d = new Date();
  d.setHours(hour, minute, 0, 0);
  return {
    id: over.id ?? `c-${hour}-${minute}`,
    title: 'Тестовый звонок',
    started_at: d.toISOString(),
    ended_at: null,
    duration_sec: 25 * 60,
    status: 'ready',
    ...over,
  } as Call;
}

const baseProps = {
  onOpen: vi.fn(),
  speakerInitials: new Map<string, string[]>(),
  locale: 'ru',
  t,
};

describe('InboxWeek (time-grid)', () => {
  test('renders .cal-week root with hour gutter and 7 day columns', () => {
    const { container } = render(<InboxWeek {...baseProps} calls={[]} onOpen={vi.fn()} />);
    expect(container.querySelector('.cal-week')).toBeTruthy();
    expect(container.querySelector('.cal-hour-gutter')).toBeTruthy();
    expect(container.querySelectorAll('.cal-week-col')).toHaveLength(7);
    // Пустая неделя → дефолтный диапазон 8..19 = 11 часовых лейблов.
    expect(container.querySelectorAll('.cal-hour-label')).toHaveLength(11);
  });

  test('event chip is absolutely positioned by start time with height from duration', () => {
    const { container } = render(
      <InboxWeek {...baseProps} calls={[callAt(9, 0)]} onOpen={vi.fn()} />,
    );
    const chip = container.querySelector<HTMLButtonElement>('.cal-event');
    expect(chip).toBeTruthy();
    // 9:00 при startHour=8 → top = 48px; 25 мин < MIN_SLOT 40 → height 40/60*48-2 = 30.
    expect(chip!.style.top).toBe('48px');
    expect(chip!.style.height).toBe('30px');
    expect(chip!.style.width).toContain('100%');
  });

  test('overlapping events split the column width', () => {
    const { container } = render(
      <InboxWeek
        {...baseProps}
        calls={[callAt(10, 0, { id: 'a' }), callAt(10, 10, { id: 'b' })]}
        onOpen={vi.fn()}
      />,
    );
    const chips = Array.from(container.querySelectorAll<HTMLButtonElement>('.cal-event'));
    expect(chips).toHaveLength(2);
    expect(chips.every((c) => c.style.width.includes('50%'))).toBe(true);
  });

  test('today column and day header are highlighted', () => {
    const { container } = render(<InboxWeek {...baseProps} calls={[]} onOpen={vi.fn()} />);
    expect(container.querySelector('.cal-week-col.is-today')).toBeTruthy();
    expect(container.querySelector('.cal-week-day.is-today')).toBeTruthy();
  });

  test('clicking an event chip opens the call', () => {
    const onOpen = vi.fn();
    const { container } = render(
      <InboxWeek {...baseProps} calls={[callAt(11, 0, { id: 'call-x' })]} onOpen={onOpen} />,
    );
    fireEvent.click(container.querySelector('.cal-event')!);
    expect(onOpen).toHaveBeenCalledWith('call-x');
  });
});

describe('InboxMonth', () => {
  test('root carries outer padding var(--s5)', () => {
    const { container } = render(<InboxMonth {...baseProps} calls={[]} onOpen={vi.fn()} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.padding).toBe('var(--s5)');
  });
});
