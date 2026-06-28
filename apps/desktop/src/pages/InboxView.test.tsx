// [B18.2a / B18.9] InboxView behaviour test (retargeted from CallsPage). A ready
// call with an active background task (regen) shows the «обрабатывается»
// indicator, even though its status stays 'ready'. Source: list_active_call_ids
// (pipeline_tasks registry). Без активной задачи — чисто, без строки.
//
// Also asserts the v2 header/layout structure: the shared `.view-head` bar and
// the database `.tbl` table replace the old flex header + `.lrow` list.

import { act, cleanup, render, screen } from '@testing-library/react';
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
import type { Call } from '../api/recording';
import { InboxView } from './InboxView';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

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
    render(<InboxView onOpen={() => {}} />);
    await flush();

    expect(screen.getByText('Синхрон по проекту')).toBeTruthy();
    expect(screen.getByText('обрабатывается')).toBeTruthy();
  });

  test('ready call without active task stays clean (no busy row)', async () => {
    routeInvoke([]);
    render(<InboxView onOpen={() => {}} />);
    await flush();

    expect(screen.getByText('Синхрон по проекту')).toBeTruthy();
    expect(screen.queryByText('обрабатывается')).toBeNull();
  });

  test('renders the shared .view-head bar and the v2 .tbl table', async () => {
    routeInvoke([]);
    const { container } = render(<InboxView onOpen={() => {}} />);
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
    const { container, rerender } = render(<InboxView onOpen={() => {}} />);
    await flush();
    expect(container.querySelector('.btn--primary')).toBeNull();

    rerender(<InboxView onOpen={() => {}} onRecord={() => {}} />);
    await flush();
    // The «Записать» primary button now appears in the header.
    expect(screen.getByText('Записать')).toBeTruthy();
  });
});
