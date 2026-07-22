// [B18.8] Smoke tests for the rebuilt left-rail navbar — verifies every nav row
// is a uniform .navitem (NavItem wrapper), count badges render, the active view
// gets aria-current, and recent rows show a StatusCell dot.

import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import type { Call } from '../api/recording';
import { Sidebar, type RailView } from './AppSidebar';

afterEach(() => cleanup());

function mkCall(id: string, overrides: Partial<Call> = {}): Call {
  return {
    id,
    title: `Call ${id}`,
    started_at: new Date(2026, 0, 1).toISOString(),
    ended_at: null,
    duration_sec: 125,
    status: 'ready',
    provider: null,
    path_label: '',
    lang_detected: null,
    failed_reason: null,
    recap_failed_reason: null,
    pipeline_step: null,
    pipeline_pct: null,
    pipeline_eta_sec: null,
    upload_bytes: null,
    paused_at: null,
    paused_total_ms: 0,
    processing_via: null,
    call_type: null,
    call_type_confidence: null,
    summary_schema_version: null,
    summary_engine: null,
    summary_pipeline_mode: null,
    created_at: '',
    updated_at: '',
    ...overrides,
  };
}

function props(view: RailView, extra: Partial<Parameters<typeof Sidebar>[0]> = {}) {
  return {
    view,
    recKind: 'idle' as const,
    elapsed: 0,
    busy: false,
    pipelineCount: 0,
    recent: [] as Call[],
    callsCount: 12,
    contactsCount: 4,
    activeCallId: null,
    isDev: false,
    // [Q] Монитор очередей — заменил theme-toggle.
    queue: null,
    onRecord: vi.fn(),
    onPause: vi.fn(),
    onNav: vi.fn(),
    onOpenCall: vi.fn(),
    onSearch: vi.fn(),
    onCollapse: vi.fn(),
    onExpand: vi.fn(),
    onResizeStart: vi.fn(),
    ...extra,
  };
}

describe('Sidebar navbar', () => {
  test('every nav row is a uniform button.navitem with icon + label', () => {
    const { container } = render(<Sidebar {...props('inbox')} />);
    const items = container.querySelectorAll('.navitem');
    // [B24.6] inbox + contacts + assistant + settings (no recents in this fixture)
    expect(items.length).toBe(4);
    items.forEach((el) => {
      expect(el.tagName).toBe('BUTTON');
      expect(el.querySelector('.nav-label')).toBeInTheDocument();
    });
  });

  test('primary nav rows show their count badges', () => {
    const { container } = render(<Sidebar {...props('inbox')} />);
    const metas = [...container.querySelectorAll('.navitem .nav-meta')].map((m) => m.textContent);
    expect(metas).toContain('12'); // callsCount
    expect(metas).toContain('4'); // contactsCount
  });

  test('the active view item carries aria-current=page', () => {
    const { container } = render(<Sidebar {...props('contacts')} />);
    const current = container.querySelectorAll('.navitem[aria-current="page"]');
    expect(current.length).toBe(1);
  });

  test('inbox is active on both inbox and call views', () => {
    const { container } = render(<Sidebar {...props('call')} />);
    const first = container.querySelector('.navitem');
    expect(first).toHaveAttribute('aria-current', 'page');
  });

  test('recent rows render a StatusCell dot as the leading element', () => {
    const recent = [mkCall('a'), mkCall('b')];
    const { container } = render(<Sidebar {...props('inbox', { recent })} />);
    // [B24.6] 4 primary (звонки/контакты/ассистент + настройки) + 2 recents.
    expect(container.querySelectorAll('.navitem').length).toBe(6);
    // recents carry a leading .dot (StatusCell) inside a .nav-ico
    expect(container.querySelectorAll('.navitem .nav-ico .dot').length).toBeGreaterThanOrEqual(2);
  });

  // [Q] Theme-toggle заменён на QueueMonitor.
  test('bottom row hosts QueueMonitor button, no theme toggle', () => {
    const { container } = render(<Sidebar {...props('inbox')} />);
    const buttons = [...container.querySelectorAll('button')];
    expect(buttons.some((b) => /Light|Dark/.test(b.title))).toBe(false);
    expect(
      buttons.some((b) => /Очереди|queues|кезек/i.test(b.getAttribute('aria-label') ?? '')),
    ).toBe(true);
  });
});
