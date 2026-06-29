// [B18.1c] CommandPalette smoke — opens, filters, Esc/overlay close, Enter runs.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import type { Call } from '../api/recording';
import { CommandPalette } from './CommandPalette';

// useI18n falls back to ru (navigator pinned in test/setup) without a provider.

const recent = [
  {
    id: 'call-acme',
    title: 'Sync with Acme',
    started_at: new Date().toISOString(),
    duration_sec: 120,
  },
] as unknown as Call[];

function setup(overrides: Partial<Parameters<typeof CommandPalette>[0]> = {}) {
  const props = {
    onClose: vi.fn(),
    onNav: vi.fn(),
    onOpenCall: vi.fn(),
    onRecord: vi.fn(),
    recent,
    ...overrides,
  };
  render(<CommandPalette {...props} />);
  return props;
}

afterEach(() => cleanup());

describe('CommandPalette', () => {
  test('renders commands group + the input', () => {
    setup();
    expect(screen.getByText('Команды')).toBeTruthy();
    // Record action label comes from rail.record.
    expect(screen.getByText('Записать звонок')).toBeTruthy();
  });

  test('filters calls by title query', () => {
    setup();
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'acme' } });
    expect(screen.getByText('Sync with Acme')).toBeTruthy();
    // Record action ('Записать звонок') no longer matches 'acme'.
    expect(screen.queryByText('Записать звонок')).toBeNull();
  });

  test('Enter runs the selected (first) action — record by default', () => {
    const props = setup();
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(props.onRecord).toHaveBeenCalledTimes(1);
  });

  test('Escape closes', () => {
    const props = setup();
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalled();
  });

  test('overlay mousedown closes', () => {
    const props = setup();
    const overlay = document.querySelector('.overlay') as HTMLElement;
    fireEvent.mouseDown(overlay);
    expect(props.onClose).toHaveBeenCalled();
  });
});
