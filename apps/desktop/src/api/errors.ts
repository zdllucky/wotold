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
  // Network / proxy
  {
    match: /(econnrefused|networkerror|failed to fetch|networkerror when attempting|err_internet)/i,
    human: 'Нет соединения с сервером Wotold.',
    hint: 'Проверь интернет и попробуй ещё раз.',
  },
  {
    match: /(timeout|timed out)/i,
    human: 'Запрос занял слишком долго.',
    hint: 'Попробуй ещё раз. Если повторится — проверь интернет.',
  },
  {
    match: /quota[_-]?exceeded|too many requests|429/i,
    human: 'Превышен дневной лимит на бесплатном тарифе.',
    hint: 'Подожди до завтра или переключись на свои API-ключи в Настройках.',
  },
  {
    match: /(unauthorized|401)/i,
    human: 'Сессия истекла или ключ невалидный.',
    hint: 'Войди заново или проверь API-ключи в Настройках.',
  },
  {
    match: /(forbidden|403)/i,
    human: 'Доступ запрещён.',
  },
  {
    match: /proxy\s*5\d\d/i,
    human: 'Сервер Wotold временно недоступен.',
    hint: 'Попробуй через 10–15 секунд — обычно проходит на втором заходе.',
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
    match: /presign failed/i,
    human: 'Не получилось подготовить загрузку аудио.',
    hint: 'Это временная ошибка прокси — попробуй ещё раз.',
  },
  {
    match: /staging_object_not_found/i,
    human: 'Файл звука пропал с сервера.',
    hint: 'Перезапусти обработку звонка.',
  },
  {
    match: /transcript.*shape|missing field/i,
    human: 'Сервис распознавания вернул неожиданный формат.',
    hint: 'Перезапусти обработку звонка — иногда помогает.',
  },
  {
    match: /llm.*not.*configured|no llm provider/i,
    human: 'LLM не настроен.',
    hint: 'Подожди пока админ зальёт ключи, либо подключи свой ключ Anthropic в Настройках.',
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
