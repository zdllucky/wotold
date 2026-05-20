import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { humanError } from '../api/errors';

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
import type { ReactNode } from 'react';
import { Button, Card, InputField, SelectField } from '../ui';
import { AccountSection } from './AccountSection';
import { AppearanceSection } from './AppearanceSection';
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
        setError(humanError(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  // [B16] Subtle saved indicator — после persist показываем 'Сохранено ✓'
  // на ~1.5s. Раньше изменение настройки молча уходило, юзер не знал что
  // оно применилось.
  const [savedTick, setSavedTick] = useState(0);
  const persist = async (key: string, value: string) => {
    try {
      await setSetting(key, value);
      setError(null);
      setSavedTick((n) => n + 1);
    } catch (e) {
      setError(humanError(e));
    }
  };
  useEffect(() => {
    if (savedTick === 0) return;
    const t = setTimeout(() => setSavedTick(0), 1500);
    return () => clearTimeout(t);
  }, [savedTick]);

  if (loading) return <p className="muted">Загрузка…</p>;

  // UX: эффективный proxy URL = user-override ИЛИ production default.
  const effectiveProxyUrl = proxyUrl.trim() || SETTINGS_DEFAULTS.PROXY_BASE_URL;

  return (
    <section>
      <h1 className="title" style={{ fontSize: 36, marginBottom: 28 }}>
        Настройки
      </h1>

      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 14,
          }}
        >
          {error}
        </p>
      )}

      <SettingsSection title="Внешний вид">
        <Card>
          <AppearanceSection />
        </Card>
      </SettingsSection>

      <SettingsSection title="Разрешения системы">
        <Card>
          <PermissionsSection />
        </Card>
      </SettingsSection>

      <SettingsSection title="Распознавание речи">
        <Card>
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
      </SettingsSection>

      <SettingsSection title="Саммари и язык">
        <Card>
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
      </SettingsSection>

      <SettingsSection title="Источник сервисов">
        <Card>
          <RadioOption
            name="path"
            value="managed"
            checked={providerPath === 'managed'}
            onSelect={() => {
              setProviderPath('managed');
              void persist(SETTINGS_KEYS.PROVIDER_PATH, 'managed');
            }}
            title="Через Wotold"
            hint="По умолчанию. Все запросы идут через серверы Wotold — свои API-ключи не нужны. Есть бесплатные лимиты."
          />
          <RadioOption
            name="path"
            value="byo"
            checked={providerPath === 'byo'}
            onSelect={() => {
              setProviderPath('byo');
              void persist(SETTINGS_KEYS.PROVIDER_PATH, 'byo');
            }}
            title="Свои API-ключи"
            hint="Подключи свои ключи Soniox/Gladia/Anthropic — Wotold пойдёт напрямую без посредника. Ключи хранятся в Keychain macOS."
          />
        </Card>
      </SettingsSection>

      {/* Managed: показываем эффективный URL + advanced collapse для override.
          В BYO режиме прокси не нужен — секция скрыта. */}
      {providerPath === 'managed' && (
        <SettingsSection
          title="Сервер Wotold"
          actions={
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowAdvancedProxy((v) => !v)}
            >
              {showAdvancedProxy ? '✕ Закрыть' : 'Advanced'}
            </Button>
          }
        >
          <Card>
            <p
              className="muted"
              style={{
                marginTop: 0,
                marginBottom: showAdvancedProxy ? 12 : 0,
                fontSize: 13,
              }}
            >
              Endpoint: <code className="mono">{effectiveProxyUrl}</code>
              {!proxyUrl.trim() && (
                <span className="subtle"> · default</span>
              )}
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
        </SettingsSection>
      )}

      {/* BYO: показываем ключи только при выборе BYO. В managed-режиме они не нужны. */}
      {providerPath === 'byo' && (
        <SettingsSection title="Свои API-ключи">
          <Card>
            <ByoKeysSection />
          </Card>
        </SettingsSection>
      )}

      <SettingsSection title="Аккаунт">
        <AccountSection />
      </SettingsSection>

      {/* #48: Quota indicator. Показываем только в managed-режиме —
          в BYO пользователь платит партнёрам напрямую, наша квота не действует. */}
      {providerPath === 'managed' && (
        <SettingsSection title="Использование">
          <UsageSection />
        </SettingsSection>
      )}

      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 13,
          margin: '24px 0',
          display: 'flex',
          gap: 10,
          alignItems: 'baseline',
        }}
      >
        Все изменения сохраняются автоматически.
        {savedTick > 0 && (
          <span
            role="status"
            aria-live="polite"
            className="small-caps"
            style={{ color: 'var(--success)', fontSize: 11 }}
          >
            ✓ Сохранено
          </span>
        )}
      </p>

      <SettingsSection title="Конфиденциальность">
        <Card>
          <DeleteAllDataSection />
        </Card>
      </SettingsSection>
    </section>
  );
}

interface SettingsSectionProps {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}

function SettingsSection({ title, actions, children }: SettingsSectionProps) {
  return (
    <section style={{ marginBottom: 32 }}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          marginBottom: 12,
        }}
      >
        <h2 className="small-caps" style={{ margin: 0 }}>
          {title}
        </h2>
        {actions}
      </div>
      {children}
    </section>
  );
}

interface RadioOptionProps {
  name: string;
  value: string;
  checked: boolean;
  onSelect: () => void;
  title: string;
  hint: string;
}

function RadioOption({ name, value, checked, onSelect, title, hint }: RadioOptionProps) {
  return (
    <label
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 12,
        padding: '12px 0',
        cursor: 'pointer',
        borderBottom: '1px solid var(--line-soft)',
      }}
    >
      <input
        type="radio"
        name={name}
        value={value}
        checked={checked}
        onChange={onSelect}
        style={{ marginTop: 4 }}
      />
      <span
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 2,
        }}
      >
        <strong
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 16,
            color: 'var(--ink)',
            fontWeight: 500,
          }}
        >
          {title}
        </strong>
        <span
          className="muted"
          style={{ fontSize: 13, lineHeight: 1.45 }}
        >
          {hint}
        </span>
      </span>
    </label>
  );
}

// [B16 audit P2 / GDPR Art. 17]: Полное удаление данных. После клика юзер
// должен сам перезапустить app (мы не хотим управлять процессом-перезапуском
// из Tauri-команды — там нюансы с relaunch plugin).
function DeleteAllDataSection() {
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleWipe = async () => {
    const ok = await ask(
      'УДАЛИТЬ ВСЕ ДАННЫЕ?\n\nЭто навсегда сотрёт:\n  • все записи звонков и аудио\n  • все контакты и voice samples\n  • сессию входа и BYO API-ключи\n\nДействие необратимо. Подойдёт перед передачей устройства другому человеку или при отзыве согласия.',
      { title: 'Wotold — Полная очистка', kind: 'warning', okLabel: 'Удалить всё', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    setBusy(true);
    setError(null);
    try {
      await invoke('wipe_all_data');
      setDone(true);
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <div>
        <p className="muted" style={{ marginTop: 0, fontSize: 14 }}>
          ✓ Все данные удалены. Закрой и заново открой Wotold чтобы начать с
          чистой установки.
        </p>
      </div>
    );
  }

  return (
    <div>
      <p className="muted" style={{ marginTop: 0, fontSize: 14, marginBottom: 12 }}>
        Стирает все записи звонков, контакты, voice samples, сессию и BYO-ключи.
        Необратимо. Полезно при отзыве согласия или передаче устройства.
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
      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
        <Button variant="danger" onClick={handleWipe} disabled={busy} busy={busy}>
          {busy ? 'Удаляем…' : 'Удалить все данные'}
        </Button>
      </div>
    </div>
  );
}
