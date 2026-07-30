// [B18.1c] CommandPalette smoke — opens, filters, Esc/overlay close, Enter runs.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import type { Call } from '../api/recording';
import { SETTINGS_ENTRIES } from '../pages/settingsIndex';
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

  // [B24.6] ⌘K-fallback «Спросить ассистента» (SPEC §5).
  test('assistant command is listed', () => {
    setup();
    expect(screen.getByText('Ассистент — поиск по звонкам')).toBeTruthy();
  });

  test('fallback appears only when neither commands nor calls match', () => {
    const props = setup({ onAsk: vi.fn() });
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    // 'acme' матчит звонок → fallback НЕ показывается.
    fireEvent.change(input, { target: { value: 'acme' } });
    expect(screen.queryByText('Спросить ассистента')).toBeNull();
    // Полный промах → fallback есть; Enter вызывает onAsk с запросом.
    fireEvent.change(input, { target: { value: 'о чём договорились с юристами' } });
    expect(screen.getByText('Ничего не найдено · Ассистент')).toBeTruthy();
    expect(screen.getByText('Спросить ассистента')).toBeTruthy();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(props.onAsk).toHaveBeenCalledWith('о чём договорились с юристами');
  });

  test('no fallback without onAsk prop', () => {
    setup();
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'полный промах' } });
    expect(screen.queryByText('Спросить ассистента')).toBeNull();
    expect(screen.getByText('Ничего не найдено')).toBeTruthy();
  });

  // ── [B32.4] Настройки в глобальном поиске. Раньше палитра умела только
  //    «открыть Настройки» целиком, и где живёт конкретный тумблер — было
  //    нечем узнать.

  test('section name opens that settings tab, and lists what is inside it', () => {
    const onOpenSettings = vi.fn();
    setup({ onOpenSettings });
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'внешний' } });

    // Имя раздела находит и сам раздел, и его строки — у них раздел стоит
    // в мете. Первым идёт раздел: он и открывает вкладку целиком.
    const hits = screen.getAllByText('Внешний вид');
    expect(hits.length).toBeGreaterThan(1);
    fireEvent.click(hits[0]!);
    expect(onOpenSettings).toHaveBeenCalledWith({ section: 'appearance' });
  });

  test('a concrete setting carries both the section and the anchor', () => {
    const onOpenSettings = vi.fn();
    setup({ onOpenSettings });
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'останавливать' } });

    fireEvent.click(screen.getByText('Останавливать автоматически'));
    expect(onOpenSettings).toHaveBeenCalledWith({
      section: 'recording',
      highlight: 'silence-auto-stop',
    });
  });

  test('settings results are capped so they cannot flood the list', () => {
    // «настройки» — ключевое слово у КАЖДОГО пункта настроек; без потолка
    // запрос выбросил бы из выдачи и команды, и звонки.
    const onOpenSettings = vi.fn();
    setup({ onOpenSettings });
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'настройки' } });

    const items = document.querySelectorAll('.menu-item');
    expect(SETTINGS_ENTRIES.length).toBeGreaterThan(6);
    // 6 настроек + само действие «Настройки» из группы команд.
    expect(items.length).toBeLessThanOrEqual(7);
  });

  test('an empty query does not list settings at all', () => {
    // Палитра открывается списком действий, а не оглавлением настроек.
    setup({ onOpenSettings: vi.fn() });
    expect(screen.queryByText('Внешний вид')).toBeNull();
  });

  test('without the handler nothing settings-related is offered', () => {
    setup();
    const input = document.querySelector('.palette-input input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'внешний' } });
    expect(screen.queryByText('Внешний вид')).toBeNull();
  });
});
