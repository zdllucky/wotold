import { invoke } from '@tauri-apps/api/core';

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>('set_setting', { key, value });
}

export const SETTINGS_KEYS = {
  ONBOARDING_DONE: 'onboarding_done',
  RECORDING_CONSENT_AT: 'recording_consent_at',
  /** [B13] BCP47 язык override для LLM-output. 'auto' = язык STT detection. */
  PREFERRED_LANGUAGE: 'preferred_language',
  /** [P-fix] BCP47 язык распознавания (STT). 'auto' = whisper auto-detect
   *  (надёжный пин из трека с речью). Явный выбор форсит язык на все chunks. */
  STT_LANG: 'stt_lang',
  /** [B16] Coachmarks показаны хотя бы раз — '1' = не показывать снова. */
  COACHMARKS_SEEN: 'coachmarks_seen',
  /** Тема интерфейса — 'light' | 'dark' | 'system'. */
  UI_THEME: 'ui.theme',
  /** UI locale — 'ru' | 'kk' | 'en'. Пусто = auto-detect from system. */
  UI_LOCALE: 'ui_locale',
  /** [V7] Auto-bind speaker когда suggestion_score >= AUTO_BIND_THRESHOLD/100.
   *  '1' = включено, иначе off. Default OFF (R2 паспорта). */
  AUTO_BIND_ENABLED: 'auto_bind_enabled',
  /** [V7] Min similarity для auto-bind (90 | 95 | 98). Default '95'. */
  AUTO_BIND_THRESHOLD: 'auto_bind_threshold',
  /** [W1] Configurable hotkey для recording toggle (start↔stop).
   *  Format: `Cmd+Shift+KeyR` (см. utils/hotkey.ts serializeHotkey).
   *  Empty value = use DEFAULT_TOGGLE_HOTKEY. */
  RECORDING_HOTKEY_TOGGLE: 'recording.hotkey.toggle',
  /** [W1] Configurable hotkey для pause↔resume (W2 wires the action). */
  RECORDING_HOTKEY_PAUSE: 'recording.hotkey.pause',
  /** [S1] Opt-in авто-предложение записи когда система детектит звонок
   *  (микрофон активен другим app + frontmost = Zoom/Teams/Meet/etc).
   *  R3 паспорта запрещал auto-detect — здесь deviation как V7 для R2:
   *  default OFF, явное opt-in юзера. '1' = enabled, иначе off. */
  CALL_DETECT_ENABLED: 'call_detect.enabled',
  /** [S1] Cooldown в минутах — не предлагать снова для того же app
   *  в течение N минут после dismiss или start. Default '5'. */
  CALL_DETECT_COOLDOWN_MIN: 'call_detect.cooldown_min',
  /** [T3/R15] Подсказка «в записи тишина — остановить?» через 15 минут
   *  тишины. Порог фиксирован, настройкой регулируется только сам факт
   *  подсказки. '1' = on. Default ON. */
  SILENCE_PROMPT: 'recording.silence_prompt',
  /** [T3/R15] Через сколько минут тишины остановить запись самим и подрезать
   *  тихий хвост: '30' | '60' | '120' | 'never'. Default '30'. */
  SILENCE_AUTO_STOP: 'recording.silence_auto_stop',
  /** [S7] Last logical X/Y position floating recording widget (RecFloat).
   *  Сохраняется при window Moved event, читается при show_recording_widget.
   *  Если пусто — fallback на top-right primary monitor. */
  RECORDING_WIDGET_X: 'recording.widget.x',
  RECORDING_WIDGET_Y: 'recording.widget.y',
  /** [M13.1.5] Feature flag для chunked pipelined transcription. '1' = ON.
   *  Default OFF — Phase 1 behind-flag rollout (см. M13_CHUNKING_PRD.md §6). */
  CHUNKED_PIPELINE: 'recording.chunked_pipeline',
  /** [M14 T-14] Feature flag для v2 cloud_universal prompt. '1' = ON
   *  (default — текущий v2 path с decisions/open_questions/evidence).
   *  '0' = OFF emergency-disable → legacy v1 markdown-only prompt. */
  SUMMARY_V2_ENABLED: 'summary_v2_enabled',
} as const;

// [B21] Rust-owned ключи settings-таблицы, НАМЕРЕННО отсутствующие в этом
// реестре (persist/read только на стороне src-tauri; UI дёргает их через
// dedicated Tauri-команды, не через get/set_setting):
//   'local_engine.active_preset' — preset (local_engine/preset.rs)
//   'local_engine.keep_resident' — keep-resident флаг (pipeline/mod.rs)
//   'local_engine.hw_report'     — кеш hardware probe (commands/local_engine.rs)

export const SETTINGS_DEFAULTS = {
  PREFERRED_LANGUAGE: 'auto' as PreferredLanguage,
  STT_LANG: 'auto' as PreferredLanguage,
  /** [V7] Default OFF — R2 паспорта: opt-in только. */
  AUTO_BIND_ENABLED: false,
  AUTO_BIND_THRESHOLD: '95' as AutoBindThreshold,
  /** [S1] Call-detect default OFF (R3 deviation opt-in). */
  CALL_DETECT_ENABLED: false,
  CALL_DETECT_COOLDOWN_MIN: '5' as CallDetectCooldown,
  /** [T3/R15] Подсказка о тишине — default ON (истина в Rust: выключают
   *  только явные '0'/'false', отсутствие ключа = ON). */
  SILENCE_PROMPT: true,
  /** [T3/R15] Авто-стоп по тишине — default 30 минут. 'never' = полный
   *  opt-out; держать в синхроне с `DEFAULT_AUTO_STOP_MIN` в
   *  `commands/silence.rs`. */
  SILENCE_AUTO_STOP: '30' as SilenceAutoStop,
  /** [M14 T-14] Summary v2 default ON. OFF — emergency disable, recap
   *  падает на legacy v1 markdown-only prompt. */
  SUMMARY_V2_ENABLED: true,
} as const;

/** [S1] Whitelist cooldown values 3/5/10/15 min. */
export type CallDetectCooldown = '3' | '5' | '10' | '15';
export const CALL_DETECT_COOLDOWNS: CallDetectCooldown[] = ['3', '5', '10', '15'];

/** [T3/R15] Порог авто-стопа по тишине. `'never'` — полный opt-out (R15).
 *  Whitelist повторён в Rust (`ALLOWED_AUTO_STOP_MIN` в `commands/silence.rs`):
 *  значение приходит из БД, и бэкенд не верит ему на слово. */
export type SilenceAutoStop = '30' | '60' | '120' | 'never';
export const SILENCE_AUTO_STOPS: SilenceAutoStop[] = ['30', '60', '120', 'never'];

/** [V7] Whitelisted threshold values. '90' / '95' / '98' — три уровня риска.
 *  String union because Select<V extends string> generic constraint. */
export type AutoBindThreshold = '90' | '95' | '98';
export const AUTO_BIND_THRESHOLDS: AutoBindThreshold[] = ['90', '95', '98'];

/** Список языков для UI селектора. 'auto' = язык STT. */
export type PreferredLanguage = 'auto' | 'ru' | 'en' | 'kk' | string;
export const PREFERRED_LANGUAGES: Array<{ code: PreferredLanguage; label: string }> = [
  { code: 'auto', label: 'Автоматически (как в звонке)' },
  { code: 'ru', label: 'Русский' },
  { code: 'en', label: 'English' },
  { code: 'kk', label: 'Қазақша' },
];

/** [P-fix] Языки для STT-селектора. 'auto' = whisper auto-detect (надёжный пин
 *  из трека с речью). Явный выбор форсит язык распознавания на все chunks —
 *  лекарство от mis-detect «en» на русском звонке ([FOREIGN] спам). */
export const STT_LANGUAGES: Array<{ code: PreferredLanguage; label: string }> = [
  { code: 'auto', label: 'Автоопределение' },
  { code: 'ru', label: 'Русский' },
  { code: 'en', label: 'English' },
  { code: 'kk', label: 'Қазақша' },
];
