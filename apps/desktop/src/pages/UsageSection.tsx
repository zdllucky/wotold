// #48 (M7.5 follow-up): секция "Использование" в Settings.
//
// Полит /v1/usage один раз при mount + при manual refresh. Offline-safe:
// если прокси недоступен — показывает explanation, не блокирует UI.

import { useCallback, useEffect, useState } from 'react';
import type { UsageResponse } from '@wotold/contracts';
import { Badge, Button, Card, UsageBar } from '../ui';
import { fetchUsage } from '../api/usage';
import { bcp47, useI18n } from '../i18n';

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ready'; data: UsageResponse; loadedAt: number }
  | { kind: 'error'; message: string };

type TFn = ReturnType<typeof useI18n>['t'];

function formatResetAt(iso: string, locale: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString(bcp47(locale as Parameters<typeof bcp47>[0]), {
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

function formatSeconds(n: number, t: TFn): string {
  if (n < 60) return t('usage.secAbbr', { n });
  const mins = Math.floor(n / 60);
  const secs = n % 60;
  return secs === 0 ? t('usage.minAbbr', { n: mins }) : t('usage.minSecAbbr', { m: mins, s: secs });
}

export function UsageSection() {
  const { locale, t } = useI18n();
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
    <Card>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: 10,
          marginBottom: 16,
        }}
      >
        <div style={{ display: 'flex', gap: 8 }}>
          {state.kind === 'ready' && (
            <Badge tone="success">{t('usage.tier', { name: state.data.tier })}</Badge>
          )}
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void load()}
          disabled={state.kind === 'loading'}
        >
          {state.kind === 'loading' ? t('usage.refreshing') : t('usage.refreshLabel')}
        </Button>
      </div>

      {(state.kind === 'idle' || state.kind === 'loading') && (
        <p className="muted">{t('usage.loading')}</p>
      )}

      {state.kind === 'error' && (
        <div>
          <p className="muted" style={{ marginTop: 0, fontSize: 14 }}>
            {t('usage.errorIntro')}
          </p>
          <p
            className="subtle mono"
            style={{ fontSize: 11, margin: 0, wordBreak: 'break-all' }}
          >
            {state.message}
          </p>
        </div>
      )}

      {state.kind === 'ready' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <UsageBar
            label={t('usage.sttLabel')}
            used={state.data.sttSecondsUsed}
            limit={state.data.sttSecondsLimit}
            format={(n) => formatSeconds(n, t)}
          />
          <UsageBar
            label={t('usage.llmLabel')}
            used={state.data.llmTokensUsed}
            limit={state.data.llmTokensLimit}
            format={(v) =>
              t('usage.tokens', {
                n: v.toLocaleString(bcp47(locale as Parameters<typeof bcp47>[0])),
              })
            }
          />
          <p className="subtle" style={{ fontSize: 12, margin: 0 }}>
            {t('usage.resetAt', { date: formatResetAt(state.data.periodResetAt, locale) })}
          </p>
        </div>
      )}
    </Card>
  );
}
