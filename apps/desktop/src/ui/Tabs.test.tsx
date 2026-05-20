import { describe, expect, test, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Tabs } from './Tabs';

function setup(initial = 'a', onChange = vi.fn()) {
  return {
    onChange,
    ...render(
      <Tabs value={initial} onChange={onChange}>
        <Tabs.List>
          <Tabs.Trigger value="a">First</Tabs.Trigger>
          <Tabs.Trigger value="b" counter="3">
            Second
          </Tabs.Trigger>
          <Tabs.Trigger value="c" disabled>
            Disabled
          </Tabs.Trigger>
        </Tabs.List>
        <Tabs.Panel value="a">A content</Tabs.Panel>
        <Tabs.Panel value="b">B content</Tabs.Panel>
      </Tabs>,
    ),
  };
}

describe('Tabs', () => {
  test('renders only active panel', () => {
    setup('a');
    expect(screen.getByText('A content')).toBeInTheDocument();
    expect(screen.queryByText('B content')).not.toBeInTheDocument();
  });

  test('aria-selected reflects active trigger', () => {
    setup('a');
    expect(screen.getByRole('tab', { name: /First/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: /Second/ })).toHaveAttribute('aria-selected', 'false');
  });

  test('click invokes onChange with value', async () => {
    const { onChange } = setup('a');
    await userEvent.click(screen.getByRole('tab', { name: /Second/ }));
    expect(onChange).toHaveBeenCalledWith('b');
  });

  test('disabled trigger blocks click', async () => {
    const { onChange } = setup('a');
    await userEvent.click(screen.getByRole('tab', { name: /Disabled/ }));
    expect(onChange).not.toHaveBeenCalled();
  });

  test('counter renders for trigger with counter prop', () => {
    setup('a');
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  test('throws if Tabs.* used without Tabs parent', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() =>
      render(
        <Tabs.List>
          <Tabs.Trigger value="x">orphan</Tabs.Trigger>
        </Tabs.List>,
      ),
    ).toThrow(/Tabs\.\* must be used within/);
    spy.mockRestore();
  });
});
