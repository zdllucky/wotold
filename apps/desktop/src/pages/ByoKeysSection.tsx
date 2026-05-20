import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';

import {
  deleteByoKey,
  listByoStatus,
  setByoKey,
  type ByoProvider,
} from '../api/secrets';
import { Badge, Button, InputField } from '../ui';

interface ProviderMeta {
  id: ByoProvider;
  label: string;
  placeholder: string;
  hint: string;
}

const PROVIDERS: ProviderMeta[] = [
  {
    id: 'soniox',
    label: 'Soniox',
    placeholder: 'sk_...',
    hint: 'STT primary. Получить ключ — soniox.com/console.',
  },
  {
    id: 'gladia',
    label: 'Gladia',
    placeholder: 'gl_...',
    hint: 'STT fallback. Получить ключ — app.gladia.io/api.',
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    placeholder: 'sk-ant-...',
    hint: 'LLM рекап. Получить ключ — console.anthropic.com.',
  },
];

export function ByoKeysSection() {
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
      setError('Введи значение ключа.');
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
        Ключи хранятся в системном Keychain. Не пишутся в БД, логи или телеметрию.
      </p>
      {error && (
        <p
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
            ⚠ Ни один ключ не задан. Записи будут падать с ошибкой авторизации —
            либо добавь ключи, либо переключись на «Через Wotold» в выборе режима.
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
          ⓘ Не заданы: {missingProviders.map((p) => p.label).join(', ')}. Без них
          часть pipeline (STT primary / fallback / recap) работать не будет.
        </p>
      )}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        {PROVIDERS.map((p) => {
          const present = statuses.get(p.id) ?? false;
          const draftValue = drafts.get(p.id) ?? '';
          const isBusy = busy === p.id;
          return (
            <div
              key={p.id}
              style={{
                padding: '14px 0',
                borderBottom: '1px solid var(--line-soft)',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  marginBottom: 4,
                }}
              >
                <span
                  style={{
                    fontFamily: 'var(--font-serif)',
                    fontSize: 17,
                    color: 'var(--ink)',
                  }}
                >
                  {p.label}
                </span>
                {present ? (
                  <Badge tone="success">сохранён</Badge>
                ) : (
                  <Badge tone="neutral">пусто</Badge>
                )}
              </div>
              <p
                className="muted"
                style={{ fontSize: 12, margin: '0 0 8px' }}
              >
                {p.hint}
              </p>
              <div style={{ display: 'flex', gap: 8, alignItems: 'flex-end', flexWrap: 'wrap' }}>
                <div style={{ flex: 1, minWidth: 200 }}>
                  <InputField
                    type="password"
                    placeholder={
                      present ? '••••• (введи, чтобы заменить)' : p.placeholder
                    }
                    value={draftValue}
                    onChange={(e) => setDraft(p.id, e.target.value)}
                    autoComplete="off"
                    disabled={isBusy}
                  />
                </div>
                <div style={{ display: 'flex', gap: 6, paddingBottom: 8 }}>
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={() => onSave(p.id)}
                    disabled={isBusy || !draftValue.trim()}
                    busy={isBusy}
                  >
                    Сохранить
                  </Button>
                  {present && (
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => onDelete(p.id)}
                      disabled={isBusy}
                    >
                      Удалить
                    </Button>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
