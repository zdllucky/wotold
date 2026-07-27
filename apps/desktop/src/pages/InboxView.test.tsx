// [B18.2a / B18.9] InboxView behaviour test (retargeted from CallsPage). A ready
// call with an active background task (regen) shows the «обрабатывается»
// indicator, even though its status stays 'ready'. Source: list_active_call_ids
// (pipeline_tasks registry). Без активной задачи — чисто, без строки.
//
// Also asserts the v2 header/layout structure: the shared `.view-head` bar and
// the database `.tbl` table replace the old flex header + `.lrow` list.

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => `asset://${p}`,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {
    /* unlisten noop */
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import type { ReactElement } from 'react';
import type { Call } from '../api/recording';
import { ToastProvider } from '../ui';
import { InboxView } from './InboxView';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

// InboxView's row-menu actions use useToast() → wrap in the provider.
const renderInbox = (ui: ReactElement) => render(<ToastProvider>{ui}</ToastProvider>);

const READY_CALL: Call = {
  id: 'call-busy-1',
  title: 'Синхрон по проекту',
  started_at: '2026-06-20T09:00:00Z',
  ended_at: '2026-06-20T09:30:00Z',
  duration_sec: 1800,
  status: 'ready',
  provider: null,
  path_label: '',
  lang_detected: 'ru',
  failed_reason: null,
  recap_failed_reason: null,
  pipeline_step: null,
  pipeline_pct: null,
  pipeline_eta_sec: null,
  upload_bytes: null,
  paused_at: null,
  paused_total_ms: 0,
  processing_via: 'local',
  call_type: null,
  call_type_confidence: null,
  summary_schema_version: 2,
  summary_engine: 'local-qwen-3b',
  summary_pipeline_mode: null,
  created_at: '2026-06-20T09:00:00Z',
  updated_at: '2026-06-20T09:31:00Z',
};

function routeInvoke(activeIds: string[]) {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'list_calls':
        return Promise.resolve([READY_CALL]);
      case 'list_active_call_ids':
        return Promise.resolve(activeIds);
      case 'list_call_speakers':
        return Promise.resolve([]);
      // [TD-46] Инбокс тянет спикеров пачкой: карта call_id → спикеры.
      case 'list_call_speakers_batch':
        return Promise.resolve({});
      default:
        return Promise.resolve(null);
    }
  });
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('InboxView — processing status', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('ready call with active background task shows processing indicator', async () => {
    routeInvoke([READY_CALL.id]);
    renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    expect(screen.getByText('Синхрон по проекту')).toBeTruthy();
    expect(screen.getByText('обрабатывается')).toBeTruthy();
  });

  test('ready call without active task stays clean (no busy row)', async () => {
    routeInvoke([]);
    renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    expect(screen.getByText('Синхрон по проекту')).toBeTruthy();
    expect(screen.queryByText('обрабатывается')).toBeNull();
  });

  test('renders the shared .view-head bar and the v2 .tbl table', async () => {
    routeInvoke([]);
    const { container } = renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    expect(container.querySelector('.view-head')).not.toBeNull();
    expect(container.querySelector('.tbl')).not.toBeNull();
    expect(container.querySelector('.tbl-head')).not.toBeNull();
    // The list row uses the database `.trow` grid, not the old `.lrow`.
    expect(container.querySelector('.trow')).not.toBeNull();
    expect(container.querySelector('.lrow')).toBeNull();
  });

  test('record action is omitted unless onRecord is provided', async () => {
    routeInvoke([]);
    const { container, rerender } = renderInbox(<InboxView onOpen={() => {}} />);
    await flush();
    expect(container.querySelector('.btn--primary')).toBeNull();

    rerender(
      <ToastProvider>
        <InboxView onOpen={() => {}} onRecord={() => {}} />
      </ToastProvider>,
    );
    await flush();
    // The «Записать» primary button now appears in the header.
    expect(screen.getByText('Записать')).toBeTruthy();
  });

  // [B20.4] Keep-alive: состояние (поиск) переживает hide/show через active,
  // скрытый корень уходит в display:none, а не unmount.
  test('keep-alive: search text survives active toggle, hidden root is display:none', async () => {
    routeInvoke([]);
    const { container, rerender } = renderInbox(<InboxView onOpen={() => {}} active />);
    await flush();

    const input = container.querySelector<HTMLInputElement>('input[type="text"], input:not([type])');
    expect(input).toBeTruthy();
    fireEvent.change(input!, { target: { value: 'синхрон' } });
    expect(input!.value).toBe('синхрон');

    rerender(
      <ToastProvider>
        <InboxView onOpen={() => {}} active={false} />
      </ToastProvider>,
    );
    await flush();
    const root = container.querySelector<HTMLElement>('.main');
    expect(root?.style.display).toBe('none');

    rerender(
      <ToastProvider>
        <InboxView onOpen={() => {}} active />
      </ToastProvider>,
    );
    await flush();
    const inputAfter = container.querySelector<HTMLInputElement>(
      'input[type="text"], input:not([type])',
    );
    expect(inputAfter!.value).toBe('синхрон');
    expect(container.querySelector<HTMLElement>('.main')?.style.display).not.toBe('none');
  });
});

// ── [TD-26] залипающий error и сортировка ─────────────────────────────

describe('InboxView — TD-26', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  // Тест на снятие залипшей ошибки НЕ написан осознанно: refresh вызывается
  // из слушателей pipeline-событий, а `listen` в этом файле замокан в noop —
  // дёрнуть повторный успешный refresh на ЖИВОМ инстансе неоткуда. Через
  // rerender с новым key тест получается фиктивным: перемонтирование само
  // сбрасывает состояние, и он зеленеет на сломанном коде. Фикс — одна
  // строка `setError(null)` в ветке успеха.

  test('заголовки колонок сортируют и сообщают состояние', async () => {
    // Регрессия TD-26: колонки «Длительность» и «Дата» имели иконку sort и
    // cursor:pointer, но ни onClick, ни sort-state не существовало —
    // аффорданса врала.
    routeInvoke([]);
    renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    const headers = screen.getAllByRole('columnheader');
    const dateHeader = headers.find((h) => h.textContent?.includes('Дата'));
    expect(dateHeader).toBeDefined();
    // По умолчанию — дата по убыванию, как и было до фикса.
    expect(dateHeader).toHaveAttribute('aria-sort', 'descending');

    const btn = dateHeader?.querySelector('button');
    expect(btn).not.toBeNull();
    await act(async () => {
      btn?.click();
      await Promise.resolve();
    });
    expect(dateHeader).toHaveAttribute('aria-sort', 'ascending');
  });
});

describe('InboxView — TD-46', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('спикеры тянутся одним батчем, а не запросом на строку', async () => {
    // Регрессия TD-46: на каждый refresh инбокс делал listCallSpeakers на
    // КАЖДЫЙ готовый звонок, и стрелял этой пачкой даже пока пользователь
    // смотрит настройки (инбокс живёт через keep-alive).
    const second: Call = { ...READY_CALL, id: 'call-busy-2', title: 'Второй' };
    mockInvoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'list_calls':
          return Promise.resolve([READY_CALL, second]);
        case 'list_active_call_ids':
          return Promise.resolve([]);
        case 'list_call_speakers_batch':
          return Promise.resolve({});
        default:
          return Promise.resolve(null);
      }
    });

    renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    const batchCalls = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'list_call_speakers_batch',
    );
    expect(batchCalls).toHaveLength(1);
    expect(batchCalls[0]?.[1]).toEqual({
      callIds: [READY_CALL.id, second.id],
    });
    expect(mockInvoke.mock.calls.some((c) => c[0] === 'list_call_speakers')).toBe(false);
  });

  test('пустой список готовых звонков не ходит в бэкенд за спикерами', async () => {
    const processing: Call = { ...READY_CALL, status: 'processing' };
    mockInvoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'list_calls':
          return Promise.resolve([processing]);
        case 'list_active_call_ids':
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });

    renderInbox(<InboxView onOpen={() => {}} />);
    await flush();

    expect(
      mockInvoke.mock.calls.some((c) => c[0] === 'list_call_speakers_batch'),
    ).toBe(false);
  });
});
