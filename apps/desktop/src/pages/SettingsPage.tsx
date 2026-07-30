// SettingsPage — Wotold v2 (uikit) settings shell + 8 sections (B18.5, B21).
//
// Канон wk-settings.jsx: breadcrumb view-head + «✓ Сохранено», левый aside-rail
// 300px c NavItem, контент-колонка max-width 560. Каждая секция: SecLede
// (muted-абзац), группы GroupLabel (.rrail-sec) и плотные SettingRow
// (label+hint слева, контрол справа, divider между строками).

import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { humanError } from '../api/errors';
import {
  CALL_DETECT_COOLDOWNS,
  SILENCE_AUTO_STOPS,
  getSetting,
  setSetting,
  PREFERRED_LANGUAGES,
  STT_LANGUAGES,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type CallDetectCooldown,
  type PreferredLanguage,
  type SilenceAutoStop,
} from '../api/settings';
import { useI18n } from '../i18n';
import {
  Button,
  Chip,
  GroupLabel,
  Icon,
  IconBtn,
  NavItem,
  Select,
  SettingRow,
  Skeleton,
  Switch,
} from '../ui';
import { useResizablePanel } from '../hooks/useResizablePanel';
import { HotkeyCapture } from '../components/HotkeyCapture';
import { ConfirmModal } from '../components/ConfirmModal';
import { DEFAULT_PAUSE_HOTKEY, DEFAULT_TOGGLE_HOTKEY } from '../utils/hotkey';
import { AppearanceSection } from './AppearanceSection';
import { LabsSection } from './LabsSection';
import { LocalEngineSection } from './LocalEngineSection';
import { PermissionsSection } from './PermissionsSection';
import { AboutSection } from './AboutSection';
import { VoiceModelSection } from './VoiceModelSection';
import {
  SECTION_ICONS,
  SECTION_LABEL_KEYS,
  settingDomId,
  type SectionId,
  type SettingsTarget,
} from './settingsIndex';

// [B22] «Обслуживание» (bulk recap) удалено по фидбеку юзера — Rust-команды
// regenerate_empty_recaps/cancel_bulk_recap остаются без UI-потребителя.
//
// [B32.4] `SectionId` и `SECTION_ICONS` переехали в `settingsIndex` — их видит
// палитра ⌘K, чтобы предлагать разделы и отдельные настройки.
interface SectionMeta {
  id: SectionId;
  label: string;
  hidden?: boolean;
}

/** Сколько держится подсветка строки после перехода из палитры. */
const HIGHLIGHT_MS = 1600;

interface SettingsPageProps {
  /** [B32.4] Куда вести из палитры: раздел и (опционально) строка. */
  target?: SettingsTarget | null;
}

export function SettingsPage({ target }: SettingsPageProps = {}) {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  const [section, setSection] = useState<SectionId>(target?.section ?? 'appearance');
  // [B29.5b] Панель разделов: drag-resize + collapse до полосы иконок.
  const panel = useResizablePanel({
    min: 180,
    max: 320,
    defaultWidth: 220,
    collapseAt: 150,
    widthKey: 'wk-setw',
    collapsedKey: 'wk-set-collapsed',
  });
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
  const [silencePrompt, setSilencePrompt] = useState<boolean>(
    SETTINGS_DEFAULTS.SILENCE_PROMPT,
  );
  const [silenceAutoStop, setSilenceAutoStop] = useState<SilenceAutoStop>(
    SETTINGS_DEFAULTS.SILENCE_AUTO_STOP,
  );
  // [W1] Hotkey settings — canonical string format ('Cmd+Shift+KeyR'). Пустая
  // = default из hotkey.ts. UI label/preview через HotkeyCapture.
  const [toggleHotkey, setToggleHotkey] = useState<string>('');
  const [pauseHotkey, setPauseHotkey] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const [
          lang,
          sttLangVal,
          toggleHk,
          pauseHk,
          cdEnabled,
          cdCooldown,
          silPrompt,
          silAutoStop,
        ] = await Promise.all([
          getSetting(SETTINGS_KEYS.PREFERRED_LANGUAGE),
          getSetting(SETTINGS_KEYS.STT_LANG),
          getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE),
          getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_PAUSE),
          getSetting(SETTINGS_KEYS.CALL_DETECT_ENABLED),
          getSetting(SETTINGS_KEYS.CALL_DETECT_COOLDOWN_MIN),
          getSetting(SETTINGS_KEYS.SILENCE_PROMPT),
          getSetting(SETTINGS_KEYS.SILENCE_AUTO_STOP),
        ]);
        if (lang) setPreferredLanguage(lang as PreferredLanguage);
        if (sttLangVal) setSttLang(sttLangVal as PreferredLanguage);
        if (toggleHk) setToggleHotkey(toggleHk);
        if (pauseHk) setPauseHotkey(pauseHk);
        setCallDetectEnabled(cdEnabled === '1');
        if (cdCooldown && (['3', '5', '10', '15'] as const).includes(cdCooldown as CallDetectCooldown)) {
          setCallDetectCooldown(cdCooldown as CallDetectCooldown);
        }
        // [T3] Отсутствие ключа = ON — та же истина, что в Rust
        // (`load_silence_config` выключает только явные '0'/'false').
        setSilencePrompt(silPrompt !== '0' && silPrompt !== 'false');
        if (silAutoStop && SILENCE_AUTO_STOPS.includes(silAutoStop as SilenceAutoStop)) {
          setSilenceAutoStop(silAutoStop as SilenceAutoStop);
        }
      } catch (e) {
        setError(humanError(e, t));
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
      setError(humanError(e, t));
    }
  };
  useEffect(() => {
    if (savedTick === 0) return;
    const t = setTimeout(() => setSavedTick(0), 1500);
    return () => clearTimeout(t);
  }, [savedTick]);

  // [B32.4] Переход из палитры: открыть раздел и подсветить строку.
  useEffect(() => {
    if (!target) return;
    setSection(target.section);
  }, [target]);

  useEffect(() => {
    const anchor = target?.highlight;
    if (!anchor || loading) return;
    // Раздел отрисуется в этом же коммите, но узел появится после него —
    // ищем на следующем кадре, иначе getElementById вернёт null.
    const raf = requestAnimationFrame(() => {
      const el = document.getElementById(settingDomId(anchor));
      if (!el) return;
      // `nearest`, а не `center`: scrollIntoView листает ВСЕ прокручиваемые
      // предки, и `center` дёргал бы внешнюю раскладку заодно с колонкой.
      el.scrollIntoView({ block: 'nearest' });
      el.setAttribute('data-flash', 'true');
      // Фокус не забираем: подсветка — это ориентир, а не приглашение
      // редактировать; кража фокуса сбила бы навигацию с клавиатуры.
      window.setTimeout(() => el.removeAttribute('data-flash'), HIGHLIGHT_MS);
    });
    return () => cancelAnimationFrame(raf);
  }, [target, section, loading]);

  if (loading) {
    // [V8.1] Rail (300px, 8 строк) + content shimmer mimics Settings layout.
    return (
      <section
        style={{ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 32 }}
        aria-busy="true"
      >
        <aside style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {Array.from({ length: 8 }, (_, i) => (
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
    // «Обработка» — локальная обработка (движок + модели).
    { id: 'processing', label: t('settings.sectionProcessing') },
    { id: 'permissions', label: t('settings.sectionPermissions') },
    // «Запись» — языки, hotkeys, call-detect probe.
    { id: 'recording', label: t('settings.sectionRecording') },
    // «Спикеры» — voice biometric model + auto-bind.
    { id: 'speakers', label: t('settings.sectionSpeakers') },
    // [M14 T-14] «Лаборатория» — experimental feature flags.
    { id: 'labs', label: t('settings.sectionLabs') },
    { id: 'privacy', label: t('settings.sectionPrivacy') },
    { id: 'about', label: t(SECTION_LABEL_KEYS.about) },
  ];

  const activeMeta = NAV.find((s) => s.id === section) ?? NAV[0]!;

  return (
    // [B18.9] Shared shell: full-width .view-head (48px) over a flex .view-body.
    // Bleed past .app-main 34/44 padding and fill the viewport so the header bar
    // spans edge-to-edge and the 2-pane body fills below.
    <div className="main page-bleed">
      {/* [B18.9] Breadcrumb view-head per prototype: Настройки › {section} + saved. */}
      {/* [B32.1] data-tauri-drag-region="deep" — как в ViewHead. Здесь шапка
          собрана вручную (breadcrumb вместо иконки с заголовком), и атрибут
          забыли: окно за верх Настроек не таскалось, а со свёрнутым рейлом эта
          шапка ещё и `position: fixed` во всю ширину — мёртвой становилась вся
          верхняя кромка. Дети некликабельны, drag-скрипт их пропустит. */}
      <div className="view-head" data-tauri-drag-region="deep">
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
        {/* [B29.5b] v2 inner settings rail — .side-list с drag+collapse;
            в свёрнутом виде навигация остаётся полосой иконок (канон minirail). */}
        <aside
          className="side-list"
          data-collapsed={panel.collapsed || undefined}
          style={{ ['--side-w' as string]: `${panel.width}px` } as CSSProperties}
        >
          {panel.collapsed ? (
            <div className="side-list-mini">
              {NAV.filter((s) => !s.hidden).map((s) => (
                <IconBtn
                  key={s.id}
                  icon={SECTION_ICONS[s.id]}
                  label={s.label}
                  tip={s.label}
                  tipSide="right"
                  active={section === s.id}
                  onClick={() => setSection(s.id)}
                />
              ))}
              <span style={{ flex: 1 }} />
              <IconBtn
                icon="chevronRight"
                label={t('settings.expandPanel')}
                tip={t('settings.expandPanel')}
                tipSide="right"
                onClick={() => panel.setCollapsed(false)}
              />
            </div>
          ) : (
            <>
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
              <div className="side-list-foot">
                <IconBtn
                  icon="chevronLeft"
                  size="sm"
                  label={t('settings.collapsePanel')}
                  tip={t('settings.collapsePanel')}
                  onClick={() => panel.setCollapsed(true)}
                />
              </div>
            </>
          )}
          {/* [B30.5] Хэндл живёт и в свёрнутом виде — drag разворачивает. */}
          <div className="panel-resize" onMouseDown={panel.onResizeStart} aria-hidden="true" />
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

          <SectionShell label={activeMeta.label}>
            {section === 'appearance' && <AppearanceSection />}
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
                silencePrompt={silencePrompt}
                setSilencePrompt={setSilencePrompt}
                silenceAutoStop={silenceAutoStop}
                setSilenceAutoStop={setSilenceAutoStop}
                persist={persist}
              />
            )}
            {section === 'speakers' && <VoiceModelSection />}
            {section === 'labs' && <LabsSection />}
            {section === 'privacy' && <DeleteAllDataSection />}
            {section === 'about' && <AboutSection />}
          </SectionShell>
        </div>
      </div>
    </div>
  );
}

interface SectionShellProps {
  /** Accessible name секции — совпадает с nav-лейблом (фикс рассинхрона). */
  label: string;
  children: ReactNode;
}

// [B22] Lede-абзацы убраны по фидбеку юзера («поясняющие текста сверху не
// нужны») — остаётся только aria-label + ширина.
function SectionShell({ label, children }: SectionShellProps) {
  return (
    <section aria-label={label} style={{ maxWidth: 560 }}>
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
  silencePrompt: boolean;
  setSilencePrompt: (v: boolean) => void;
  silenceAutoStop: SilenceAutoStop;
  setSilenceAutoStop: (v: SilenceAutoStop) => void;
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
  silencePrompt,
  setSilencePrompt,
  silenceAutoStop,
  setSilenceAutoStop,
  persist,
}: RecordingSectionProps) {
  const { t } = useI18n();
  return (
    <div>
      <GroupLabel top={2}>{t('settings.groupLanguages')}</GroupLabel>
      <SettingRow settingId="stt-lang" label={t('settings.sttLangLabel')} hint={t('settings.sttLangHint')} align="top">
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
        settingId="recap-lang"
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
      <SettingRow settingId="hotkey-toggle" label={t('settings.hotkeyToggleLabel')} hint={t('settings.hotkeyToggleHint')}>
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
        settingId="hotkey-pause"
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
        settingId="call-detect"
        label={t('settings.callDetectRowLabel')}
        hint={t('settings.callDetectHint')}
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
        <SettingRow settingId="call-detect-cooldown" label={t('settings.callDetectCooldownRowLabel')} last>
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

      {/* [T3/R14] Тишина в идущей записи: подсказка и авто-стоп с подрезкой. */}
      <GroupLabel>{t('settings.groupSilence')}</GroupLabel>
      <SettingRow
        settingId="silence-prompt"
        label={t('settings.silencePromptRowLabel')}
        hint={t('settings.silencePromptHint')}
        align="top"
      >
        <Switch
          checked={silencePrompt}
          label={t('settings.silencePromptRowLabel')}
          onChange={(v) => {
            setSilencePrompt(v);
            void persist(SETTINGS_KEYS.SILENCE_PROMPT, v ? '1' : '0');
          }}
        />
      </SettingRow>
      <SettingRow
        settingId="silence-auto-stop"
        label={t('settings.silenceAutoStopRowLabel')}
        hint={t('settings.silenceAutoStopHint')}
        align="top"
        last
      >
        <Select<SilenceAutoStop>
          value={silenceAutoStop}
          options={SILENCE_AUTO_STOPS.map((v) => ({
            value: v,
            label:
              v === 'never'
                ? t('settings.silenceAutoStopNever')
                : t('settings.silenceAutoStopOption', { n: v }),
          }))}
          onChange={(v) => {
            setSilenceAutoStop(v);
            void persist(SETTINGS_KEYS.SILENCE_AUTO_STOP, v);
          }}
        />
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
        settingId="wipe-all-data"
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
            setError(humanError(e, t));
          } finally {
            setBusy(false);
          }
        }}
      />
    </div>
  );
}
