// [B24.4/ревью] AssistantComposer: Enter-путь (в форме нет submit-кнопки),
// trim, disabled, кнопка send.

import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { AssistantComposer } from './AssistantComposer';

function setup(overrides: Partial<Parameters<typeof AssistantComposer>[0]> = {}) {
  const onAsk = vi.fn();
  render(
    <AssistantComposer placeholder="Спросить…" icon="search" onAsk={onAsk} {...overrides} />,
  );
  const input = screen.getByPlaceholderText('Спросить…') as HTMLInputElement;
  return { onAsk, input };
}

describe('AssistantComposer', () => {
  it('Enter отправляет и чистит драфт', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: '  вопрос  ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).toHaveBeenCalledWith('вопрос');
    expect(input.value).toBe('');
  });

  it('submit формы отправляет', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'вопрос' } });
    fireEvent.submit(input.closest('form') as HTMLFormElement);
    expect(onAsk).toHaveBeenCalledWith('вопрос');
  });

  it('кнопка send отправляет', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'вопрос' } });
    fireEvent.click(screen.getByLabelText('Отправить'));
    expect(onAsk).toHaveBeenCalledWith('вопрос');
  });

  it('пустой драфт и disabled не отправляются, драфт сохраняется', () => {
    const { onAsk, input } = setup({ disabled: true });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).not.toHaveBeenCalled();
    // disabled: текст не должен стираться молча (ревью H2 чанка).
    fireEvent.change(input, { target: { value: 'висящий вопрос' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).not.toHaveBeenCalled();
    expect(input.value).toBe('висящий вопрос');
  });
});
