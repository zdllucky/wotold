// Счётчик распознавания речи в панели обработки (`stt:progress`).

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { ProcessingPanel } from './ProcessingPanel';
import type { Call } from '../../api/recording';

afterEach(() => cleanup());

function callAtStep(step: number): Call {
  return {
    id: 'c1',
    title: null,
    started_at: '2026-07-01T10:00:00Z',
    ended_at: null,
    duration_sec: 3600,
    status: 'processing',
    provider: null,
    path_label: 'local',
    lang_detected: null,
    failed_reason: null,
    pipeline_step: step,
    pipeline_pct: 30,
    pipeline_eta_sec: null,
  } as unknown as Call;
}

describe('ProcessingPanel — счётчик STT', () => {
  test('на шаге распознавания показывает секунды', () => {
    render(<ProcessingPanel call={callAtStep(2)} sttElapsedSec={45} />);
    expect(screen.getByRole('status').textContent).toContain('45');
  });

  test('без тиков строки нет — иначе «0 с» появлялось бы до первого события', () => {
    const { container } = render(<ProcessingPanel call={callAtStep(2)} />);
    expect(container.querySelector('[role="status"]')).toBeNull();
  });

  test('на других шагах счётчик не показывается', () => {
    const { container } = render(<ProcessingPanel call={callAtStep(4)} sttElapsedSec={45} />);
    expect(container.querySelector('[role="status"]')).toBeNull();
  });
});
