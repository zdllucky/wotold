// Tests for Select.tsx — custom combobox with keyboard navigation, typeahead,
// a11y roles, and selected-option styling.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { Select } from './Select';
import type { SelectOption } from './Select';

afterEach(() => cleanup());

const FRUITS: SelectOption[] = [
  { value: 'apple', label: 'Apple', searchText: 'apple' },
  { value: 'banana', label: 'Banana', searchText: 'banana' },
  { value: 'cherry', label: 'Cherry', searchText: 'cherry' },
  { value: 'date', label: 'Date', searchText: 'date', disabled: true },
];

function renderSelect(overrides: Partial<Parameters<typeof Select>[0]> = {}) {
  const onChange = vi.fn();
  const utils = render(
    <Select
      value="apple"
      options={FRUITS}
      onChange={onChange}
      ariaLabel="fruit picker"
      {...overrides}
    />,
  );
  return { ...utils, onChange };
}

// ─── Rendering ─────────────────────────────────────────────────────────────

describe('Select — initial render', () => {
  test('shows selected label in trigger', () => {
    renderSelect({ value: 'banana' });
    expect(screen.getByRole('combobox')).toHaveTextContent('Banana');
  });

  test('shows placeholder when no value matches', () => {
    renderSelect({ value: '' as 'apple' });
    expect(screen.getByRole('combobox')).toHaveTextContent('— не выбран —');
  });

  test('shows custom placeholder', () => {
    renderSelect({ value: '' as 'apple', placeholder: 'Pick one' });
    expect(screen.getByRole('combobox')).toHaveTextContent('Pick one');
  });

  test('trigger has combobox role + aria-expanded=false when closed', () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    expect(btn).toHaveAttribute('aria-expanded', 'false');
    expect(btn).toHaveAttribute('aria-haspopup', 'listbox');
  });

  test('disabled trigger is not clickable', () => {
    renderSelect({ disabled: true });
    const btn = screen.getByRole('combobox');
    expect(btn).toBeDisabled();
  });
});

// ─── Open / Close ──────────────────────────────────────────────────────────

describe('Select — open/close', () => {
  test('click opens listbox', async () => {
    renderSelect();
    await userEvent.click(screen.getByRole('combobox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-expanded', 'true');
  });

  test('click again closes listbox', async () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    await userEvent.click(btn);
    await userEvent.click(btn);
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  test('Esc key closes listbox', async () => {
    renderSelect();
    await userEvent.click(screen.getByRole('combobox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  test('Tab key closes listbox', async () => {
    renderSelect();
    await userEvent.click(screen.getByRole('combobox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Tab' });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  test('mousedown outside closes listbox', async () => {
    const { container } = renderSelect();
    await userEvent.click(screen.getByRole('combobox'));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent.mouseDown(container.ownerDocument.body);
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
});

// ─── Keyboard open ─────────────────────────────────────────────────────────

describe('Select — keyboard open', () => {
  test('Enter opens when closed', async () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    btn.focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  test('Space opens when closed', async () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    btn.focus();
    fireEvent.keyDown(btn, { key: ' ' });
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  test('ArrowDown opens when closed', async () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    btn.focus();
    fireEvent.keyDown(btn, { key: 'ArrowDown' });
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  test('ArrowUp opens when closed', async () => {
    renderSelect();
    const btn = screen.getByRole('combobox');
    btn.focus();
    fireEvent.keyDown(btn, { key: 'ArrowUp' });
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });
});

// ─── Selection ─────────────────────────────────────────────────────────────

describe('Select — selection', () => {
  test('clicking option calls onChange with value', async () => {
    const { onChange } = renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    fireEvent.mouseDown(opts[1]!); // banana
    expect(onChange).toHaveBeenCalledWith('banana');
  });

  test('Enter on highlighted option calls onChange', async () => {
    const { onChange } = renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'ArrowDown' });
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
    expect(onChange).toHaveBeenCalled();
  });

  test('clicking disabled option does NOT call onChange', async () => {
    const { onChange } = renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    // date (idx 3) is disabled
    fireEvent.mouseDown(opts[3]!);
    expect(onChange).not.toHaveBeenCalled();
  });
});

// ─── Keyboard navigation ────────────────────────────────────────────────────

describe('Select — keyboard navigation', () => {
  test('ArrowDown moves highlight down, wraps around', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    // start at apple (idx 0), go to banana (idx 1)
    fireEvent.keyDown(btn, { key: 'ArrowDown' });
    // aria-activedescendant should point to idx 1
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-1$/);
  });

  test('ArrowUp moves highlight up', async () => {
    renderSelect({ value: 'banana' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    // start at banana (idx 1), go to apple (idx 0)
    fireEvent.keyDown(btn, { key: 'ArrowUp' });
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-0$/);
  });

  test('Home moves to first non-disabled option', async () => {
    renderSelect({ value: 'cherry' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    fireEvent.keyDown(btn, { key: 'Home' });
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-0$/);
  });

  test('End moves to last non-disabled option', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    fireEvent.keyDown(btn, { key: 'End' });
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    // date is disabled (idx 3), so last enabled is cherry (idx 2)
    expect(adId).toMatch(/-opt-2$/);
  });

  test('ArrowDown skips disabled options', async () => {
    // Put value at cherry (idx 2), next is date (disabled, idx 3), wraps to apple (0)
    renderSelect({ value: 'cherry' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    fireEvent.keyDown(btn, { key: 'ArrowDown' });
    // should skip disabled date and wrap to apple
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-0$/);
  });
});

// ─── Typeahead ─────────────────────────────────────────────────────────────

describe('Select — typeahead', () => {
  test('typing letter jumps to matching option', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    fireEvent.keyDown(btn, { key: 'c' });
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-2$/); // cherry
  });

  test('meta/ctrl/alt keys are not treated as typeahead', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    const initialAdId = btn.getAttribute('aria-activedescendant');
    fireEvent.keyDown(btn, { key: 'c', metaKey: true });
    // highlight unchanged
    expect(btn.getAttribute('aria-activedescendant')).toBe(initialAdId);
  });
});

// ─── A11y attributes ────────────────────────────────────────────────────────

describe('Select — a11y', () => {
  test('listbox options have aria-selected', async () => {
    renderSelect({ value: 'banana' });
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    // banana is value → aria-selected=true
    expect(opts[1]).toHaveAttribute('aria-selected', 'true');
    expect(opts[0]).toHaveAttribute('aria-selected', 'false');
  });

  test('disabled option has aria-disabled', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    expect(opts[3]).toHaveAttribute('aria-disabled');
  });

  test('hint renders inside option', async () => {
    const opts: SelectOption[] = [
      { value: 'x', label: 'Extra', hint: 'x-hint' },
    ];
    render(<Select value="x" options={opts} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole('combobox'));
    expect(screen.getByText('x-hint')).toBeInTheDocument();
  });

  test('ariaLabel is forwarded to button', () => {
    renderSelect({ ariaLabel: 'custom label' });
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-label', 'custom label');
  });

  test('disabled select ignores keyboard', () => {
    renderSelect({ disabled: true });
    const btn = screen.getByRole('combobox');
    btn.focus();
    fireEvent.keyDown(btn, { key: 'Enter' });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
});

// ─── MouseEnter highlight ──────────────────────────────────────────────────

describe('Select — mouseEnter highlight', () => {
  test('hovering non-disabled option changes highlight', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    fireEvent.mouseEnter(opts[2]!); // cherry
    const btn = screen.getByRole('combobox');
    const adId = btn.getAttribute('aria-activedescendant') ?? '';
    expect(adId).toMatch(/-opt-2$/);
  });

  test('hovering disabled option does NOT change highlight', async () => {
    renderSelect({ value: 'apple' });
    await userEvent.click(screen.getByRole('combobox'));
    const btn = screen.getByRole('combobox');
    const adIdBefore = btn.getAttribute('aria-activedescendant');
    const opts = screen.getAllByRole('option');
    fireEvent.mouseEnter(opts[3]!); // date (disabled)
    expect(btn.getAttribute('aria-activedescendant')).toBe(adIdBefore);
  });
});
