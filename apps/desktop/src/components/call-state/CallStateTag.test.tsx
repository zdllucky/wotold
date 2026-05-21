import { describe, expect, test } from 'vitest';
import { render, screen } from '@testing-library/react';

import { CallStateTag } from './CallStateTag';
// useI18n() в test environment без Provider'а возвращает fallback ctx
// который выдаёт строки из дефолтного locale (см. i18n/index.ts). Тесты
// без Provider'а — быстрее, не trigger'ят async useEffect → settings call.

describe('CallStateTag', () => {
  test('renders class variant for each state', () => {
    const states = ['live', 'uploading', 'queued', 'processing', 'ready', 'error'] as const;
    for (const s of states) {
      const { container, unmount } = render(<CallStateTag state={s} />);
      const el = container.querySelector(`.stat-tag.stat-tag--${s}`);
      expect(el).toBeTruthy();
      expect(el?.textContent?.trim().length).toBeGreaterThan(0);
      expect(container.querySelector('.stat-tag-dot')).toBeTruthy();
      unmount();
    }
  });

  test('appends detail with separator', () => {
    const { container } = render(
      <CallStateTag state="processing" detail="64%" />,
    );
    expect(container.querySelector('.stat-tag')?.textContent).toContain('64%');
    expect(container.querySelector('.stat-tag')?.textContent).toContain('·');
  });

  test('labelOverride replaces default label', () => {
    render(<CallStateTag state="processing" labelOverride="custom label" />);
    expect(screen.getByText('custom label')).toBeTruthy();
  });

  test('empty detail does not show separator', () => {
    const { container } = render(<CallStateTag state="ready" detail="" />);
    const text = container.querySelector('.stat-tag')?.textContent ?? '';
    expect(text).not.toContain('·');
  });
});
