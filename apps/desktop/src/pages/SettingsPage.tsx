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
  CALL_DETECT_COOLDOWNS,
  getSetting,
  setSetting,
  PREFERRED_LANGUAGES,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type CallDetectCooldown,
  type PreferredLanguage,
} from '../api/settings';
import { useI18n } from '../i18n';
import { Select, Skeleton } from '../ui';
import { HotkeyCapture } from '../components/HotkeyCapture';
import { DEFAULT_PAUSE_HOTKEY, DEFAULT_TOGGLE_HOTKEY } from '../utils/hotkey';
import { AccountSection } from './AccountSection';
import { AppearanceSection } from './AppearanceSection';
import { LocalEngineSection } from './LocalEngineSection';
import { PermissionsSection } from './PermissionsSection';
import { VoiceModelSection } from './VoiceModelSection';

type SectionId =
  | 'account'
  | 'appearance'
  | 'permissions'
  | 'processing'
  | 'recording'
  | 'speakers'
  | 'privacy';

interface SectionMeta {
  id: SectionId;
  label: string;
  hidden?: boolean;
}

export function SettingsPage() {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  const [section, setSection] = useState<SectionId>('appearance');
  const [preferredLanguage, setPreferredLanguage] = useState<PreferredLanguage>(
    SETTINGS_DEFAULTS.PREFERRED_LANGUAGE,
  );
  const [callDetectEnabled, setCallDetectEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.CALL_DETECT_ENABLED,
  );
  const [callDetectCooldown, setCallDetectCooldown] = useState<CallDetectCooldown>(
    SETTINGS_DEFAULTS.CALL_DETECT_COOLDOWN_MIN,
  );
  // [W1] Hotkey settings — canonical string format ('Cmd+Shift+KeyR'). Пустая
  // = default из hotkey.ts. UI label/preview через HotkeyCapture.
  const [toggleHotkey, setToggleHotkey] = useState<string>('');
  const [pauseHotkey, setPauseHotkey] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const [lang, toggleHk, pauseHk, cdEnabled, cdCooldown] = await Promise.all([
          getSetting(SETTINGS_KEYS.PREFERRED_LANGUAGE),
          getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE),
          getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE),
          getSetting(SETTINGS_KEYS.CALL_DETECT_ENABLED),
          getSetting(SETTINGS_KEYS.CALL_DETECT_COOLDOWN_MIN),
        ]);
        if (lang) setPreferredLanguage(lang as PreferredLanguage);
        if (toggleHk) setToggleHotkey(toggleHk);
        if (pauseHk) setPauseHotkey(pauseHk);
        setCallDetectEnabled(cdEnabled === '1');
        if (cdCooldown && (['3', '5', '10', '15'] as const).includes(cdCooldown as CallDetectCooldown)) {
          setCallDetectCooldown(cdCooldown as CallDetectCooldown);
        }
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

  if (loading) {
    // [V8.1] Inner rail (220px) + content shimmer mimics Settings layout.
    return (
      <section
        style={{ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 32 }}
        aria-busy="true"
      >
        <aside style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {Array.from({ length: 7 }, (_, i) => (
            <Skeleton key={i} width="80%" height="1em" />
          ))}
        </aside>
        <div>
          <Skeleton width="14ch" height="0.7em" style={{ marginBottom: 12 }} />
          <Skeleton width="20ch" height="2rem" style={{ marginBottom: 8 }} />
          <Skeleton width="36ch" height="0.85em" style={{ marginBottom: 28 }} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 22 }}>
            {Array.from({ length: 4 }, (_, i) => (
              <div key={i} style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                <Skeleton width="12ch" height="0.8em" />
                <Skeleton width="100%" height="2.25rem" />
              </div>
            ))}
          </div>
        </div>
      </section>
    );
  }

  // Sidebar — 7 sections. BYO path, proxy URL override, и usage спрятаны в UI:
  // path хардкодим = 'managed', usage встроен в Processing (cloud branch).
  const NAV: SectionMeta[] = [
    { id: 'appearance', label: t('settings.sectionAppearance') },
    { id: 'account', label: t('settings.sectionAccount') },
    // «Обработка звонков» — объединяет engine choice + (для cloud) usage.
    { id: 'processing', label: t('settings.sectionProcessing') },
    { id: 'permissions', label: t('settings.sectionPermissions') },
    // «Запись» — recap language, hotkeys, call-detect probe.
    { id: 'recording', label: t('settings.sectionRecording') },
    // «Спикеры» — voice biometric model + auto-bind toggle.
    { id: 'speakers', label: t('settings.sectionSpeakers') },
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

        {section === 'processing' && (
          <SectionShell
            title={t('settings.engineTitle')}
            lede={t('settings.sectionProcessingSubtitle')}
          >
            <LocalEngineSection />
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

        {section === 'recording' && (
          <SectionShell
            title={t('settings.sttTitle')}
            lede={t('settings.sectionRecordingSubtitle')}
          >
            <div style={{ display: 'flex', flexDirection: 'column', gap: 28, maxWidth: 540 }}>
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

              {/* [W1] Configurable recording hotkeys. */}
              <div
                style={{
                  marginTop: 8,
                  paddingTop: 18,
                  borderTop: '1px solid var(--line-soft)',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 18,
                }}
              >
                <div className="field">
                  <label className="field-label">
                    {t('settings.hotkeyToggleLabel')}
                  </label>
                  <HotkeyCapture
                    value={toggleHotkey}
                    defaultHotkey={DEFAULT_TOGGLE_HOTKEY}
                    onChange={(v) => {
                      setToggleHotkey(v);
                      void persist(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE, v);
                    }}
                  />
                  <span
                    style={{
                      fontSize: 12,
                      color: 'var(--subtle)',
                      marginTop: 6,
                    }}
                  >
                    {t('settings.hotkeyToggleHint')}
                  </span>
                </div>
                <div className="field">
                  <label className="field-label">
                    {t('settings.hotkeyPauseLabel')}
                  </label>
                  <HotkeyCapture
                    value={pauseHotkey}
                    defaultHotkey={DEFAULT_PAUSE_HOTKEY}
                    onChange={(v) => {
                      setPauseHotkey(v);
                      void persist(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE, v);
                    }}
                  />
                  <span
                    style={{
                      fontSize: 12,
                      color: 'var(--subtle)',
                      marginTop: 6,
                    }}
                  >
                    {t('settings.hotkeyPauseHint')}
                  </span>
                </div>
              </div>

              {/* [S1] Auto-detect call popup — opt-in R3 deviation. */}
              <div
                style={{
                  marginTop: 8,
                  paddingTop: 18,
                  borderTop: '1px solid var(--line-soft)',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 18,
                }}
              >
                <div className="field">
                  <label className="field-label">
                    {t('settings.callDetectLabel')}
                  </label>
                  <label
                    style={{
                      display: 'flex',
                      alignItems: 'flex-start',
                      gap: 12,
                      cursor: 'pointer',
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={callDetectEnabled}
                      onChange={(e) => {
                        const v = e.target.checked;
                        setCallDetectEnabled(v);
                        void persist(
                          SETTINGS_KEYS.CALL_DETECT_ENABLED,
                          v ? '1' : '0',
                        );
                        // [S2] Поднять/потушить probe сразу же — не ждать
                        // следующего рестарта. cooldown в минутах из текущего
                        // значения селектора.
                        if (v) {
                          void invoke('enable_call_detect', {
                            cooldownMin: Number.parseInt(callDetectCooldown, 10),
                          }).catch((err) => {
                            console.warn('enable_call_detect failed', err);
                          });
                        } else {
                          void invoke('disable_call_detect').catch((err) => {
                            console.warn('disable_call_detect failed', err);
                          });
                        }
                      }}
                      style={{ marginTop: 4 }}
                    />
                    <span
                      style={{
                        fontFamily: 'var(--font-serif)',
                        fontSize: 14,
                        color: 'var(--ink-2)',
                        lineHeight: 1.5,
                      }}
                    >
                      {t('settings.callDetectCheckboxLabel')}
                    </span>
                  </label>
                  <span
                    style={{
                      fontSize: 12,
                      color: 'var(--subtle)',
                      marginTop: 6,
                      fontStyle: 'italic',
                    }}
                  >
                    {t('settings.callDetectHint')}
                  </span>
                </div>
                {callDetectEnabled && (
                  <div className="field">
                    <label className="field-label">
                      {t('settings.callDetectCooldownLabel')}
                    </label>
                    <Select<CallDetectCooldown>
                      value={callDetectCooldown}
                      options={CALL_DETECT_COOLDOWNS.map((n) => ({
                        value: n,
                        label: t('settings.callDetectCooldownOption', { n }),
                      }))}
                      onChange={(v) => {
                        setCallDetectCooldown(v);
                        void persist(
                          SETTINGS_KEYS.CALL_DETECT_COOLDOWN_MIN,
                          v,
                        );
                        // [S2] Если probe уже работает — пере-enable с новым
                        // cooldown'ом (controller сохранит value, без перезапуска).
                        if (callDetectEnabled) {
                          void invoke('enable_call_detect', {
                            cooldownMin: Number.parseInt(v, 10),
                          }).catch((err) => {
                            console.warn('enable_call_detect (refresh) failed', err);
                          });
                        }
                      }}
                    />
                    <span
                      style={{
                        fontSize: 12,
                        color: 'var(--subtle)',
                        marginTop: 2,
                      }}
                    >
                      {t('settings.callDetectCooldownHint')}
                    </span>
                  </div>
                )}
              </div>
            </div>
          </SectionShell>
        )}

        {section === 'speakers' && (
          <SectionShell
            title={t('settings.voiceTitle')}
            lede={t('settings.sectionSpeakersSubtitle')}
          >
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
