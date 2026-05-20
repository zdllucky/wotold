// [B17] SettingsPage — exact match per docs/design/atelier-v2/_reference/atelier-2.jsx §9.
//
// Inner 220px rail (Настройки nav) + flex content with per-section layout:
//   - Eyebrow "Настройки · {section.label}"
//   - .display 40 headline
//   - .subtitle lede
//   - Section content
//
// "Источник сервисов" — rounded-pill 2-button path toggle с italic right hint.
// "Ключи" — field-label + ●подключён/●пусто mono caps right + .input + italic hint.

import { useEffect, useState, type ReactNode } from 'react';
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
import { InputField, SelectField } from '../ui';
import { AccountSection } from './AccountSection';
import { AppearanceSection } from './AppearanceSection';
import { ByoKeysSection } from './ByoKeysSection';
import { PermissionsSection } from './PermissionsSection';
import { UsageSection } from './UsageSection';

type SectionId =
  | 'account'
  | 'appearance'
  | 'permissions'
  | 'stt'
  | 'path'
  | 'keys'
  | 'proxy'
  | 'usage'
  | 'privacy';

interface SectionMeta {
  id: SectionId;
  label: string;
  hidden?: boolean;
}

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
  const [section, setSection] = useState<SectionId>('appearance');
  const [sttProvider, setSttProvider] = useState<SttProvider>(SETTINGS_DEFAULTS.STT_PROVIDER);
  const [providerPath, setProviderPath] = useState<ProviderPath>(SETTINGS_DEFAULTS.PROVIDER_PATH);
  const [llmModel, setLlmModel] = useState<string>(SETTINGS_DEFAULTS.LLM_MODEL);
  const [preferredLanguage, setPreferredLanguage] = useState<PreferredLanguage>(
    SETTINGS_DEFAULTS.PREFERRED_LANGUAGE,
  );
  const [proxyUrl, setProxyUrl] = useState<string>('');
  const [proxyUrlError, setProxyUrlError] = useState<string | null>(null);
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

  const effectiveProxyUrl = proxyUrl.trim() || SETTINGS_DEFAULTS.PROXY_BASE_URL;

  const NAV: SectionMeta[] = [
    { id: 'appearance', label: 'Внешний вид' },
    { id: 'account', label: 'Учётная запись' },
    { id: 'permissions', label: 'Разрешения' },
    { id: 'stt', label: 'Распознавание речи' },
    { id: 'path', label: 'Источник сервисов' },
    { id: 'keys', label: 'Ключи (BYO)', hidden: providerPath !== 'byo' },
    { id: 'proxy', label: 'Сервер Wotold', hidden: providerPath !== 'managed' },
    { id: 'usage', label: 'Использование', hidden: providerPath !== 'managed' },
    { id: 'privacy', label: 'Конфиденциальность' },
  ];

  const activeMeta = NAV.find((s) => s.id === section) ?? NAV[0]!;

  return (
    <div
      style={{
        margin: '-34px -44px',
        display: 'flex',
        minHeight: '100%',
      }}
    >
      {/* Inner settings rail */}
      <div
        style={{
          width: 220,
          padding: '32px 22px',
          borderRight: '1px solid var(--line-soft)',
          flexShrink: 0,
        }}
      >
        <div className="small-caps" style={{ marginBottom: 14 }}>
          Настройки
        </div>
        {NAV.filter((s) => !s.hidden).map((s) => (
          <button
            key={s.id}
            type="button"
            className={`nav-item${section === s.id ? ' nav-item--active' : ''}`}
            onClick={() => setSection(s.id)}
            aria-current={section === s.id ? 'page' : undefined}
            style={{ fontSize: 14 }}
          >
            {s.label}
          </button>
        ))}
        {savedTick > 0 && (
          <div
            role="status"
            aria-live="polite"
            className="small-caps"
            style={{ marginTop: 18, color: 'var(--success)', fontSize: 11 }}
          >
            ✓ Сохранено
          </div>
        )}
      </div>

      {/* Content */}
      <div style={{ flex: 1, padding: '32px 44px', overflowY: 'auto' }}>
        <div className="small-caps" style={{ marginBottom: 8 }}>
          Настройки · {activeMeta.label}
        </div>
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

        {section === 'appearance' && (
          <SectionShell
            title="Внешний вид."
            lede="Тема и акцент применяются мгновенно — переключи и сравни. Все экраны реагируют одновременно."
          >
            <AppearanceSection />
          </SectionShell>
        )}

        {section === 'account' && (
          <SectionShell
            title="Аккаунт."
            lede="Облачная синхронизация скоро. Сейчас вход ничего не разблокирует — Wotold полностью работает локально без логина."
          >
            <AccountSection />
          </SectionShell>
        )}

        {section === 'permissions' && (
          <SectionShell
            title="Разрешения системы."
            lede="Wotold нужны два разрешения macOS: микрофон и запись экрана для системного звука. Без них запись не начнётся."
          >
            <PermissionsSection />
          </SectionShell>
        )}

        {section === 'stt' && (
          <SectionShell
            title="Распознавание речи."
            lede="Поставщик STT и язык вывода для рекапа. Auto переключается между Soniox и Gladia при сбоях."
          >
            <div style={{ display: 'flex', flexDirection: 'column', gap: 28, maxWidth: 540 }}>
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
              <SelectField
                label="Язык рекапа и задач"
                value={preferredLanguage}
                onChange={(e) => {
                  const v = e.target.value as PreferredLanguage;
                  setPreferredLanguage(v);
                  void persist(SETTINGS_KEYS.PREFERRED_LANGUAGE, v);
                }}
                hint="На каком языке писать рекап и задачи. 'Авто' = язык распознанной речи. Не влияет на сам STT."
              >
                {PREFERRED_LANGUAGES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.label}
                  </option>
                ))}
              </SelectField>
              <InputField
                label="LLM-модель (опционально)"
                type="text"
                value={llmModel}
                onChange={(e) => setLlmModel(e.target.value)}
                onBlur={() => {
                  const trimmed = llmModel.trim();
                  setLlmModel(trimmed);
                  void persist(SETTINGS_KEYS.LLM_MODEL, trimmed);
                }}
                placeholder="auto (определяется бэкендом)"
                hint="Пусто = прокси сам выбирает по LLM_BACKEND. Override только если знаешь что делаешь."
              />
            </div>
          </SectionShell>
        )}

        {section === 'path' && (
          <SectionShell
            title="Источник сервисов."
            lede="По умолчанию Wotold ходит через прокси с дневной бесплатной квотой. Подключи свои ключи — и запросы пойдут напрямую, без лимитов."
          >
            <PathToggle
              value={providerPath}
              onChange={(v) => {
                setProviderPath(v);
                void persist(SETTINGS_KEYS.PROVIDER_PATH, v);
                if (v === 'byo') setSection('keys');
                if (v === 'managed') setSection('proxy');
              }}
            />
            <div
              style={{
                marginTop: 24,
                padding: '14px 16px',
                background: 'var(--bg-2)',
                borderRadius: 8,
                fontFamily: 'var(--font-serif)',
                fontSize: 14,
                color: 'var(--ink-2)',
                fontStyle: 'italic',
                lineHeight: 1.55,
                maxWidth: 560,
              }}
            >
              {providerPath === 'managed'
                ? 'Через Wotold — managed-режим. Все запросы STT/LLM идут через наш прокси. Бесплатный тир: 60 минут STT и 50 тыс. токенов LLM в день. Превышение — мягкий отказ, без списаний.'
                : 'Свои ключи — BYO-режим. Wotold ходит напрямую к Soniox/Gladia/Anthropic с твоими ключами. Ключи хранятся в системном Keychain, не в БД и не в логах.'}
            </div>
          </SectionShell>
        )}

        {section === 'keys' && providerPath === 'byo' && (
          <SectionShell
            title="Свои ключи API."
            lede="Подключи ключи Soniox · Gladia · Anthropic — Wotold пойдёт напрямую, мимо нашего прокси. Ключи живут в Keychain macOS."
          >
            <div
              style={{
                background: 'var(--paper)',
                border: '1px solid var(--line)',
                borderRadius: 8,
                padding: 18,
                marginBottom: 36,
                display: 'flex',
                alignItems: 'center',
                gap: 18,
                maxWidth: 700,
                flexWrap: 'wrap',
              }}
            >
              <span className="small-caps">Путь</span>
              <PathToggle
                value={providerPath}
                onChange={(v) => {
                  setProviderPath(v);
                  void persist(SETTINGS_KEYS.PROVIDER_PATH, v);
                  if (v === 'managed') setSection('proxy');
                }}
                compact
              />
              <span
                className="muted"
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontStyle: 'italic',
                  fontSize: 13,
                  marginLeft: 'auto',
                }}
              >
                Ключи хранятся в системном Keychain
              </span>
            </div>
            <ByoKeysSection />
          </SectionShell>
        )}

        {section === 'proxy' && providerPath === 'managed' && (
          <SectionShell
            title="Сервер Wotold."
            lede="Managed-прокси — общий endpoint для STT/LLM. Можно подменить URL на staging или self-hosted, если знаешь что делаешь."
          >
            <p
              className="muted"
              style={{
                fontSize: 13,
                marginTop: 0,
                marginBottom: 14,
                maxWidth: 560,
              }}
            >
              Endpoint: <code className="mono">{effectiveProxyUrl}</code>
              {!proxyUrl.trim() && (
                <span className="subtle"> · default</span>
              )}
            </p>
            <div style={{ maxWidth: 560 }}>
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
            </div>
          </SectionShell>
        )}

        {section === 'usage' && providerPath === 'managed' && (
          <SectionShell
            title="Использование."
            lede="Дневная квота managed-режима — STT-минуты и LLM-токены. Сбрасывается каждые 24 часа. В BYO-режиме счётчик не действует."
          >
            <UsageSection />
          </SectionShell>
        )}

        {section === 'privacy' && (
          <SectionShell
            title="Конфиденциальность."
            lede="Полная очистка локальных данных. Полезно перед передачей устройства другому человеку или при отзыве согласия."
          >
            <DeleteAllDataSection />
          </SectionShell>
        )}
      </div>
    </div>
  );
}

interface SectionShellProps {
  title: string;
  lede: string;
  children: ReactNode;
}

function SectionShell({ title, lede, children }: SectionShellProps) {
  return (
    <>
      <div className="display" style={{ fontSize: 40, marginBottom: 10, marginTop: 0 }}>
        {title}
      </div>
      <p className="subtitle" style={{ maxWidth: 560, marginBottom: 32 }}>
        {lede}
      </p>
      {children}
    </>
  );
}

// ── Path toggle — rounded-pill 2-button per artboard §9 path toggle card
interface PathToggleProps {
  value: ProviderPath;
  onChange: (v: ProviderPath) => void;
  compact?: boolean;
}

function PathToggle({ value, onChange, compact }: PathToggleProps) {
  const inner: Array<[ProviderPath, string]> = [
    ['byo', 'Свои ключи'],
    ['managed', 'Через прокси'],
  ];
  return (
    <div
      style={{
        display: 'inline-flex',
        border: '1px solid var(--line)',
        borderRadius: 999,
        padding: 3,
        background: 'var(--bg)',
      }}
    >
      {inner.map(([key, label]) => {
        const active = value === key;
        return (
          <button
            key={key}
            type="button"
            className={`mono${active ? '' : ' muted'}`}
            onClick={() => onChange(key)}
            aria-pressed={active}
            style={{
              fontSize: 11,
              padding: compact ? '5px 12px' : '6px 14px',
              border: 'none',
              borderRadius: 999,
              background: active ? 'var(--accent)' : 'transparent',
              color: active ? 'var(--accent-fg)' : undefined,
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

// [B16 audit P2 / GDPR Art. 17] Полное удаление данных.
function DeleteAllDataSection() {
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleWipe = async () => {
    const ok = await ask(
      'УДАЛИТЬ ВСЕ ДАННЫЕ?\n\nЭто навсегда сотрёт:\n  • все записи звонков и аудио\n  • все контакты и voice samples\n  • сессию входа и BYO API-ключи\n\nДействие необратимо.',
      {
        title: 'Wotold — Полная очистка',
        kind: 'warning',
        okLabel: 'Удалить всё',
        cancelLabel: 'Отмена',
      },
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
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 16,
          color: 'var(--ink)',
          margin: 0,
          maxWidth: 560,
        }}
      >
        ✓ Все данные удалены. Закрой и заново открой Wotold чтобы начать с
        чистой установки.
      </p>
    );
  }

  return (
    <div style={{ maxWidth: 560 }}>
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
      <button
        type="button"
        className="btn btn--ghost"
        onClick={handleWipe}
        disabled={busy}
        style={{ color: 'var(--signal)', borderColor: 'var(--signal)' }}
      >
        {busy ? 'Удаляем…' : 'Удалить все данные'}
      </button>
    </div>
  );
}
