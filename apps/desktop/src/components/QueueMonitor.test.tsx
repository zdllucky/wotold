// [Q] QueueMonitor — smoke RTL: 3 ресурса, busy/waiting строки, дедуп
// дорожек одного звонка, пустое состояние, badge-точка.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';

import type { Call } from '../api/recording';
import type { QueueState } from '../api/queue';
import { QueueMonitor } from './QueueMonitor';

afterEach(() => cleanup());

const calls = [
  { id: 'call-a', title: 'Продажа Acme' },
  { id: 'call-b', title: 'Продакт-синк' },
] as Call[];

const emptyQueue: QueueState = {
  resources: [
    { id: 'stt', busy: null, waiting: [] },
    { id: 'diarization', busy: null, waiting: [] },
    { id: 'llm', busy: null, waiting: [] },
  ],
};

function openMonitor() {
  fireEvent.click(screen.getByRole('button'));
}

describe('QueueMonitor', () => {
  test('renders all three resources with empty state', () => {
    render(<QueueMonitor queue={emptyQueue} calls={calls} />);
    openMonitor();
    const list = screen.getByRole('list');
    expect(list).toBeTruthy();
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    // Нет badge-точки при пустых очередях.
    expect(document.querySelector('.dot--pulse')).toBeNull();
  });

  test('shows busy call title and waiting entries with positions', () => {
    const queue: QueueState = {
      resources: [
        {
          id: 'stt',
          busy: { call_id: 'call-a' },
          waiting: [{ call_id: 'call-b' }],
        },
        { id: 'diarization', busy: null, waiting: [] },
        { id: 'llm', busy: null, waiting: [] },
      ],
    };
    render(<QueueMonitor queue={queue} calls={calls} />);
    openMonitor();
    expect(screen.getByText('Продажа Acme')).toBeTruthy();
    expect(screen.getByText(/Продакт-синк/)).toBeTruthy();
    // Badge-точка активна.
    expect(document.querySelector('.dot--pulse')).toBeTruthy();
  });

  test('dedupes two waiting tracks of the same call into one row with ×2', () => {
    const queue: QueueState = {
      resources: [
        {
          id: 'stt',
          busy: null,
          waiting: [{ call_id: 'call-a' }, { call_id: 'call-a' }],
        },
        { id: 'diarization', busy: null, waiting: [] },
        { id: 'llm', busy: null, waiting: [] },
      ],
    };
    render(<QueueMonitor queue={queue} calls={calls} />);
    openMonitor();
    expect(screen.getAllByText(/Продажа Acme/)).toHaveLength(1);
    expect(screen.getByText(/×2/)).toBeTruthy();
  });

  test('null call_id renders as system task; unknown id falls back to short id', () => {
    const queue: QueueState = {
      resources: [
        { id: 'stt', busy: null, waiting: [] },
        { id: 'diarization', busy: null, waiting: [] },
        { id: 'llm', busy: { call_id: null }, waiting: [{ call_id: 'deadbeef-1234' }] },
      ],
    };
    render(<QueueMonitor queue={queue} calls={calls} />);
    openMonitor();
    // «Служебная задача» (ru fallback вне провайдера) — по ключу systemTask.
    expect(screen.getByText(/Служебная|System/)).toBeTruthy();
    expect(screen.getByText(/deadbeef/)).toBeTruthy();
  });

  test('null queue renders all resources as free', () => {
    render(<QueueMonitor queue={null} calls={[]} />);
    openMonitor();
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
  });
});
