// [B21] SettingRow (канон Row) + Progress — smoke.

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { SettingRow } from './SettingRow';
import { Progress } from './Progress';

afterEach(() => cleanup());

describe('SettingRow', () => {
  test('renders label, hint and control', () => {
    const { container } = render(
      <SettingRow label="Тема" hint="Применяется сразу">
        <button type="button">ctrl</button>
      </SettingRow>,
    );
    const row = container.querySelector('.setting-row')!;
    expect(row.querySelector('.setting-row-label')?.textContent).toBe('Тема');
    expect(row.querySelector('.set-hint')?.textContent).toBe('Применяется сразу');
    expect(row.querySelector('.setting-row-control button')).toBeTruthy();
  });

  test('last row drops the divider via data-last', () => {
    const { container } = render(
      <SettingRow label="A" last>
        <span />
      </SettingRow>,
    );
    expect(container.querySelector('.setting-row')?.getAttribute('data-last')).toBe('true');
  });

  test('align=top and disabled set data attributes', () => {
    const { container } = render(
      <SettingRow label="A" align="top" disabled>
        <span />
      </SettingRow>,
    );
    const row = container.querySelector('.setting-row')!;
    expect(row.getAttribute('data-align')).toBe('top');
    expect(row.getAttribute('data-disabled')).toBe('true');
  });

  test('labelAdornment renders next to label', () => {
    const { container } = render(
      <SettingRow label="Микрофон" labelAdornment={<span className="chip">выдано</span>}>
        <span />
      </SettingRow>,
    );
    expect(container.querySelector('.setting-row-label .chip')?.textContent).toBe('выдано');
  });
});

describe('Progress', () => {
  test('renders .progress with clamped aria-valuenow and width', () => {
    const { container } = render(<Progress value={140} ariaLabel="Загрузка" />);
    const bar = container.querySelector('.progress')!;
    expect(bar.getAttribute('role')).toBe('progressbar');
    expect(bar.getAttribute('aria-valuenow')).toBe('100');
    expect((bar.querySelector('i') as HTMLElement).style.width).toBe('100%');
  });

  test('negative clamps to 0', () => {
    const { container } = render(<Progress value={-5} />);
    expect(container.querySelector('.progress')?.getAttribute('aria-valuenow')).toBe('0');
  });
});
