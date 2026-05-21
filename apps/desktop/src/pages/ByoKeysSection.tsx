import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';

import {
  deleteByoKey,
  listByoStatus,
  setByoKey,
  type ByoProvider,
} from '../api/secrets';
import { useI18n } from '../i18n';
import { Button } from '../ui';

interface ProviderMeta {
  id: ByoProvider;
  label: string;
  placeholder: string;
  hintKey: 'settings.keySonioxHint' | 'settings.keyGladiaHint' | 'settings.keyAnthropicHint';
}

const PROVIDERS: ProviderMeta[] = [
  {
    id: 'soniox',
    label: 'Soniox',
    placeholder: 'sk_...',
    hintKey: 'settings.keySonioxHint',
  },
  {
    id: 'gladia',
    label: 'Gladia',
    placeholder: 'gl_...',
    hintKey: 'settings.keyGladiaHint',
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    placeholder: 'sk-ant-...',
    hintKey: 'settings.keyAnthropicHint',
  },
];

export function ByoKeysSection() {
  const { t } = useI18n();
  const [statuses, setStatuses] = useState<Map<ByoProvider, boolean>>(new Map());
  const [drafts, setDrafts] = useState<Map<ByoProvider, string>>(new Map());
  const [busy, setBusy] = useState<ByoProvider | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const list = await listByoStatus();
      const map = new Map<ByoProvider, boolean>();
      for (const s of list) map.set(s.provider, s.present);
      setStatuses(map);
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const setDraft = (provider: ByoProvider, value: string) => {
    setDrafts((prev) => {
      const next = new Map(prev);
      next.set(provider, value);
      return next;
    });
  };

  const onSave = async (provider: ByoProvider) => {
    const value = drafts.get(provider)?.trim() ?? '';
    if (!value) {
      setError(t('settings.keyNeedValue'));
      return;
    }
    setBusy(provider);
    setError(null);
    try {
      await setByoKey(provider, value);
      setDraft(provider, '');
      await refresh();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(null);
    }
  };

  const onDelete = async (provider: ByoProvider) => {
    setBusy(provider);
    setError(null);
    try {
      await deleteByoKey(provider);
      setDraft(provider, '');
      await refresh();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(null);
    }
  };

  // [B16 audit P1]: warn если активирован BYO но ключи не заполнены —
  // юзер запустит запись, она силит fail с непонятной ошибкой 'no api key'.
  const missingProviders = PROVIDERS.filter((p) => !(statuses.get(p.id) ?? false));
  const allMissing = missingProviders.length === PROVIDERS.length;
  const someMissing = missingProviders.length > 0 && !allMissing;

  return (
    <div>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 14,
          marginTop: 0,
          marginBottom: 14,
        }}
      >
        {t('settings.keysStored')}
      </p>
      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 12,
          }}
        >
          {error}
        </p>
      )}
      {allMissing && (
        <div
          role="alert"
          className="card"
          style={{
            borderColor: 'var(--signal)',
            background: 'var(--signal-soft)',
            marginBottom: 16,
            padding: 12,
          }}
        >
          <p
            style={{
              margin: 0,
              fontFamily: 'var(--font-sans)',
              color: 'var(--ink)',
              fontSize: 13,
            }}
          >
            {t('settings.keysEmptyAll')}
          </p>
        </div>
      )}
      {someMissing && (
        <p
          className="muted"
          style={{
            fontFamily: 'var(--font-serif)',
            fontStyle: 'italic',
            fontSize: 13,
            marginBottom: 12,
          }}
        >
          {t('settings.keysSomeMissing', { names: missingProviders.map((p) => p.label).join(', ') })}
        </p>
      )}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
        {PROVIDERS.map((p) => {
          const present = statuses.get(p.id) ?? false;
          const draftValue = drafts.get(p.id) ?? '';
          const isBusy = busy === p.id;
          const inputId = `byo-${p.id}`;
          return (
            <div key={p.id} className="field">
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'baseline',
                  marginBottom: 4,
                }}
              >
                <label className="field-label" htmlFor={inputId}>
                  {p.label}
                </label>
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: 10.5,
                    color: present ? 'var(--accent)' : 'var(--muted)',
                    letterSpacing: '0.14em',
                    textTransform: 'uppercase',
                  }}
                >
                  {present ? t('settings.keyConnected') : t('settings.keyEmpty')}
                </span>
              </div>
              <input
                id={inputId}
                type="password"
                className="input"
                placeholder={
                  present ? t('settings.keyReplacePlaceholder') : p.placeholder
                }
                value={draftValue}
                onChange={(e) => setDraft(p.id, e.target.value)}
                autoComplete="off"
                disabled={isBusy}
              />
              <span
                className="muted"
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontStyle: 'italic',
                  fontSize: 13,
                  marginTop: 6,
                }}
              >
                {t(p.hintKey)}
              </span>
              <div style={{ display: 'flex', gap: 6, marginTop: 10 }}>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => onSave(p.id)}
                  disabled={isBusy || !draftValue.trim()}
                  busy={isBusy}
                >
                  {t('common.save')}
                </Button>
                {present && (
                  <Button
                    variant="danger"
                    size="sm"
                    onClick={() => onDelete(p.id)}
                    disabled={isBusy}
                  >
                    {t('common.delete')}
                  </Button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
