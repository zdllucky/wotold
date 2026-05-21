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
import { useI18n } from '../i18n';
import { InputField, Select } from '../ui';
import { AccountSection } from './AccountSection';
import { AppearanceSection } from './AppearanceSection';
import { ByoKeysSection } from './ByoKeysSection';
import { PermissionsSection } from './PermissionsSection';
import { UsageSection } from './UsageSection';
import { VoiceModelSection } from './VoiceModelSection';

type SectionId =
  | 'account'
  | 'appearance'
  | 'permissions'
  | 'stt'
  | 'path'
  | 'keys'
  | 'proxy'
  | 'usage'
  | 'voice'
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
  const { t } = useI18n();
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

  if (loading) return <p className="muted">{t('common.loading')}</p>;

  const effectiveProxyUrl = proxyUrl.trim() || SETTINGS_DEFAULTS.PROXY_BASE_URL;

  const NAV: SectionMeta[] = [
    { id: 'appearance', label: t('settings.sectionAppearance') },
    { id: 'account', label: t('settings.sectionAccount') },
    { id: 'permissions', label: t('settings.sectionPermissions') },
    { id: 'stt', label: t('settings.sectionStt') },
    { id: 'path', label: t('settings.sectionPath') },
    { id: 'keys', label: t('settings.sectionKeys'), hidden: providerPath !== 'byo' },
    { id: 'proxy', label: t('settings.sectionProxy'), hidden: providerPath !== 'managed' },
    { id: 'usage', label: t('settings.sectionUsage'), hidden: providerPath !== 'managed' },
    { id: 'voice', label: t('settings.sectionVoice') },
    { id: 'privacy', label: t('settings.sectionPrivacy') },
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
          {t('settings.title')}
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
            {t('settings.saved')}
          </div>
        )}
      </div>

      {/* Content */}
      <div style={{ flex: 1, padding: '32px 44px', overflowY: 'auto' }}>
        <div className="small-caps" style={{ marginBottom: 8 }}>
          {t('settings.breadcrumb', { section: activeMeta.label })}
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
          <SectionShell title={t('settings.appearanceTitle')} lede={t('settings.appearanceLede')}>
            <AppearanceSection />
          </SectionShell>
        )}

        {section === 'account' && (
          <SectionShell title={t('settings.accountTitle')} lede={t('settings.accountLede')}>
            <AccountSection />
          </SectionShell>
        )}

        {section === 'permissions' && (
          <SectionShell
            title={t('settings.permissionsTitle')}
            lede={t('settings.permissionsLede')}
          >
            <PermissionsSection />
          </SectionShell>
        )}

        {section === 'stt' && (
          <SectionShell title={t('settings.sttTitle')} lede={t('settings.sttLede')}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 28, maxWidth: 540 }}>
              <div className="field">
                <label className="field-label">{t('settings.sttProviderLabel')}</label>
                <Select<SttProvider>
                  value={sttProvider}
                  options={[
                    { value: 'auto', label: t('settings.sttProviderAuto') },
                    { value: 'soniox', label: 'Soniox' },
                    { value: 'gladia', label: 'Gladia' },
                  ]}
                  onChange={(v) => {
                    setSttProvider(v);
                    void persist(SETTINGS_KEYS.STT_PROVIDER, v);
                  }}
                />
              </div>
              <div className="field">
                <label className="field-label">{t('settings.sttRecapLangLabel')}</label>
                <Select<PreferredLanguage>
                  value={preferredLanguage}
                  options={PREFERRED_LANGUAGES.map((l) => ({
                    value: l.code,
                    label: l.label,
                  }))}
                  onChange={(v) => {
                    setPreferredLanguage(v);
                    void persist(SETTINGS_KEYS.PREFERRED_LANGUAGE, v);
                  }}
                />
                <span style={{ fontSize: 12, color: 'var(--subtle)', marginTop: 2 }}>
                  {t('settings.sttRecapLangHint')}
                </span>
              </div>
              <InputField
                label={t('settings.sttModelLabel')}
                type="text"
                value={llmModel}
                onChange={(e) => setLlmModel(e.target.value)}
                onBlur={() => {
                  const trimmed = llmModel.trim();
                  setLlmModel(trimmed);
                  void persist(SETTINGS_KEYS.LLM_MODEL, trimmed);
                }}
                placeholder={t('settings.sttModelPlaceholder')}
                hint={t('settings.sttModelHint')}
              />
            </div>
          </SectionShell>
        )}

        {section === 'path' && (
          <SectionShell title={t('settings.pathTitle')} lede={t('settings.pathLede')}>
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
                ? t('settings.pathManagedExplain')
                : t('settings.pathByoExplain')}
            </div>
          </SectionShell>
        )}

        {section === 'keys' && providerPath === 'byo' && (
          <SectionShell title={t('settings.keysTitle')} lede={t('settings.keysLede')}>
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
              <span className="small-caps">{t('settings.pathTogglePath')}</span>
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
                {t('settings.pathKeychainNote')}
              </span>
            </div>
            <ByoKeysSection />
          </SectionShell>
        )}

        {section === 'proxy' && providerPath === 'managed' && (
          <SectionShell title={t('settings.proxyTitle')} lede={t('settings.proxyLede')}>
            <p
              className="muted"
              style={{
                fontSize: 13,
                marginTop: 0,
                marginBottom: 14,
                maxWidth: 560,
              }}
            >
              {t('settings.proxyEndpointLabel')} <code className="mono">{effectiveProxyUrl}</code>
              {!proxyUrl.trim() && (
                <span className="subtle">{t('settings.proxyDefaultMark')}</span>
              )}
            </p>
            <div style={{ maxWidth: 560 }}>
              <InputField
                label={t('settings.proxyCustomLabel')}
                type="text"
                placeholder={SETTINGS_DEFAULTS.PROXY_BASE_URL}
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                onBlur={() => {
                  const trimmed = proxyUrl.trim();
                  if (!isValidProxyUrl(trimmed)) {
                    setProxyUrlError(t('settings.proxyInvalidUrl'));
                    return;
                  }
                  setProxyUrl(trimmed);
                  setProxyUrlError(null);
                  void persist(SETTINGS_KEYS.PROXY_BASE_URL, trimmed);
                }}
                hint={t('settings.proxyCustomHint')}
                error={proxyUrlError ?? undefined}
              />
            </div>
          </SectionShell>
        )}

        {section === 'usage' && providerPath === 'managed' && (
          <SectionShell title={t('settings.usageTitle')} lede={t('settings.usageLede')}>
            <UsageSection />
          </SectionShell>
        )}

        {section === 'voice' && (
          <SectionShell title={t('settings.voiceTitle')} lede={t('settings.voiceLede')}>
            <VoiceModelSection />
          </SectionShell>
        )}

        {section === 'privacy' && (
          <SectionShell title={t('settings.privacyTitle')} lede={t('settings.privacyLede')}>
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
  const { t } = useI18n();
  const inner: Array<[ProviderPath, string]> = [
    ['byo', t('settings.pathByoToggle')],
    ['managed', t('settings.pathManagedToggle')],
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
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleWipe = async () => {
    const ok = await ask(t('settings.wipeConfirmBody'), {
      title: t('settings.wipeConfirmTitle'),
      kind: 'warning',
      okLabel: t('settings.wipeConfirmOk'),
      cancelLabel: t('common.cancel'),
    });
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
        {t('settings.wipeDone')}
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
        {busy ? t('settings.wipeBusy') : t('settings.wipeBtn')}
      </button>
    </div>
  );
}
