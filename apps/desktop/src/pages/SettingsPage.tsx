import { useEffect, useState } from 'react';

import {
  getSetting,
  setSetting,
  PREFERRED_LANGUAGES,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type PreferredLanguage,
  type ProviderPath,
  type SttProvider,
} from '../api/settings';
import { Button, Card, InputField, SelectField, Toolbar } from '../ui';
import { AccountSection } from './AccountSection';
import { ByoKeysSection } from './ByoKeysSection';
import { PermissionsSection } from './PermissionsSection';
import { UsageSection } from './UsageSection';

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
  const [preferredLanguage, setPreferredLanguage] = useState<PreferredLanguage>(
    SETTINGS_DEFAULTS.PREFERRED_LANGUAGE,
  );
  const [proxyUrl, setProxyUrl] = useState<string>('');
  const [proxyUrlError, setProxyUrlError] = useState<string | null>(null);
  const [showAdvancedProxy, setShowAdvancedProxy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const [stt, path, model, proxy, lang] = await Promise.all([
          getSetting(SETTINGS_KEYS.STT_PROVIDER),
          getSetting(SETTINGS_KEYS.PROVIDER_PATH),
          getSetting(SETTINGS_KEYS.LLM_MODEL),
          getSetting(SETTINGS_KEYS.PROXY_BASE_URL),
          getSetting(SETTINGS_KEYS.PREFERRED_LANGUAGE),
        ]);
        if (isSttProvider(stt)) setSttProvider(stt);
        if (isProviderPath(path)) setProviderPath(path);
        if (model) setLlmModel(model);
        if (proxy) setProxyUrl(proxy);
        if (lang) setPreferredLanguage(lang as PreferredLanguage);
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
          <SelectField
            label="Язык рекапа и задач"
            value={preferredLanguage}
            onChange={(e) => {
              const v = e.target.value as PreferredLanguage;
              setPreferredLanguage(v);
              void persist(SETTINGS_KEYS.PREFERRED_LANGUAGE, v);
            }}
            hint="На каком языке писать рекап и задачи. 'Авто' = язык распознанной речи. Не влияет на сам STT — звонок на любом языке распознаётся как есть."
          >
            {PREFERRED_LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </SelectField>
          <InputField
            label="Модель (опционально)"
            type="text"
            value={llmModel}
            onChange={(e) => setLlmModel(e.target.value)}
            onBlur={() => {
              const trimmed = llmModel.trim();
              setLlmModel(trimmed);
              void persist(SETTINGS_KEYS.LLM_MODEL, trimmed);
            }}
            placeholder="auto (определяется бэкендом прокси)"
            hint="Пусто = прокси сам выбирает по LLM_BACKEND (сейчас Groq Llama 3.3 70B). Override только если знаешь что делаешь."
          />
        </Card>
      </div>

      <div className="settings-section">
        <h3 className="settings-section-title">Как пользуемся сервисами распознавания</h3>
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
              <strong>Через Wotold</strong>
              <span className="radio-row-hint">
                По умолчанию. Все запросы идут через серверы Wotold —
                свои API-ключи не нужны. Есть бесплатные лимиты.
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
              <strong>Свои API-ключи</strong>
              <span className="radio-row-hint">
                Подключи свои ключи Soniox/Gladia/Anthropic — Wotold пойдёт
                напрямую без посредника. Ключи хранятся в Keychain macOS.
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
          <h3 className="settings-section-title">Свои API-ключи</h3>
          <Card compact>
            <ByoKeysSection />
          </Card>
        </div>
      )}

      <div className="settings-section">
        <h3 className="settings-section-title">Аккаунт</h3>
        <AccountSection />
      </div>

      {/* #48: Quota indicator. Показываем только в managed-режиме —
          в BYO пользователь платит партнёрам напрямую, наша квота не действует. */}
      {providerPath === 'managed' && (
        <div className="settings-section">
          <h3 className="settings-section-title">Использование</h3>
          <UsageSection />
        </div>
      )}

      <p className="hint">Все изменения сохраняются автоматически.</p>
    </section>
  );
}
