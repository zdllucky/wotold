// [TD-30] Скелетон обязан повторять разметку живого транскрипта.
//
// До фикса он рисовал ghost-строки легаси-классом `.transcript-row` (сетка
// 130px 1fr 60px, кегль 17px), а живой транскрипт — v2-классом `.turn`
// (сетка 140px minmax(0,1fr), --t-15). При переходе загрузка→контент менялись
// и сетка, и кегль: скелетон устраивал ровно тот layout-jump, от которого
// должен защищать.

import { render } from '@testing-library/react';
import { describe, expect, test } from 'vitest';
import { CallDetailSkeleton } from './CallDetailSkeleton';

describe('CallDetailSkeleton', () => {
  test('ghost-строки используют разметку живого транскрипта', () => {
    const { container } = render(<CallDetailSkeleton onBack={() => {}} />);

    // Та же сетка, что у настоящих реплик.
    expect(container.querySelectorAll('.turn').length).toBeGreaterThan(0);
    // Легаси-разметка с другой сеткой и кеглем — не должна возвращаться.
    expect(container.querySelector('.transcript-row')).toBeNull();
  });

  test('скелетон помечен как busy, ghost-строки скрыты от скринридера', () => {
    const { container } = render(<CallDetailSkeleton onBack={() => {}} />);
    expect(container.querySelector('[aria-busy="true"]')).not.toBeNull();
    for (const row of container.querySelectorAll('.turn')) {
      expect(row.getAttribute('aria-hidden')).toBe('true');
    }
  });

  test('shimmer идёт через канонический .skeleton', () => {
    // В проекте жили три параллельные shimmer-системы, и канон был мёртв.
    const { container } = render(<CallDetailSkeleton onBack={() => {}} />);
    expect(container.querySelectorAll('.skeleton').length).toBeGreaterThan(0);
    expect(container.querySelector('.ds-skeleton')).toBeNull();
  });
});
