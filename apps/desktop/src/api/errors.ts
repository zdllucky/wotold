// [B16] Central error mapper — Tauri invoke errors + fetch errors →
// human-readable строки. Заменяет setError(String(e)) разбросанные по
// pages, которые показывали raw 'InvocationError: ...' юзеру.
//
// Использование:
//   try { await invoke('foo') } catch (e) { setError(humanError(e)) }

interface ErrorPattern {
  /** Регулярка по lower-cased error message либо exact match. */
  match: RegExp | string;
  /** Человекочитаемое сообщение на ru. */
  human: string;
  /** Опциональный hint что сделать. */
  hint?: string;
}

const PATTERNS: ErrorPattern[] = [
  // Network
  {
    match: /(econnrefused|networkerror|failed to fetch|networkerror when attempting|err_internet)/i,
    human: 'Нет соединения с сервером Wotold.',
    hint: 'Проверь интернет и попробуй ещё раз.',
  },
  // [Bug-fix] Local LLM specifics — должны идти ДО generic /timeout/ паттерна
  // (иначе "local_llm_timeout" попадёт в "Запрос занял слишком долго").
  {
    match: /local_llm_timeout/i,
    human: 'Локальная модель не успела ответить за 10 минут.',
    hint: 'Попробуй preset «Light» — он быстрее. Настройки → Локальный движок.',
  },
  {
    match: /local_engine_model_missing/i,
    human: 'Локальная модель не установлена.',
    hint: 'Скачай её в Настройках → Локальный движок.',
  },
  {
    match: /local_engine_preset_not_set/i,
    human: 'Не выбран preset локального движка.',
    hint: 'Выбери Light / Balanced / Quality в Настройках → Локальный движок.',
  },
  {
    match: /local_engine_transcript_empty/i,
    human: 'Транскрипт пустой — нечего саммаризировать.',
  },
  // [P2.2] Internal — должен быть rare, но если попал в UI значит pipeline
  // запустился без AppHandle (headless / race). Перезапуск приложения чинит.
  {
    match: /local_engine_no_app_handle/i,
    human: 'Внутренняя ошибка приложения.',
    hint: 'Перезапусти Wotold и попробуй снова.',
  },
  // [P2.2] Чтение merged transcript.md с диска упало — disk / permissions /
  // race с уборкой файлов. Reprocess пересоздаёт.
  {
    match: /local_engine_transcript_read/i,
    human: 'Не удалось прочитать транскрипт с диска.',
    hint: 'Попробуй переобработать звонок целиком (Действия → Переобработать).',
  },
  // [P2.2] Local STT crash — sherpa-onnx Whisper sidecar упал на одной из
  // дорожек (mic | system). Обычно отсутствуют модели либо повреждены.
  {
    match: /local_engine_stt_failed/i,
    human: 'Локальная транскрипция не справилась с дорожкой.',
    hint: 'Проверь что модели установлены в Настройках → Локальный движок.',
  },
  // [P2.2] Recap JSON persisted в DB упало — disk full либо integrity
  // violation. Содержимое recap сгенерировано, но не сохранено.
  {
    match: /local_engine_recap_persist/i,
    human: 'Саммари сгенерировано, но не сохранилось.',
    hint: 'Попробуй пересоздать саммари ещё раз.',
  },
  {
    match: /local_engine_llm_failed/i,
    human: 'Локальная модель не справилась с задачей.',
    hint: 'Попробуй preset «Light» в Настройках → Локальный движок.',
  },
  // Модель вернула саммари со всеми пустыми полями → header-only recap.
  // Бэкенд теперь не сохраняет такое молча, а возвращает ошибку.
  {
    match: /recap_blank_llm_output/i,
    human: 'Модель вернула пустое саммари — не удалось извлечь содержание из транскрипта.',
    hint: 'Попробуй пересоздать саммари ещё раз.',
  },
  // Паника в фоновой задаче regen (sidecar/LLM). Не должна происходить, но
  // если попала в UI — задача корректно завершилась, spinner снят.
  {
    match: /regen_panic/i,
    human: 'Не удалось пересоздать саммари — внутренняя ошибка.',
    hint: 'Попробуй ещё раз. Если повторится — перезапусти Wotold.',
  },
  // [P13] Halt gate сработал — есть failed chunks, pipeline не идёт
  // дальше step 2 (Расшифровка). User должен retry failed segments
  // через accordion → P11.1 auto-resume подхватит pipeline.
  {
    match: /chunks_need_retry/i,
    human: 'Часть сегментов не распозналась.',
    hint: 'Повтори их перед продолжением — нажми ↻ Повторить на каждом неудачном фрагменте ниже.',
  },
  {
    match: /(timeout|timed out)/i,
    human: 'Запрос занял слишком долго.',
    hint: 'Попробуй ещё раз. Если повторится — проверь интернет.',
  },

  // Permissions
  {
    match: /(permission denied|not authorized|tccd|tcc)/i,
    human: 'Нет разрешения системы.',
    hint: 'Открой «Настройки macOS → Конфиденциальность и Безопасность» и дай Wotold доступ.',
  },
  {
    match: /(microphone|nsmicrophone)/i,
    human: 'Нет доступа к микрофону.',
    hint: 'Открой Настройки → Микрофон и включи Wotold, потом перезапусти приложение.',
  },
  {
    match: /(screen[\s-]?cap|screencapture|screenrecording|nsscreen)/i,
    human: 'Нет доступа к записи системного звука.',
    hint: 'Открой Настройки → Захват системного звука и включи Wotold, потом перезапусти приложение.',
  },

  // Recording
  {
    match: /recording already in progress/i,
    human: 'Запись уже идёт.',
  },
  {
    match: /not recording/i,
    human: 'Сейчас запись не идёт.',
  },
  {
    match: /sidecar.*not.*found|wotold-audio.*not/i,
    human: 'Не найден компонент записи звука.',
    hint: 'Переустанови Wotold или сообщи нам — повреждена сборка.',
  },

  // Storage / disk
  {
    match: /(disk full|no space|enospc)/i,
    human: 'Не хватает места на диске.',
    hint: 'Очисти диск или удали старые записи.',
  },
  {
    match: /(database is locked|sqlite_busy)/i,
    human: 'База данных занята — другая операция в процессе.',
    hint: 'Попробуй через секунду.',
  },
  {
    match: /integrity_check|database corrupt/i,
    human: 'База данных повреждена.',
    hint: 'Wotold создал резервную копию (app.db.corrupt-*) и запустился с чистой. Звонки могут пропасть.',
  },

  // STT/LLM specifics
  {
    match: /transcript.*shape|missing field/i,
    human: 'Сервис распознавания вернул неожиданный формат.',
    hint: 'Перезапусти обработку звонка — иногда помогает.',
  },

  // Validation
  {
    match: /required|invalid input|bad request|400/i,
    human: 'Некорректный запрос.',
  },
  {
    match: /not found|404/i,
    human: 'Не найдено.',
  },

  // Cancellation
  {
    match: /(aborted|cancel)/i,
    human: 'Операция отменена.',
  },
];

/** Возвращает человекочитаемую строку ошибки. Если совпадения нет —
 *  возвращает строку оригинальной ошибки усечённую до 160 символов. */
export function humanError(err: unknown): string {
  const raw = errorToString(err);
  const haystack = raw.toLowerCase();

  for (const p of PATTERNS) {
    const matched =
      p.match instanceof RegExp ? p.match.test(haystack) : haystack.includes(p.match.toLowerCase());
    if (matched) {
      return p.hint ? `${p.human} ${p.hint}` : p.human;
    }
  }

  // Не нашли — возвращаем оригинал, но обрезаем длинные tech-сообщения.
  return raw.length > 160 ? `${raw.slice(0, 160)}…` : raw;
}

/** Извлекает строку из unknown. Поддерживает Error, string, обычный объект. */
function errorToString(err: unknown): string {
  if (err == null) return 'Неизвестная ошибка';
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
