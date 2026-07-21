// SettingsPage — Wotold v2 (uikit) settings shell + 9 sections (B18.5, B21).
//
// Канон wk-settings.jsx: breadcrumb view-head + «✓ Сохранено», левый aside-rail
// 300px c NavItem, контент-колонка max-width 560. Каждая секция: SecLede
// (muted-абзац), группы GroupLabel (.rrail-sec) и плотные SettingRow
// (label+hint слева, контрол справа, divider между строками).
//
// BYO path / proxy URL override спрятаны сознательно: path хардкодим =
// 'managed', usage встроен в Processing (cloud branch).

import { useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { humanError } from '../api/errors';
import {
  regenerateEmptyRecaps,
  cancelBulkRecap,
  type BulkRecapProgress,
  type BulkRecapDone,
} from '../api/calls';

import {
  CALL_DETECT_COOLDOWNS,
  getSetting,
  setSetting,
  PREFERRED_LANGUAGES,
  STT_LANGUAGES,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type CallDetectCooldown,
  type PreferredLanguage,
} from '../api/settings';
import { useI18n, type TranslationKey } from '../i18n';
import {
  Button,
  Chip,
  GroupLabel,
  Icon,
  NavItem,
  Select,
  SettingRow,
  Skeleton,
  Switch,
  Wave,
} from '../ui';
import { type IconName } from '../ui/Icon';
import { HotkeyCapture } from '../components/HotkeyCapture';
import { ConfirmModal } from '../components/ConfirmModal';
import { DEFAULT_PAUSE_HOTKEY, DEFAULT_TOGGLE_HOTKEY } from '../utils/hotkey';
import { AccountSection } from './AccountSection';
import { AppearanceSection } from './AppearanceSection';
import { LabsSection } from './LabsSection';
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
  | 'labs'
  | 'maintenance'
  | 'privacy';

interface SectionMeta {
  id: SectionId;
  label: string;
  hidden?: boolean;
}

// [B18.5a] v2 rail icon per section (канон wk-settings.jsx SET_SECS:
// permissions=shield, privacy=lock).
const SECTION_ICONS: Record<SectionId, IconName> = {
  appearance: 'sun',
  account: 'user',
  processing: 'cpu',
  permissions: 'shield',
  recording: 'mic',
  speakers: 'users',
  labs: 'bolt',
  maintenance: 'refresh',
  privacy: 'lock',
};

// [B21] Muted-lede на секцию (канон SET_HEAD).
const SECTION_LEDES: Record<SectionId, TranslationKey> = {
  appearance: 'settings.ledeAppearance',
  account: 'settings.ledeAccount',
  processing: 'settings.ledeProcessing',
  permissions: 'settings.ledePermissions',
  recording: 'settings.ledeRecording',
  speakers: 'settings.ledeSpeakers',
  labs: 'settings.ledeLabs',
  maintenance: 'settings.ledeMaintenance',
  privacy: 'settings.ledePrivacy',
};

export function SettingsPage() {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  const [section, setSection] = useState<SectionId>('appearance');
  const [preferredLanguage, setPreferredLanguage] = useState<PreferredLanguage>(
    SETTINGS_DEFAULTS.PREFERRED_LANGUAGE,
  );
  const [sttLang, setSttLang] = useState<PreferredLanguage>(
    SETTINGS_DEFAULTS.STT_LANG,
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
        const [lang, sttLangVal, toggleHk, pauseHk, cdEnabled, cdCooldown] =
          await Promise.all([
            getSetting(SETTINGS_KEYS.PREFERRED_LANGUAGE),
            getSetting(SETTINGS_KEYS.STT_LANG),
            getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE),
            getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE),
            getSetting(SETTINGS_KEYS.CALL_DETECT_ENABLED),
            getSetting(SETTINGS_KEYS.CALL_DETECT_COOLDOWN_MIN),
          ]);
        if (lang) setPreferredLanguage(lang as PreferredLanguage);
        if (sttLangVal) setSttLang(sttLangVal as PreferredLanguage);
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
    // [V8.1] Rail (300px, 9 строк) + content shimmer mimics Settings layout.
    return (
      <section
        style={{ display: 'grid', gridTemplateColumns: '300px 1fr', gap: 32 }}
        aria-busy="true"
      >
        <aside style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {Array.from({ length: 9 }, (_, i) => (
            <Skeleton key={i} width="80%" height="1em" />
          ))}
        </aside>
        <div>
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

  // Sidebar — 9 sections (канонный порядок wk-settings.jsx).
  const NAV: SectionMeta[] = [
    { id: 'appearance', label: t('settings.sectionAppearance') },
    { id: 'account', label: t('settings.sectionAccount') },
    // «Обработка» — engine choice + (для cloud) дневная квота.
    { id: 'processing', label: t('settings.sectionProcessing') },
    { id: 'permissions', label: t('settings.sectionPermissions') },
    // «Запись» — языки, hotkeys, call-detect probe.
    { id: 'recording', label: t('settings.sectionRecording') },
    // «Спикеры» — voice biometric model + auto-bind.
    { id: 'speakers', label: t('settings.sectionSpeakers') },
    // [M14 T-14] «Лаборатория» — experimental feature flags.
    { id: 'labs', label: t('settings.sectionLabs') },
    // [Bulk recap] «Обслуживание» — пересоздать пустые рекапы старых звонков.
    { id: 'maintenance', label: t('settings.sectionMaintenance') },
    { id: 'privacy', label: t('settings.sectionPrivacy') },
  ];

  const activeMeta = NAV.find((s) => s.id === section) ?? NAV[0]!;

  return (
    // [B18.9] Shared shell: full-width .view-head (48px) over a flex .view-body.
    // Bleed past .app-main 34/44 padding and fill the viewport so the header bar
    // spans edge-to-edge and the 2-pane body fills below.
    <div
      className="main"
      style={{ margin: '-34px -44px', height: '100vh' }}
    >
      {/* [B18.9] Breadcrumb view-head per prototype: Настройки › {section} + saved. */}
      <div className="view-head">
        <Icon name="settings" size={17} style={{ color: 'var(--text-3)' }} />
        <span className="u-faint" style={{ fontSize: 'var(--t-13)' }}>{t('settings.title')}</span>
        <Icon name="chevronRight" size={13} style={{ color: 'var(--text-faint)' }} />
        <span style={{ fontWeight: 600 }}>{activeMeta.label}</span>
        <span
          className="set-saved"
          role="status"
          aria-live="polite"
          style={{ marginLeft: 10, opacity: savedTick > 0 ? 1 : 0 }}
        >
          {t('settings.saved')}
        </span>
      </div>
      <div className="view-body">
        {/* [B21] v2 inner settings rail — канон aside 300px + .scroll pad 8. */}
        <aside
          style={{
            width: 300,
            flex: '0 0 300px',
            borderRight: '1px solid var(--border)',
            display: 'flex',
            minHeight: 0,
          }}
        >
          <div className="scroll" style={{ flex: 1, minHeight: 0, padding: 8 }}>
            {NAV.filter((s) => !s.hidden).map((s) => (
              <NavItem
                key={s.id}
                icon={SECTION_ICONS[s.id]}
                label={s.label}
                active={section === s.id}
                current={section === s.id}
                onClick={() => setSection(s.id)}
              />
            ))}
          </div>
        </aside>

        {/* Content — канон: paddingTop 28, bottom 80, ширина .set-group 560. */}
        <div className="scroll" style={{ flex: 1, minHeight: 0, padding: '28px 44px 80px' }}>
          {error && (
            <p
              role="alert"
              style={{
                color: 'var(--danger)',
                fontFamily: 'var(--font)',
                marginBottom: 14,
                maxWidth: 560,
              }}
            >
              {error}
            </p>
          )}

          <SectionShell label={activeMeta.label} lede={t(SECTION_LEDES[section])}>
            {section === 'appearance' && <AppearanceSection />}
            {section === 'account' && <AccountSection />}
            {section === 'processing' && <LocalEngineSection />}
            {section === 'permissions' && <PermissionsSection />}
            {section === 'recording' && (
              <RecordingSection
                sttLang={sttLang}
                setSttLang={setSttLang}
                preferredLanguage={preferredLanguage}
                setPreferredLanguage={setPreferredLanguage}
                toggleHotkey={toggleHotkey}
                setToggleHotkey={setToggleHotkey}
                pauseHotkey={pauseHotkey}
                setPauseHotkey={setPauseHotkey}
                callDetectEnabled={callDetectEnabled}
                setCallDetectEnabled={setCallDetectEnabled}
                callDetectCooldown={callDetectCooldown}
                setCallDetectCooldown={setCallDetectCooldown}
                persist={persist}
              />
            )}
            {section === 'speakers' && <VoiceModelSection />}
            {section === 'labs' && <LabsSection />}
            {section === 'maintenance' && <BulkRecapSection />}
            {section === 'privacy' && <DeleteAllDataSection />}
          </SectionShell>
        </div>
      </div>
    </div>
  );
}

interface SectionShellProps {
  /** Accessible name секции — совпадает с nav-лейблом (фикс рассинхрона). */
  label: string;
  /** [B21] Видимый muted-lede (канон SecHead). */
  lede: string;
  children: ReactNode;
}

function SectionShell({ label, lede, children }: SectionShellProps) {
  return (
    <section aria-label={label} style={{ maxWidth: 560 }}>
      <p className="muted" style={{ fontSize: 13, lineHeight: 1.5, margin: '0 0 18px' }}>
        {lede}
      </p>
      {children}
    </section>
  );
}

// ── [B21] «Запись» — языки / горячие клавиши / авто-определение (канон
// SecRecording: GroupLabel + плотные SettingRow). Логика persist/invoke 1-в-1.
interface RecordingSectionProps {
  sttLang: PreferredLanguage;
  setSttLang: (v: PreferredLanguage) => void;
  preferredLanguage: PreferredLanguage;
  setPreferredLanguage: (v: PreferredLanguage) => void;
  toggleHotkey: string;
  setToggleHotkey: (v: string) => void;
  pauseHotkey: string;
  setPauseHotkey: (v: string) => void;
  callDetectEnabled: boolean;
  setCallDetectEnabled: (v: boolean) => void;
  callDetectCooldown: CallDetectCooldown;
  setCallDetectCooldown: (v: CallDetectCooldown) => void;
  persist: (key: string, value: string) => Promise<void>;
}

function RecordingSection({
  sttLang,
  setSttLang,
  preferredLanguage,
  setPreferredLanguage,
  toggleHotkey,
  setToggleHotkey,
  pauseHotkey,
  setPauseHotkey,
  callDetectEnabled,
  setCallDetectEnabled,
  callDetectCooldown,
  setCallDetectCooldown,
  persist,
}: RecordingSectionProps) {
  const { t } = useI18n();
  return (
    <div>
      <GroupLabel top={2}>{t('settings.groupLanguages')}</GroupLabel>
      <SettingRow label={t('settings.sttLangLabel')} hint={t('settings.sttLangHint')} align="top">
        <Select<PreferredLanguage>
          value={sttLang}
          options={STT_LANGUAGES.map((l) => ({ value: l.code, label: l.label }))}
          onChange={(v) => {
            setSttLang(v);
            void persist(SETTINGS_KEYS.STT_LANG, v);
          }}
        />
      </SettingRow>
      <SettingRow
        label={t('settings.sttRecapLangLabel')}
        hint={t('settings.sttRecapLangHint')}
        align="top"
        last
      >
        <Select<PreferredLanguage>
          value={preferredLanguage}
          options={PREFERRED_LANGUAGES.map((l) => ({ value: l.code, label: l.label }))}
          onChange={(v) => {
            setPreferredLanguage(v);
            void persist(SETTINGS_KEYS.PREFERRED_LANGUAGE, v);
          }}
        />
      </SettingRow>

      {/* [W1] Configurable recording hotkeys. */}
      <GroupLabel>{t('settings.groupHotkeys')}</GroupLabel>
      <SettingRow label={t('settings.hotkeyToggleLabel')} hint={t('settings.hotkeyToggleHint')}>
        <HotkeyCapture
          value={toggleHotkey}
          defaultHotkey={DEFAULT_TOGGLE_HOTKEY}
          onChange={(v) => {
            setToggleHotkey(v);
            void persist(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE, v);
          }}
        />
      </SettingRow>
      <SettingRow
        label={t('settings.hotkeyPauseLabel')}
        hint={t('settings.hotkeyPauseHint')}
        last
      >
        <HotkeyCapture
          value={pauseHotkey}
          defaultHotkey={DEFAULT_PAUSE_HOTKEY}
          onChange={(v) => {
            setPauseHotkey(v);
            void persist(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE, v);
          }}
        />
      </SettingRow>

      {/* [S1] Auto-detect call popup — opt-in R3 deviation. */}
      <GroupLabel>{t('settings.groupAutoDetect')}</GroupLabel>
      <SettingRow
        label={t('settings.callDetectRowLabel')}
        hint={
          <>
            {t('settings.callDetectCheckboxLabel')} {t('settings.callDetectHint')}
          </>
        }
        align="top"
        last={!callDetectEnabled}
      >
        <Switch
          checked={callDetectEnabled}
          label={t('settings.callDetectRowLabel')}
          onChange={(v) => {
            setCallDetectEnabled(v);
            void persist(SETTINGS_KEYS.CALL_DETECT_ENABLED, v ? '1' : '0');
            // [S2] Поднять/потушить probe сразу же.
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
        />
      </SettingRow>
      {callDetectEnabled && (
        <SettingRow
          label={t('settings.callDetectCooldownRowLabel')}
          hint={t('settings.callDetectCooldownHint')}
          align="top"
          last
        >
          <Select<CallDetectCooldown>
            value={callDetectCooldown}
            options={CALL_DETECT_COOLDOWNS.map((n) => ({
              value: n,
              label: t('settings.callDetectCooldownOption', { n }),
            }))}
            onChange={(v) => {
              setCallDetectCooldown(v);
              void persist(SETTINGS_KEYS.CALL_DETECT_COOLDOWN_MIN, v);
              // [S2] Если probe уже работает — пере-enable с новым cooldown'ом.
              if (callDetectEnabled) {
                void invoke('enable_call_detect', {
                  cooldownMin: Number.parseInt(v, 10),
                }).catch((err) => {
                  console.warn('enable_call_detect (refresh) failed', err);
                });
              }
            }}
          />
        </SettingRow>
      )}
    </div>
  );
}

// [Bulk recap, B21] «Обслуживание» — один Row «Пустые саммари» с инлайн-
// состояниями (канон SecMaintenance): idle → кнопка; working → Wave + N/M +
// Стоп; done → ok-иконка + счёт.
function BulkRecapSection() {
  const { t } = useI18n();
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<BulkRecapProgress | null>(null);
  const [result, setResult] = useState<BulkRecapDone | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unsubs: UnlistenFn[] = [];
    const attach = async () => {
      try {
        unsubs.push(
          await listen<BulkRecapProgress>('recap:bulk_progress', (e) => {
            setProgress(e.payload);
          }),
        );
        unsubs.push(
          await listen<BulkRecapDone>('recap:bulk_done', (e) => {
            setResult(e.payload);
            setRunning(false);
            setProgress(null);
          }),
        );
      } catch (err) {
        console.warn('bulk recap listeners failed:', err);
      }
    };
    void attach();
    return () => {
      for (const u of unsubs) u();
    };
  }, []);

  const start = async () => {
    setError(null);
    setResult(null);
    setRunning(true);
    try {
      const total = await regenerateEmptyRecaps();
      if (total === 0) {
        setRunning(false);
        setResult({ regenerated: 0, failed: 0, cancelled: false });
      } else {
        setProgress({ done: 0, total, call_id: '' });
      }
    } catch (e) {
      setError(humanError(e));
      setRunning(false);
    }
  };

  const stop = async () => {
    try {
      await cancelBulkRecap();
    } catch (e) {
      console.warn('cancel bulk recap:', e);
    }
  };

  const control = running ? (
    <span
      role="status"
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 8,
        color: 'var(--accent-text)',
        fontSize: 12.5,
      }}
    >
      <Wave />
      {progress
        ? t('settings.bulkRecapProgress', { done: progress.done + 1, total: progress.total })
        : t('settings.bulkRecapScanning')}
      <Button variant="ghost" size="sm" onClick={() => void stop()}>
        {t('settings.bulkRecapStop')}
      </Button>
    </span>
  ) : result ? (
    <span
      role="status"
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        color: result.failed > 0 ? 'var(--text-2)' : 'var(--ok)',
        fontSize: 12.5,
      }}
    >
      <Icon name="checkCircle" size={15} />
      {result.regenerated === 0 && result.failed === 0 && !result.cancelled
        ? t('settings.bulkRecapNoneEmpty')
        : t('settings.bulkRecapResult', {
            regenerated: result.regenerated,
            failed: result.failed,
          })}
    </span>
  ) : (
    <Button variant="default" size="sm" leading={<Icon name="refresh" size={14} />} onClick={() => void start()}>
      {t('settings.bulkRecapStart')}
    </Button>
  );

  return (
    <div>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', marginBottom: 12 }}>
          {error}
        </p>
      )}
      <SettingRow
        label={t('settings.bulkRecapRowLabel')}
        hint={t('settings.bulkRecapRowHint')}
        align="top"
        last
      >
        {control}
      </SettingRow>
    </div>
  );
}

// [B16 audit P2 / GDPR Art. 17, B21] «Приватность» — Row «Удалить все данные»
// + danger-ghost sm; done → Chip «удалено» (канон SecPrivacy).
function DeleteAllDataSection() {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);

  return (
    <div>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', marginBottom: 12 }}>
          {error}
        </p>
      )}
      <SettingRow
        label={t('settings.wipeBtn')}
        hint={done ? t('settings.wipeDone') : t('settings.wipeRowHint')}
        align="top"
        last
      >
        {done ? (
          <Chip tone="ok" size="sm" icon="check">
            {t('settings.wipeDoneChip')}
          </Chip>
        ) : (
          <Button
            variant="danger-ghost"
            size="sm"
            leading={<Icon name="trash" size={14} />}
            onClick={() => setConfirmOpen(true)}
            disabled={busy}
          >
            {busy ? t('settings.wipeBusy') : t('common.delete')}
          </Button>
        )}
      </SettingRow>
      <ConfirmModal
        open={confirmOpen}
        title={t('settings.wipeConfirmTitle')}
        body={t('settings.wipeConfirmBody')}
        confirmLabel={t('settings.wipeConfirmOk')}
        cancelLabel={t('common.cancel')}
        danger
        busy={busy}
        onCancel={() => setConfirmOpen(false)}
        onConfirm={async () => {
          setConfirmOpen(false);
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
        }}
      />
    </div>
  );
}
