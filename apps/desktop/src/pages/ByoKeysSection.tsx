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
    <div className="byo-keys">
      <p className="byo-keys-hint">
        Ключи хранятся в системном Keychain. Не пишутся в БД, логи или телеметрию.
      </p>
      {error && <p className="error">{error}</p>}
      {allMissing && (
        <p className="byo-keys-warn" role="alert">
          ⚠ Ни один ключ не задан. Записи будут падать с ошибкой авторизации —
          либо добавь ключи, либо переключись на «Через Wotold» в выборе режима.
        </p>
      )}
      {someMissing && (
        <p className="byo-keys-warn-soft">
          ⓘ Не заданы: {missingProviders.map((p) => p.label).join(', ')}. Без них
          часть pipeline (STT primary / fallback / recap) работать не будет.
        </p>
      )}
      {PROVIDERS.map((p) => {
        const present = statuses.get(p.id) ?? false;
        const draftValue = drafts.get(p.id) ?? '';
        const isBusy = busy === p.id;
        return (
          <div key={p.id} className="byo-row">
            <div className="byo-row-head">
              <span className="byo-row-label">{p.label}</span>
              {present ? (
                <Badge tone="success">сохранён</Badge>
              ) : (
                <Badge tone="neutral">пусто</Badge>
              )}
            </div>
            <p className="byo-row-hint">{p.hint}</p>
            <div className="byo-row-controls">
              <InputField
                type="password"
                placeholder={present ? '••••• (введи, чтобы заменить)' : p.placeholder}
                value={draftValue}
                onChange={(e) => setDraft(p.id, e.target.value)}
                autoComplete="off"
                disabled={isBusy}
              />
              <div className="byo-row-actions">
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
  );
}
