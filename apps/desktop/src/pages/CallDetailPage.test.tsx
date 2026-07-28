// Phase 5 smoke test for CallDetailPage after R7 split.
//
// Goal: ensure the orchestrator + extracted sub-components + useCallDetail
// hook still mount without crashing for the three high-level states:
//  - loading (skeleton)
//  - error (allSettled rejected on call meta)
//  - notFound (allSettled resolved with null)
//
// Не покрывает: pipeline event listeners, mutating actions (delete /
// reprocess / regenerate-recap) — это outside-of-smoke и принадлежит будущим
// behavior-focused тестам. Inline component тесты остаются на месте
// (InteractiveTranscript.test.tsx, PipelineStrip.test.tsx и т.д.).

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

// Mock Tauri APIs before importing the page. `invoke` is shared across api/*
// modules (api/calls, api/contacts, api/speakers) — single mock covers all.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => `asset://${p}`,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {
    /* unlisten noop */
  }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(false),
  save: vi.fn().mockResolvedValue(null),
}));

import { invoke } from '@tauri-apps/api/core';
import { CallDetailPage } from './CallDetailPage';
import { ToastProvider } from '../ui';

// [TD-24] Ошибки несмертельных действий уходят в тост — страница требует
// провайдера, как InboxView.test.tsx.
const renderPage = (ui: React.ReactElement) =>
  render(<ToastProvider>{ui}</ToastProvider>);

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

// jsdom does not implement HTMLMediaElement.{play,pause,load} — stub так
// что useCallAudio mount/cleanup не валит «Not implemented» в stderr.
if (typeof window !== 'undefined') {
  window.HTMLMediaElement.prototype.play = () => Promise.resolve();
  window.HTMLMediaElement.prototype.pause = () => {
    /* noop */
  };
  window.HTMLMediaElement.prototype.load = () => {
    /* noop */
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// Helper — Promise resolver for awaiting microtasks after async setState.
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('CallDetailPage — smoke', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  test('renders skeleton while initial load pending', () => {
    // All invocations stall (never resolve) — page stays in loading state.
    mockInvoke.mockImplementation(() => new Promise(() => {}));
    const { container } = renderPage(
      <CallDetailPage callId="c-1" onBack={() => {}} />,
    );
    // Skeleton uses aria-busy="true" on the root section.
    expect(container.querySelector('[aria-busy="true"]')).not.toBeNull();
  });

  test('shows error alert when call meta load rejects', async () => {
    // First invoke is get_call — reject; all others resolve to safe defaults.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_call') return Promise.reject(new Error('boom'));
      if (cmd === 'list_contacts') return Promise.resolve([]);
      if (cmd === 'list_call_speakers') return Promise.resolve([]);
      if (cmd === 'list_call_action_items') return Promise.resolve([]);
      return Promise.resolve(null);
    });
    renderPage(<CallDetailPage callId="c-1" onBack={() => {}} />);
    await flush();
    // humanError(Error('boom')) → 'boom'. role=alert is asserted.
    expect(screen.getByRole('alert')).toBeInTheDocument();
  });

  test('shows notFound copy when call meta resolves to null', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_call') return Promise.resolve(null);
      if (cmd === 'list_contacts') return Promise.resolve([]);
      if (cmd === 'list_call_speakers') return Promise.resolve([]);
      if (cmd === 'list_call_action_items') return Promise.resolve([]);
      return Promise.resolve(null);
    });
    renderPage(<CallDetailPage callId="c-1" onBack={() => {}} />);
    await flush();
    // notFound copy ('Звонок не найден' / 'Call not found' depending on locale)
    // — locate by .muted class which is unique to this state branch.
    const muted = document.querySelector('p.muted');
    expect(muted).not.toBeNull();
    expect(muted?.textContent?.length ?? 0).toBeGreaterThan(0);
  });

  // [B24.5] Вкладка «Ассистент» — только у ready-звонка (SPEC §3).
  function mockCall(status: string) {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_call')
        return {
          id: 'c1',
          title: 'Тестовый звонок',
          status,
          started_at: '2026-07-22T10:00:00Z',
          duration_sec: 60,
          path_label: 'managed',
          created_at: '2026-07-22T10:00:00Z',
          updated_at: '2026-07-22T10:00:00Z',
        };
      if (cmd === 'list_call_speakers' || cmd === 'list_call_action_items') return [];
      if (cmd === 'list_call_decisions' || cmd === 'list_call_open_questions') return [];
      if (cmd === 'list_call_chunks' || cmd === 'list_contacts') return [];
      return null;
    });
  }

  test('assistant tab present for ready call, absent for processing', async () => {
    mockCall('ready');
    renderPage(<CallDetailPage callId="c1" onBack={() => {}} />);
    await flush();
    expect(screen.getByRole('tab', { name: 'Ассистент' })).toBeInTheDocument();

    cleanup();
    vi.clearAllMocks();
    mockCall('processing');
    renderPage(<CallDetailPage callId="c1" onBack={() => {}} />);
    await flush();
    expect(screen.queryByRole('tab', { name: 'Ассистент' })).not.toBeInTheDocument();
  });

  // ── [TD-24] смена звонка на лету и сбой действия ──────────────────────

  const callRow = (id: string, title: string, started: string) => ({
    id,
    title,
    status: 'ready',
    started_at: started,
    duration_sec: 60,
    path_label: 'managed',
    created_at: started,
    updated_at: started,
  });

  test('resolve по старому звонку не перезаписывает данные нового', async () => {
    // Регрессия TD-24: 12 ресурсов резолвятся вразнобой, а cancelled-флага в
    // хуке не было. Смена callId на ЖИВОМ компоненте (именно так и было до
    // key= в App) перезапускала эффект, и поздний ответ по старому звонку
    // затирал данные уже открытого нового.
    let resolveOld: ((v: unknown) => void) | null = null;
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      const id = args?.id as string | undefined;
      if (cmd === 'get_call' && id === 'old') {
        return new Promise((res) => {
          resolveOld = res;
        });
      }
      if (cmd === 'get_call') return callRow('new', 'Новый звонок', '2026-07-22T10:00:00Z');
      if (cmd === 'list_call_speakers' || cmd === 'list_call_action_items') return [];
      if (cmd === 'list_call_decisions' || cmd === 'list_call_open_questions') return [];
      if (cmd === 'list_call_chunks' || cmd === 'list_contacts') return [];
      return null;
    });

    const { rerender } = renderPage(<CallDetailPage callId="old" onBack={() => {}} />);
    // Тот же инстанс, другой звонок — состояние до key= переиспользовалось.
    rerender(
      <ToastProvider>
        <CallDetailPage callId="new" onBack={() => {}} />
      </ToastProvider>,
    );
    await flush();

    // Опоздавший ответ по старому звонку приходит уже после переключения.
    await act(async () => {
      resolveOld?.(callRow('old', 'Старый звонок', '2020-01-01T10:00:00Z'));
      await Promise.resolve();
    });
    await flush();

    expect(screen.queryByText('Старый звонок')).not.toBeInTheDocument();
    expect(screen.getAllByText('Новый звонок').length).toBeGreaterThan(0);
  });

  test('фон-реген можно остановить — кнопка зовёт cancel_reprocess', async () => {
    // Пересоздание саммари идёт минутами; до этого прервать его было нечем —
    // кнопка отмены жила только у полной переобработки.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_call') return callRow('c1', 'Звонок', '2026-07-22T10:00:00Z');
      if (cmd === 'is_call_processing') return true;
      if (cmd === 'list_call_speakers' || cmd === 'list_call_action_items') return [];
      if (cmd === 'list_call_decisions' || cmd === 'list_call_open_questions') return [];
      if (cmd === 'list_call_chunks' || cmd === 'list_contacts') return [];
      return null;
    });
    renderPage(<CallDetailPage callId="c1" onBack={() => {}} />);
    await flush();

    const stop = screen.getByRole('button', { name: /Остановить|Stop|Тоқтату/ });
    await act(async () => {
      stop.click();
      await Promise.resolve();
    });
    expect(mockInvoke).toHaveBeenCalledWith('cancel_reprocess', { callId: 'c1' });
  });
});
