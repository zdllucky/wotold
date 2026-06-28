// [B18.6c] Smoke tests for the new v2 uikit interactive wrappers.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, test, vi } from 'vitest';

import { Switch } from './Switch';
import { Segmented } from './Segmented';
import { Modal } from './Modal';
import { Dropdown, MenuItem } from './Menu';

afterEach(() => cleanup());

describe('Switch', () => {
  test('renders role=switch reflecting checked + toggles on click', async () => {
    const onChange = vi.fn();
    render(<Switch checked={false} onChange={onChange} label="feature" />);
    const sw = screen.getByRole('switch');
    expect(sw).toHaveAttribute('aria-checked', 'false');
    await userEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

describe('Segmented', () => {
  const opts = [
    { value: 'a', label: 'A' },
    { value: 'b', label: 'B' },
  ];
  test('marks active option and calls onChange', async () => {
    const onChange = vi.fn();
    render(<Segmented options={opts} value="a" onChange={onChange} ariaLabel="pick" />);
    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(2);
    expect(tabs[0]).toHaveAttribute('aria-selected', 'true');
    await userEvent.click(tabs[1]!);
    expect(onChange).toHaveBeenCalledWith('b');
  });
});

describe('Modal', () => {
  test('renders nothing when closed', () => {
    render(
      <Modal open={false} onClose={vi.fn()} title="T">
        body
      </Modal>,
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  test('open renders dialog with title + body; Esc and backdrop close', async () => {
    const onClose = vi.fn();
    const { container } = render(
      <Modal open onClose={onClose} title="Title">
        <p>hello</p>
      </Modal>,
    );
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(dialog).toHaveAttribute('aria-labelledby');
    expect(screen.getByText('Title')).toBeInTheDocument();
    expect(screen.getByText('hello')).toBeInTheDocument();
    // Esc (handled by useFocusTrap on the trapped element)
    fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
    // backdrop mousedown
    const overlay = container.querySelector('.overlay')!;
    fireEvent.mouseDown(overlay);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe('Dropdown', () => {
  test('opens menu via trigger, closes on Escape', async () => {
    render(
      <Dropdown trigger={({ toggle }) => <button onClick={toggle}>open</button>}>
        <MenuItem>Item</MenuItem>
      </Dropdown>,
    );
    expect(screen.queryByText('Item')).not.toBeInTheDocument();
    await userEvent.click(screen.getByText('open'));
    expect(screen.getByText('Item')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByText('Item')).not.toBeInTheDocument();
  });
});
