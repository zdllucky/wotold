// [B18.1a] Auto-update notice lifted out of HomePage (which is removed). Mounts
// at App-level so the updater (R11) keeps surfacing available versions on any
// screen. Self-contained: checks once on mount, applies via Tauri command.
// Reuses i18n keys home.update* + common.later.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';

interface AvailableUpdate {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

export function UpdateBanner() {
  const { t } = useI18n();
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<AvailableUpdate | null>('check_for_update')
      .then((u) => {
        if (u) setUpdate(u);
      })
      .catch((e: unknown) => console.warn('updater check failed', e));
  }, []);

  if (!update) return null;

  const applyUpdate = async () => {
    setInstalling(true);
    setError(null);
    try {
      await invoke('apply_update');
    } catch (e) {
      setInstalling(false);
      setError(humanError(e));
    }
  };

  return (
    <div
      className="panel panel--raised"
      role="region"
      aria-label={t('home.updateInstall')}
      style={{ margin: 'var(--s4) var(--s6) 0', padding: 'var(--s4)' }}
    >
      <p style={{ margin: 0, fontSize: 'var(--t-14)' }}>
        {t('home.updateAvailable', {
          version: update.version,
          current: update.current_version,
        })}
      </p>
      {update.notes && (
        <pre
          className="mono"
          style={{
            fontSize: 12,
            color: 'var(--text-3)',
            whiteSpace: 'pre-wrap',
            margin: '8px 0 0',
            padding: '8px 12px',
            background: 'var(--sunken)',
            borderRadius: 'var(--r-sm)',
            maxHeight: '12rem',
            overflow: 'auto',
          }}
        >
          {update.notes}
        </pre>
      )}
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: '8px 0 0', fontSize: 'var(--t-13)' }}>
          {error}
        </p>
      )}
      <div style={{ display: 'flex', gap: 10, marginTop: 10 }}>
        <button
          type="button"
          className="btn btn--primary"
          onClick={() => void applyUpdate()}
          disabled={installing}
        >
          {installing ? t('home.updateInstalling') : t('home.updateInstall')}
        </button>
        <button
          type="button"
          className="btn btn--ghost"
          onClick={() => setUpdate(null)}
          disabled={installing}
        >
          {t('common.later')}
        </button>
      </div>
    </div>
  );
}
