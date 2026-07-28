// [B21.6] OptionCard внутри radiogroup — roving tabindex + стрелки.

import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { OptionCard } from './OptionCard';

afterEach(() => cleanup());

function renderGroup(onPick: (title: string) => void, activeTitle = 'Средний') {
  return render(
    <div role="radiogroup" aria-label="Пресет">
      {['Лёгкий', 'Средний', 'Полный'].map((title) => (
        <OptionCard
          key={title}
          radio
          title={title}
          active={title === activeTitle}
          onClick={() => onPick(title)}
        />
      ))}
    </div>,
  );
}

describe('OptionCard radiogroup', () => {
  test('в группу ведёт одна табостановка — выбранный вариант', () => {
    const { container } = renderGroup(() => {});
    const items = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    expect(items.map((el) => el.tabIndex)).toEqual([-1, 0, -1]);
    expect(items.map((el) => el.getAttribute('aria-checked'))).toEqual(['false', 'true', 'false']);
  });

  test('ничего не выбрано — табостановку держит первый вариант', () => {
    // Свежая установка: пресет ещё не выбран. Без явного tabStop все карточки
    // получали tabIndex=-1, и в группу нельзя было попасть с клавиатуры.
    const { container } = render(
      <div role="radiogroup" aria-label="Пресет">
        {['Лёгкий', 'Средний', 'Полный'].map((title, i) => (
          <OptionCard key={title} radio title={title} tabStop={i === 0} onClick={() => {}} />
        ))}
      </div>,
    );
    const items = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    expect(items.map((el) => el.tabIndex)).toEqual([0, -1, -1]);
    expect(items.map((el) => el.getAttribute('aria-checked'))).toEqual([
      'false',
      'false',
      'false',
    ]);
  });

  test('стрелка вперёд переносит фокус и выбирает следующий', () => {
    const onPick = vi.fn();
    const { container } = renderGroup(onPick);
    const items = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    fireEvent.keyDown(items[1]!, { key: 'ArrowDown' });
    expect(onPick).toHaveBeenCalledWith('Полный');
    expect(document.activeElement).toBe(items[2]);
  });

  test('стрелка назад с первого варианта заворачивает на последний', () => {
    const onPick = vi.fn();
    const { container } = renderGroup(onPick, 'Лёгкий');
    const items = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    fireEvent.keyDown(items[0]!, { key: 'ArrowLeft' });
    expect(onPick).toHaveBeenCalledWith('Полный');
    expect(document.activeElement).toBe(items[2]);
  });

  test('disabled вариант пропускается', () => {
    const onPick = vi.fn();
    const { container } = render(
      <div role="radiogroup" aria-label="Пресет">
        <OptionCard radio title="A" active onClick={() => onPick('A')} />
        <OptionCard radio title="B" disabled onClick={() => onPick('B')} />
        <OptionCard radio title="C" onClick={() => onPick('C')} />
      </div>,
    );
    const items = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    fireEvent.keyDown(items[0]!, { key: 'ArrowRight' });
    expect(onPick).toHaveBeenCalledWith('C');
  });

  test('вне radiogroup карточка остаётся обычной кнопкой', () => {
    const onPick = vi.fn();
    const { container } = render(<OptionCard title="A" onClick={() => onPick('A')} />);
    const btn = container.querySelector('button')!;
    expect(btn.getAttribute('role')).toBeNull();
    expect(btn.tabIndex).toBe(0);
    // Стрелки не должны ничего выбирать: это не группа.
    fireEvent.keyDown(btn, { key: 'ArrowRight' });
    expect(onPick).not.toHaveBeenCalled();
  });
});
