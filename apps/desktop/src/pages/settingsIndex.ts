// [B32.4] Реестр разделов и строк настроек — общий для страницы и палитры ⌘K.
//
// До этого `SectionId` и `SECTION_ICONS` были приватными внутри SettingsPage, и
// палитра умела только «открыть Настройки» целиком. Чтобы искать «где включить
// авто-стоп» и попадать сразу на строку, нужен список, который видят оба.
//
// Реестр держит не тексты, а ключи i18n — те же самые, которыми строка
// подписана на странице. Дублировать формулировки нельзя: разойдутся при первой
// же правке копирайта, и поиск начнёт находить не то, что написано в интерфейсе.

import type { IconName } from '../ui/Icon';
import type { TranslationKey } from '../i18n';

export type SectionId =
  | 'appearance'
  | 'permissions'
  | 'processing'
  | 'recording'
  | 'speakers'
  | 'labs'
  | 'privacy'
  | 'about';

/** Порядок разделов канонный (wk-settings.jsx) — им же идёт выдача в палитре. */
export const SECTION_ORDER: SectionId[] = [
  'appearance',
  'processing',
  'permissions',
  'recording',
  'speakers',
  'labs',
  'privacy',
  'about',
];

// [B18.5a] v2 rail icon per section (канон wk-settings.jsx SET_SECS:
// permissions=shield, privacy=lock).
export const SECTION_ICONS: Record<SectionId, IconName> = {
  appearance: 'sun',
  processing: 'cpu',
  permissions: 'shield',
  recording: 'mic',
  speakers: 'users',
  labs: 'bolt',
  privacy: 'lock',
  about: 'info',
};

/** Заголовок раздела. `about` живёт в чужом неймспейсе — так исторически. */
export const SECTION_LABEL_KEYS: Record<SectionId, TranslationKey> = {
  appearance: 'settings.sectionAppearance',
  processing: 'settings.sectionProcessing',
  permissions: 'settings.sectionPermissions',
  recording: 'settings.sectionRecording',
  speakers: 'settings.sectionSpeakers',
  labs: 'settings.sectionLabs',
  privacy: 'settings.sectionPrivacy',
  about: 'update.sectionAbout',
};

/** Одна искомая строка настроек. */
export interface SettingsEntry {
  /** Якорь: он же `id` DOM-узла строки, по нему палитра подсвечивает. */
  id: string;
  section: SectionId;
  /** Ключ подписи — ровно тот, что рендерит `SettingRow`. */
  labelKey: TranslationKey;
}

/**
 * Плоский список строк. Держать в синхроне со страницей — за этим следит
 * `settingsIndex.test.ts`: он проверяет, что каждый ключ существует в локали,
 * что раздел валиден и что якоря уникальны.
 *
 * Строки, у которых нет собственной подписи (кнопки внутри блоков, статусы
 * моделей), сюда не попадают: искать их по имени всё равно нечем.
 */
export const SETTINGS_ENTRIES: SettingsEntry[] = [
  { id: 'theme', section: 'appearance', labelKey: 'settings.fieldTheme' },
  { id: 'ui-language', section: 'appearance', labelKey: 'settings.fieldLanguage' },

  { id: 'engine-preset', section: 'processing', labelKey: 'localEngine.presetLabel' },
  { id: 'keep-resident', section: 'processing', labelKey: 'localEngine.keepResidentLabel' },
  { id: 'semantic-search', section: 'processing', labelKey: 'localEngine.semanticLabel' },

  { id: 'perm-mic', section: 'permissions', labelKey: 'permissions.rowMic' },
  { id: 'perm-screen', section: 'permissions', labelKey: 'permissions.rowScreen' },
  { id: 'perm-accessibility', section: 'permissions', labelKey: 'permissions.rowAccessibility' },

  { id: 'stt-lang', section: 'recording', labelKey: 'settings.sttLangLabel' },
  { id: 'recap-lang', section: 'recording', labelKey: 'settings.sttRecapLangLabel' },
  { id: 'hotkey-toggle', section: 'recording', labelKey: 'settings.hotkeyToggleLabel' },
  { id: 'hotkey-pause', section: 'recording', labelKey: 'settings.hotkeyPauseLabel' },
  { id: 'call-detect', section: 'recording', labelKey: 'settings.callDetectRowLabel' },
  { id: 'call-detect-cooldown', section: 'recording', labelKey: 'settings.callDetectCooldownRowLabel' },
  { id: 'silence-prompt', section: 'recording', labelKey: 'settings.silencePromptRowLabel' },
  { id: 'silence-auto-stop', section: 'recording', labelKey: 'settings.silenceAutoStopRowLabel' },

  { id: 'auto-bind', section: 'speakers', labelKey: 'settings.speakersAutoBindLabel' },
  { id: 'auto-bind-threshold', section: 'speakers', labelKey: 'settings.autoBindThresholdLabel' },
  { id: 'mic-diarization', section: 'speakers', labelKey: 'settings.speakersMicDiarizationLabel' },

  { id: 'summary-v2', section: 'labs', labelKey: 'settings.summaryV2Label' },
  { id: 'speculative-decoding', section: 'labs', labelKey: 'settings.speculativeDecodingLabel' },
  { id: 'force-num-speakers', section: 'labs', labelKey: 'settings.forceNumSpeakersLabel' },

  { id: 'wipe-all-data', section: 'privacy', labelKey: 'settings.wipeBtn' },

  { id: 'app-version', section: 'about', labelKey: 'update.version' },
  { id: 'check-update', section: 'about', labelKey: 'update.check' },
  { id: 'changelog', section: 'about', labelKey: 'update.changelog' },
];

/** DOM-id строки. Префикс, чтобы якоря настроек не столкнулись с чужими id. */
export function settingDomId(id: string): string {
  return `setting-${id}`;
}

/** Куда вести и что подсветить при переходе из палитры. */
export interface SettingsTarget {
  section: SectionId;
  /** `SettingsEntry.id`; пусто — открываем раздел без подсветки. */
  highlight?: string;
}
