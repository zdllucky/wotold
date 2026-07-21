// [B20.5] ContextMenu — portal, позиция у курсора, Escape/outside/click close.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { ContextMenu } from './ContextMenu';
import { MenuItem } from './Menu';

afterEach(() => cleanup());

describe('ContextMenu', () => {
  test('renders into body at fixed cursor position with role=menu', () => {
    render(
      <ContextMenu pos={{ x: 40, y: 60 }} onClose={() => {}}>
        <MenuItem onClick={() => {}}>Открыть</MenuItem>
      </ContextMenu>,
    );
    const menu = screen.getByRole('menu');
    expect(menu.parentElement).toBe(document.body);
    expect(menu.style.position).toBe('fixed');
    expect(screen.getByText('Открыть')).toBeTruthy();
  });

  test('Escape closes', () => {
    const onClose = vi.fn();
    render(
      <ContextMenu pos={{ x: 0, y: 0 }} onClose={onClose}>
        <MenuItem onClick={() => {}}>Пункт</MenuItem>
      </ContextMenu>,
    );
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  test('outside pointerdown closes', () => {
    const onClose = vi.fn();
    render(
      <ContextMenu pos={{ x: 0, y: 0 }} onClose={onClose}>
        <MenuItem onClick={() => {}}>Пункт</MenuItem>
      </ContextMenu>,
    );
    fireEvent.pointerDown(document.body);
    expect(onClose).toHaveBeenCalled();
  });

  test('item click fires handler and closes menu', () => {
    const onClose = vi.fn();
    const onAction = vi.fn();
    render(
      <ContextMenu pos={{ x: 0, y: 0 }} onClose={onClose}>
        <MenuItem onClick={onAction}>Пункт</MenuItem>
      </ContextMenu>,
    );
    fireEvent.click(screen.getByText('Пункт'));
    expect(onAction).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  test('focuses first enabled item on mount', () => {
    render(
      <ContextMenu pos={{ x: 0, y: 0 }} onClose={() => {}}>
        <MenuItem disabled onClick={() => {}}>
          Выкл
        </MenuItem>
        <MenuItem onClick={() => {}}>Вкл</MenuItem>
      </ContextMenu>,
    );
    expect(document.activeElement?.textContent).toContain('Вкл');
  });
});
