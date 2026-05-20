// #48 (M7.5 follow-up): секция "Использование" в Settings.
//
// Полит /v1/usage один раз при mount + при manual refresh. Offline-safe:
// если прокси недоступен — показывает explanation, не блокирует UI.

import { useCallback, useEffect, useState } from 'react';
import type { UsageResponse } from '@wotold/contracts';
import { Badge, Button, Card, UsageBar } from '../ui';
import { fetchUsage } from '../api/usage';

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ready'; data: UsageResponse; loadedAt: number }
  | { kind: 'error'; message: string };

function formatResetAt(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('ru-RU', {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
      timeZoneName: 'short',
    });
  } catch {
    return iso;
  }
}

function formatSeconds(n: number): string {
  if (n < 60) return `${n} сек`;
  const mins = Math.floor(n / 60);
  const secs = n % 60;
  return secs === 0 ? `${mins} мин` : `${mins} мин ${secs} сек`;
}

export function UsageSection() {
  const [state, setState] = useState<State>({ kind: 'idle' });

  const load = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const data = await fetchUsage();
      setState({ kind: 'ready', data, loadedAt: Date.now() });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setState({ kind: 'error', message });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <Card compact>
      <div className="settings-row-between">
        <div className="settings-row" style={{ gap: 'var(--space-2)' }}>
          {state.kind === 'ready' && (
            <Badge tone="success">tier: {state.data.tier}</Badge>
          )}
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void load()}
          disabled={state.kind === 'loading'}
        >
          {state.kind === 'loading' ? '…' : '↻ Обновить'}
        </Button>
      </div>

      {state.kind === 'idle' && (
        <p className="text-muted">Загружаем данные…</p>
      )}

      {state.kind === 'loading' && (
        <p className="text-muted">Загружаем данные…</p>
      )}

      {state.kind === 'error' && (
        <div>
          <p className="text-muted">
            Не удалось получить данные использования. Это нормально если ты
            offline или прокси не настроен.
          </p>
          <p className="text-subtle text-mono" style={{ fontSize: 'var(--text-xs)' }}>
            {state.message}
          </p>
        </div>
      )}

      {state.kind === 'ready' && (
        <>
          <UsageBar
            label="STT (распознавание речи)"
            used={state.data.sttSecondsUsed}
            limit={state.data.sttSecondsLimit}
            format={formatSeconds}
          />
          <UsageBar
            label="LLM (рекапы, нудж-вопросы)"
            used={state.data.llmTokensUsed}
            limit={state.data.llmTokensLimit}
            format={(v) => `${v.toLocaleString('ru-RU')} токенов`}
          />
          <p className="text-subtle" style={{ fontSize: 'var(--text-xs)' }}>
            Сброс счётчиков: {formatResetAt(state.data.periodResetAt)}
          </p>
        </>
      )}
    </Card>
  );
}
