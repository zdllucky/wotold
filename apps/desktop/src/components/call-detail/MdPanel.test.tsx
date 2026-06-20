import { describe, expect, test, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { MdPanel } from './MdPanel';

describe('MdPanel', () => {
  test('header-only recap (старый пустой) → CTA, не голый заголовок', () => {
    const onRegenerate = vi.fn();
    render(
      <MdPanel md={'# Рекап\n\n'} emptyHint="hint" onRegenerate={onRegenerate} />,
    );
    const button = screen.getByRole('button');
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(onRegenerate).toHaveBeenCalledTimes(1);
  });

  test('null → CTA', () => {
    render(<MdPanel md={null} emptyHint="hint" onRegenerate={() => {}} />);
    expect(screen.getByRole('button')).toBeTruthy();
  });

  test('рекап с телом → рендерит markdown, без CTA', () => {
    render(
      <MdPanel
        md={'# Рекап\n\nреальный текст саммари'}
        emptyHint="hint"
        onRegenerate={() => {}}
      />,
    );
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByText('реальный текст саммари')).toBeTruthy();
  });
});
