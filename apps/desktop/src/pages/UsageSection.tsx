// #48 (M7.5 follow-up), B21: «Дневная квота» внутри Обработка → cloud.
//
// Канон SecProcessing quota-блок (wk-settings.jsx :203-207): строка label +
// mono used/limit справа + .progress трек. Legacy Card/Badge/UsageBar выпилены.
// Полит /v1/usage один раз при mount + manual refresh. Offline-safe: если
// прокси недоступен — показывает explanation, не блокирует UI.

import { useCallback, useEffect, useState } from 'react';
import type { UsageResponse } from '@wotold/contracts';
import { Chip, IconBtn, Progress } from '../ui';
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

function QuotaRow({
  label,
  used,
  limit,
  format,
}: {
  label: string;
  used: number;
  limit: number | null;
  format: (n: number) => string;
}) {
  const pct = limit && limit > 0 ? (used / limit) * 100 : 0;
  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          fontSize: 12.5,
          marginBottom: 6,
        }}
      >
        <span>{label}</span>
        <span className="mono muted">
          {format(used)}
          {limit != null ? ` / ${format(limit)}` : ''}
        </span>
      </div>
      <Progress value={pct} ariaLabel={label} />
    </div>
  );
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
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        {state.kind === 'ready' && (
          <Chip tone="ok" size="sm">
            {t('usage.tier', { name: state.data.tier })}
          </Chip>
        )}
        <span style={{ flex: 1 }} />
        <IconBtn
          icon="refresh"
          size="sm"
          label={t('usage.refreshLabel')}
          onClick={() => void load()}
          disabled={state.kind === 'loading'}
        />
      </div>

      {(state.kind === 'idle' || state.kind === 'loading') && (
        <p className="muted" style={{ margin: 0, fontSize: 12.5 }}>
          {t('usage.loading')}
        </p>
      )}

      {state.kind === 'error' && (
        <div>
          <p className="muted" style={{ marginTop: 0, fontSize: 13 }}>
            {t('usage.errorIntro')}
          </p>
          <p className="subtle mono" style={{ fontSize: 11, margin: 0, wordBreak: 'break-all' }}>
            {state.message}
          </p>
        </div>
      )}

      {state.kind === 'ready' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14, maxWidth: 420 }}>
          <QuotaRow
            label={t('usage.sttLabel')}
            used={state.data.sttSecondsUsed}
            limit={state.data.sttSecondsLimit}
            format={(n) => formatSeconds(n, t)}
          />
          <QuotaRow
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
    </div>
  );
}
