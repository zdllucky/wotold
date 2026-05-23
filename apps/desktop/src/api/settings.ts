import { invoke } from '@tauri-apps/api/core';

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>('set_setting', { key, value });
}

export const SETTINGS_KEYS = {
  ONBOARDING_DONE: 'onboarding_done',
  STT_PROVIDER: 'stt_provider',
  PROVIDER_PATH: 'provider_path',
  LLM_MODEL: 'llm_model',
  PROXY_BASE_URL: 'proxy_base_url',
  RECORDING_CONSENT_AT: 'recording_consent_at',
  /** [B13] BCP47 язык override для LLM-output. 'auto' = язык STT detection. */
  PREFERRED_LANGUAGE: 'preferred_language',
  /** [B16] Coachmarks показаны хотя бы раз — '1' = не показывать снова. */
  COACHMARKS_SEEN: 'coachmarks_seen',
  /** [B17] Atelier theme — 'light' | 'dark' | 'system'. */
  UI_THEME: 'ui.theme',
  /** [B17] Atelier accent — 'bordeaux' | 'persian' | 'ink'. */
  UI_ACCENT: 'ui.accent',
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
  /** [S7] Last logical X/Y position floating recording widget (RecFloat).
   *  Сохраняется при window Moved event, читается при show_recording_widget.
   *  Если пусто — fallback на top-right primary monitor. */
  RECORDING_WIDGET_X: 'recording.widget.x',
  RECORDING_WIDGET_Y: 'recording.widget.y',
  /** [M12.7.5] One-time announcement баннер для existing users о local engine.
   *  '1' = баннер закрыт (либо принят → переход в Settings, либо dismiss). */
  LOCAL_ENGINE_ANNOUNCEMENT_SEEN: 'local_engine_announcement_seen',
  /** [M12-v1.1] ISO timestamp когда баннер был dismiss-нут. Позволяет
   *  показывать повторно через 7 дней (в отличие от one-shot _SEEN). */
  LOCAL_ENGINE_ANNOUNCEMENT_DISMISSED_AT: 'local_engine_announcement_dismissed_at',
  /** [M12-v1.1] Permanent dismiss редискавери-чипа: '1' = не показывать. */
  LOCAL_ENGINE_INVITE_DISMISSED: 'local_engine_invite_dismissed',
  /** [M13.1.5] Feature flag для chunked pipelined transcription. '1' = ON.
   *  Default OFF — Phase 1 behind-flag rollout (см. M13_CHUNKING_PRD.md §6). */
  CHUNKED_PIPELINE: 'recording.chunked_pipeline',
  /** [M13 follow-up] Прогнать sortformer и по mic-дорожке, чтобы поймать
   *  гостевые голоса записанные через тот же микрофон. Owner-голос
   *  определяется через voice biometric match против voice_samples
   *  владельца (fallback: primary-speaker heuristic). Default ON.
   *  '0'/'false' = выключено; остальное → ON. */
  MIC_DIARIZATION_ENABLED: 'mic_diarization_enabled',
  /** [M14 T-14] Feature flag для v2 cloud_universal prompt. '1' = ON
   *  (default — текущий v2 path с decisions/open_questions/evidence).
   *  '0' = OFF emergency-disable → legacy v1 markdown-only prompt. */
  SUMMARY_V2_ENABLED: 'summary_v2_enabled',
  /** [M14 T-16 P2] Opt-in speculative decoding с 0.5B draft model для
   *  Quality (7B) preset. '1' = ON. Default OFF. Активация требует
   *  preset=Quality + downloaded 0.5B draft model. */
  SUMMARY_SPECULATIVE_DECODING: 'summary_speculative_decoding',
} as const;

export const SETTINGS_DEFAULTS = {
  STT_PROVIDER: 'auto' as SttProvider,
  PROVIDER_PATH: 'managed' as ProviderPath,
  /** Пусто → прокси использует свой default (LLM_BACKEND-зависимый).
   *  Override на конкретную модель через Settings → LLM section. */
  LLM_MODEL: '',
  /** Default proxy URL — dev-сборка целится на staging, prod на production.
   *  User override через Settings → Прокси → Advanced. */
  PROXY_BASE_URL: import.meta.env.DEV
    ? 'https://wotold-proxy-staging.animereader.workers.dev'
    : 'https://wotold-proxy.animereader.workers.dev',
  PREFERRED_LANGUAGE: 'auto' as PreferredLanguage,
  /** [V7] Default OFF — R2 паспорта: opt-in только. */
  AUTO_BIND_ENABLED: false,
  AUTO_BIND_THRESHOLD: '95' as AutoBindThreshold,
  /** [S1] Call-detect default OFF (R3 deviation opt-in). */
  CALL_DETECT_ENABLED: false,
  CALL_DETECT_COOLDOWN_MIN: '5' as CallDetectCooldown,
  /** [M13 follow-up] Mic diarization — default ON per user request.
   *  Полезно для multi-voice meeting через один микрофон. */
  MIC_DIARIZATION_ENABLED: true,
  /** [M14 T-14] Summary v2 default ON. OFF — emergency disable, recap
   *  падает на legacy v1 markdown-only prompt. */
  SUMMARY_V2_ENABLED: true,
  /** [M14 T-16 P2] Speculative decoding default OFF (Labs opt-in). */
  SUMMARY_SPECULATIVE_DECODING: false,
} as const;

/** [S1] Whitelist cooldown values 3/5/10/15 min. */
export type CallDetectCooldown = '3' | '5' | '10' | '15';
export const CALL_DETECT_COOLDOWNS: CallDetectCooldown[] = ['3', '5', '10', '15'];

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

export type SttProvider = 'auto' | 'soniox' | 'gladia';
export type ProviderPath = 'managed' | 'byo';
