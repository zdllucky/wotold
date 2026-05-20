import { useEffect, useState } from 'react';

import {
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type ProviderPath,
  type SttProvider,
} from '../api/settings';
import { Card, InputField, SelectField, Toolbar } from '../ui';
import { PermissionsSection } from './PermissionsSection';

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
      <Toolbar title="Настройки" />

      {error && <p className="error">{error}</p>}

      <div className="settings-section">
        <h3 className="settings-section-title">Системные разрешения</h3>
        <Card compact>
          <PermissionsSection />
        </Card>
      </div>

      <div className="settings-section">
        <h3 className="settings-section-title">Транскрипция</h3>
        <Card compact>
          <SelectField
            label="Провайдер"
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
          </SelectField>
        </Card>
      </div>

      <div className="settings-section">
        <h3 className="settings-section-title">LLM</h3>
        <Card compact>
          <InputField
            label="Модель Anthropic"
            type="text"
            value={llmModel}
            onChange={(e) => setLlmModel(e.target.value)}
            onBlur={() => {
              const trimmed = llmModel.trim() || SETTINGS_DEFAULTS.LLM_MODEL;
              setLlmModel(trimmed);
              void persist(SETTINGS_KEYS.LLM_MODEL, trimmed);
            }}
          />
        </Card>
      </div>

      <div className="settings-section">
        <h3 className="settings-section-title">Доставка партнёрских вызовов</h3>
        <Card compact>
          <label className="radio-row">
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
            <span className="radio-row-text">
              <strong>Managed</strong>
              <span className="radio-row-hint">через прокси с квотой Free-тира</span>
            </span>
          </label>
          <label className="radio-row">
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
            <span className="radio-row-text">
              <strong>BYO</strong>
              <span className="radio-row-hint">свои ключи напрямую · keychain — #47</span>
            </span>
          </label>
        </Card>
      </div>

      <p className="hint">Все изменения сохраняются автоматически.</p>
    </section>
  );
}
