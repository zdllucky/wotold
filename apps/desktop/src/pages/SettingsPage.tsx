import { useEffect, useState } from 'react';

import {
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type ProviderPath,
  type SttProvider,
} from '../api/settings';

function isSttProvider(v: string | null): v is SttProvider {
  return v === 'auto' || v === 'soniox' || v === 'gladia';
}

function isProviderPath(v: string | null): v is ProviderPath {
  return v === 'managed' || v === 'byo';
}

export function SettingsPage() {
  const [loading, setLoading] = useState(true);
  const [sttProvider, setSttProvider] = useState<SttProvider>(SETTINGS_DEFAULTS.STT_PROVIDER);
  const [providerPath, setProviderPath] = useState<ProviderPath>(SETTINGS_DEFAULTS.PROVIDER_PATH);
  const [llmModel, setLlmModel] = useState<string>(SETTINGS_DEFAULTS.LLM_MODEL);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const [stt, path, model] = await Promise.all([
          getSetting(SETTINGS_KEYS.STT_PROVIDER),
          getSetting(SETTINGS_KEYS.PROVIDER_PATH),
          getSetting(SETTINGS_KEYS.LLM_MODEL),
        ]);
        if (isSttProvider(stt)) setSttProvider(stt);
        if (isProviderPath(path)) setProviderPath(path);
        if (model) setLlmModel(model);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const persist = async (key: string, value: string) => {
    try {
      await setSetting(key, value);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  if (loading) return <p className="hint">Загрузка…</p>;

  return (
    <section className="settings">
      <h2>Настройки</h2>

      {error && <p className="error">{error}</p>}

      <fieldset>
        <legend>Транскрипция</legend>
        <label>
          Провайдер
          <select
            value={sttProvider}
            onChange={(e) => {
              const v = e.target.value as SttProvider;
              setSttProvider(v);
              void persist(SETTINGS_KEYS.STT_PROVIDER, v);
            }}
          >
            <option value="auto">Auto (Soniox → Gladia)</option>
            <option value="soniox">Soniox</option>
            <option value="gladia">Gladia</option>
          </select>
        </label>
      </fieldset>

      <fieldset>
        <legend>LLM</legend>
        <label>
          Модель Anthropic
          <input
            type="text"
            value={llmModel}
            onChange={(e) => setLlmModel(e.target.value)}
            onBlur={() => {
              const trimmed = llmModel.trim() || SETTINGS_DEFAULTS.LLM_MODEL;
              setLlmModel(trimmed);
              void persist(SETTINGS_KEYS.LLM_MODEL, trimmed);
            }}
          />
        </label>
      </fieldset>

      <fieldset>
        <legend>Доставка партнёрских вызовов</legend>
        <label className="radio">
          <input
            type="radio"
            name="path"
            value="managed"
            checked={providerPath === 'managed'}
            onChange={() => {
              setProviderPath('managed');
              void persist(SETTINGS_KEYS.PROVIDER_PATH, 'managed');
            }}
          />
          <span>
            <strong>Managed</strong> — через прокси с квотой Free-тира
          </span>
        </label>
        <label className="radio">
          <input
            type="radio"
            name="path"
            value="byo"
            checked={providerPath === 'byo'}
            onChange={() => {
              setProviderPath('byo');
              void persist(SETTINGS_KEYS.PROVIDER_PATH, 'byo');
            }}
          />
          <span>
            <strong>BYO</strong> — свои ключи напрямую
            <span className="hint"> · хранение ключей в keychain — #47</span>
          </span>
        </label>
      </fieldset>

      <p className="hint">Все изменения сохраняются автоматически.</p>
    </section>
  );
}
