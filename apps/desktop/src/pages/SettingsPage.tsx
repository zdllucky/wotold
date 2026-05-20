import { useEffect, useState } from 'react';

import {
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type ProviderPath,
  type SttProvider,
} from '../api/settings';
import { Button, Card, InputField, SelectField, Toolbar } from '../ui';
import { AccountSection } from './AccountSection';
import { ByoKeysSection } from './ByoKeysSection';
import { PermissionsSection } from './PermissionsSection';

function isSttProvider(v: string | null): v is SttProvider {
  return v === 'auto' || v === 'soniox' || v === 'gladia';
}

function isProviderPath(v: string | null): v is ProviderPath {
  return v === 'managed' || v === 'byo';
}

function isValidProxyUrl(v: string): boolean {
  if (!v) return true;
  try {
    const u = new URL(v);
    return u.protocol === 'https:' || u.protocol === 'http:';
  } catch {
    return false;
  }
}

export function SettingsPage() {
  const [loading, setLoading] = useState(true);
  const [sttProvider, setSttProvider] = useState<SttProvider>(SETTINGS_DEFAULTS.STT_PROVIDER);
  const [providerPath, setProviderPath] = useState<ProviderPath>(SETTINGS_DEFAULTS.PROVIDER_PATH);
  const [llmModel, setLlmModel] = useState<string>(SETTINGS_DEFAULTS.LLM_MODEL);
  const [proxyUrl, setProxyUrl] = useState<string>('');
  const [proxyUrlError, setProxyUrlError] = useState<string | null>(null);
  const [showAdvancedProxy, setShowAdvancedProxy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const [stt, path, model, proxy] = await Promise.all([
          getSetting(SETTINGS_KEYS.STT_PROVIDER),
          getSetting(SETTINGS_KEYS.PROVIDER_PATH),
          getSetting(SETTINGS_KEYS.LLM_MODEL),
          getSetting(SETTINGS_KEYS.PROXY_BASE_URL),
        ]);
        if (isSttProvider(stt)) setSttProvider(stt);
        if (isProviderPath(path)) setProviderPath(path);
        if (model) setLlmModel(model);
        if (proxy) setProxyUrl(proxy);
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

  // UX: эффективный proxy URL = user-override ИЛИ production default.
  const effectiveProxyUrl = proxyUrl.trim() || SETTINGS_DEFAULTS.PROXY_BASE_URL;

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
              <span className="radio-row-hint">
                Out-of-the-box: запросы идут через прокси Wotold с квотой Free-тира.
                Свой ключ не нужен.
              </span>
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
              <span className="radio-row-hint">
                Свои ключи Soniox/Gladia/Anthropic. Хранятся в системном Keychain.
              </span>
            </span>
          </label>
        </Card>
      </div>

      {/* Managed: показываем эффективный URL + advanced collapse для override.
          В BYO режиме прокси не нужен — секция скрыта. */}
      {providerPath === 'managed' && (
        <div className="settings-section">
          <div className="settings-row-between">
            <h3 className="settings-section-title">Прокси</h3>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowAdvancedProxy((v) => !v)}
            >
              {showAdvancedProxy ? '✕ Закрыть' : 'Advanced'}
            </Button>
          </div>
          <Card compact>
            <p className="text-muted">
              Endpoint: <code className="text-mono">{effectiveProxyUrl}</code>
              {!proxyUrl.trim() && <span className="text-subtle"> · default</span>}
            </p>
            {showAdvancedProxy && (
              <InputField
                label="Custom Proxy URL"
                type="text"
                placeholder={SETTINGS_DEFAULTS.PROXY_BASE_URL}
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                onBlur={() => {
                  const trimmed = proxyUrl.trim();
                  if (!isValidProxyUrl(trimmed)) {
                    setProxyUrlError('URL должен быть http:// или https://');
                    return;
                  }
                  setProxyUrl(trimmed);
                  setProxyUrlError(null);
                  void persist(SETTINGS_KEYS.PROXY_BASE_URL, trimmed);
                }}
                hint="Override для staging или self-hosted прокси. Оставь пустым для default."
                error={proxyUrlError ?? undefined}
              />
            )}
          </Card>
        </div>
      )}

      {/* BYO: показываем ключи только при выборе BYO. В managed-режиме они не нужны. */}
      {providerPath === 'byo' && (
        <div className="settings-section">
          <h3 className="settings-section-title">BYO API ключи</h3>
          <Card compact>
            <ByoKeysSection />
          </Card>
        </div>
      )}

      <div className="settings-section">
        <h3 className="settings-section-title">Аккаунт (SSO)</h3>
        <AccountSection />
      </div>

      <p className="hint">Все изменения сохраняются автоматически.</p>
    </section>
  );
}
