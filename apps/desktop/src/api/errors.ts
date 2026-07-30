// [B16] Central error mapper — Tauri invoke errors + fetch errors →
// human-readable строки. Заменяет setError(String(e)) разбросанные по
// pages, которые показывали raw 'InvocationError: ...' юзеру.
//
// [TD-25] Строки живут в i18n, а не здесь. Раньше это были ~47 русских
// литералов в обход типизированного словаря: пользователь с en/kk видел
// happy-path переведённым, а весь unhappy-path — по-русски. Образец
// интеграции — utils/modelLabel.ts, он тоже принимает `t`.
//
// Использование:
//   const { t } = useI18n();
//   try { await invoke('foo') } catch (e) { setError(humanError(e, t)) }

import type { TranslationKey, useI18n } from '../i18n';

type TFn = ReturnType<typeof useI18n>['t'];

interface ErrorPattern {
  /** Регулярка по lower-cased error message либо exact match. */
  match: RegExp | string;
  /** Ключ человекочитаемого сообщения. */
  human: TranslationKey;
  /** Опциональный ключ подсказки что сделать. */
  hint?: TranslationKey;
}

const PATTERNS: ErrorPattern[] = [
  // Network
  {
    match: /(econnrefused|networkerror|failed to fetch|networkerror when attempting|err_internet)/i,
    human: 'errors.network.human',
    hint: 'errors.network.hint',
  },
  // [Bug-fix] Local LLM specifics — должны идти ДО generic /timeout/ паттерна
  // (иначе "local_llm_timeout" попадёт в "Запрос занял слишком долго").
  {
    match: /local_llm_timeout/i,
    human: 'errors.llmTimeout.human',
    hint: 'errors.llmTimeout.hint',
  },
  // Звонок припаркован: не хватает обязательных модулей. Идёт ДО
  // `local_engine_model_missing` — тот остался для звонков, упавших до
  // появления парковки, и текст у него про ручную установку.
  {
    match: /local_engine_not_ready/i,
    human: 'errors.notReady.human',
    hint: 'errors.notReady.hint',
  },
  {
    match: /local_engine_model_tampered/i,
    human: 'errors.modelTampered.human',
    hint: 'errors.modelTampered.hint',
  },
  {
    match: /local_engine_model_missing/i,
    human: 'errors.modelMissing.human',
    hint: 'errors.modelMissing.hint',
  },
  {
    match: /local_engine_preset_not_set/i,
    human: 'errors.presetNotSet.human',
    hint: 'errors.presetNotSet.hint',
  },
  {
    match: /local_engine_transcript_empty/i,
    human: 'errors.transcriptEmpty.human',
  },
  // [P2.2] Internal — должен быть rare, но если попал в UI значит pipeline
  // запустился без AppHandle (headless / race). Перезапуск приложения чинит.
  {
    match: /local_engine_no_app_handle/i,
    human: 'errors.noAppHandle.human',
    hint: 'errors.noAppHandle.hint',
  },
  {
    match: /local_engine_transcript_read/i,
    human: 'errors.transcriptRead.human',
    hint: 'errors.transcriptRead.hint',
  },
  // [P2.2] Local STT crash — sherpa-onnx Whisper sidecar упал на одной из
  // дорожек (mic | system). Обычно отсутствуют модели либо повреждены.
  {
    match: /local_engine_stt_failed/i,
    human: 'errors.sttFailed.human',
    hint: 'errors.sttFailed.hint',
  },
  // [P2.2] Recap JSON persisted в DB упало — disk full либо integrity
  // violation. Содержимое recap сгенерировано, но не сохранено.
  {
    match: /local_engine_recap_persist/i,
    human: 'errors.recapPersist.human',
    hint: 'errors.recapPersist.hint',
  },
  {
    match: /local_engine_llm_failed/i,
    human: 'errors.llmFailed.human',
    hint: 'errors.llmFailed.hint',
  },
  // Модель вернула саммари со всеми пустыми полями → header-only recap.
  // Бэкенд теперь не сохраняет такое молча, а возвращает ошибку.
  {
    match: /recap_blank_llm_output/i,
    human: 'errors.recapBlank.human',
    hint: 'errors.recapBlank.hint',
  },
  // Паника в фоновой задаче regen (sidecar/LLM). Не должна происходить, но
  // если попала в UI — задача корректно завершилась, spinner снят.
  {
    match: /regen_panic/i,
    human: 'errors.regenPanic.human',
    hint: 'errors.regenPanic.hint',
  },
  // [P13] Halt gate сработал — есть failed chunks, pipeline не идёт
  // дальше step 2 (Расшифровка). User должен retry failed segments
  // через accordion → P11.1 auto-resume подхватит pipeline.
  {
    match: /chunks_need_retry/i,
    human: 'errors.chunksRetry.human',
    hint: 'errors.chunksRetry.hint',
  },
  {
    match: /(timeout|timed out)/i,
    human: 'errors.timeout.human',
    hint: 'errors.timeout.hint',
  },

  // Permissions
  {
    match: /(permission denied|not authorized|tccd|tcc)/i,
    human: 'errors.permission.human',
    hint: 'errors.permission.hint',
  },
  {
    match: /(microphone|nsmicrophone)/i,
    human: 'errors.micPermission.human',
    hint: 'errors.micPermission.hint',
  },
  {
    match: /(screen[\s-]?cap|screencapture|screenrecording|nsscreen)/i,
    human: 'errors.screenPermission.human',
    hint: 'errors.screenPermission.hint',
  },

  // Recording
  {
    match: /recording already in progress/i,
    human: 'errors.alreadyRecording.human',
  },
  {
    match: /not recording/i,
    human: 'errors.notRecording.human',
  },
  {
    match: /sidecar.*not.*found|wotold-audio.*not/i,
    human: 'errors.sidecarMissing.human',
    hint: 'errors.sidecarMissing.hint',
  },

  // Storage / disk
  {
    match: /(disk full|no space|enospc)/i,
    human: 'errors.diskFull.human',
    hint: 'errors.diskFull.hint',
  },
  {
    match: /(database is locked|sqlite_busy)/i,
    human: 'errors.dbLocked.human',
    hint: 'errors.dbLocked.hint',
  },
  {
    match: /integrity_check|database corrupt/i,
    human: 'errors.dbCorrupt.human',
    hint: 'errors.dbCorrupt.hint',
  },

  // STT/LLM specifics
  {
    match: /transcript.*shape|missing field/i,
    human: 'errors.badShape.human',
    hint: 'errors.badShape.hint',
  },

  // Validation
  {
    match: /required|invalid input|bad request|400/i,
    human: 'errors.badRequest.human',
  },
  {
    match: /not found|404/i,
    human: 'errors.notFound.human',
  },

  // Cancellation
  {
    match: /(aborted|cancel)/i,
    human: 'errors.cancelled.human',
  },
];

/** Возвращает человекочитаемую строку ошибки. Если совпадения нет —
 *  возвращает строку оригинальной ошибки усечённую до 160 символов. */
export function humanError(err: unknown, t: TFn): string {
  const raw = errorToString(err, t);
  const haystack = raw.toLowerCase();

  for (const p of PATTERNS) {
    const matched =
      p.match instanceof RegExp ? p.match.test(haystack) : haystack.includes(p.match.toLowerCase());
    if (matched) {
      return p.hint ? `${t(p.human)} ${t(p.hint)}` : t(p.human);
    }
  }

  // Не нашли — возвращаем оригинал, но обрезаем длинные tech-сообщения.
  return raw.length > 160 ? `${raw.slice(0, 160)}…` : raw;
}

/** Извлекает строку из unknown. Поддерживает Error, string, обычный объект. */
function errorToString(err: unknown, t: TFn): string {
  if (err == null) return t('errors.unknown');
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message || err.toString();
  if (typeof err === 'object') {
    const e = err as Record<string, unknown>;
    if (typeof e.message === 'string') return e.message;
    try {
      return JSON.stringify(err);
    } catch {
      return String(err);
    }
  }
  return String(err);
}
