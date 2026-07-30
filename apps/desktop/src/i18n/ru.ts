// Russian translation strings — source-of-truth для shape всех locale файлов.
// При добавлении ключа сюда — обязательно добавить в kk.ts и en.ts (TS enforced).
//
// We define a literal shape via `as const`, then re-export it as a widened
// `TranslationStrings` so other locale files (kk/en) can satisfy the same
// shape with their own string values.

const ruInternal = {
  // ── Top nav (App.tsx rail) ──────────────────────────────────────────────
  nav: {
    calls: 'Звонки',
    contacts: 'Контакты',
    settings: 'Настройки',
    main: 'Главная навигация',
    processingTitle: 'Обработка {n} {plural}…',
    callsPluralOne: 'звонка',
    callsPluralMany: 'звонков',
  },

  // [B18.1a] Wotold v2 shell rail.
  rail: {
    record: 'Записать звонок',
    recent: 'Недавние',
    collapse: 'Свернуть',
    expand: 'Развернуть панель',
    designSystem: 'Дизайн-система',
  },

  // [B18.1c] ⌘K command palette.
  palette: {
    placeholder: 'Найти звонок или спросить ассистента…',
    commands: 'Команды',
    calls: 'Звонки',
    empty: 'Ничего не найдено',
    allCalls: 'Все звонки',
  },

  // [B18.2a] Inbox — omni-bar, facets, view switcher.
  inbox: {
    searchPlaceholder: 'Поиск или фильтр…',
    clearAll: 'Сбросить',
    facetStatus: 'Статус',
    facetRecap: 'Рекап',
    facetPeriod: 'Период',
    facetPerson: 'Участник',
    statusReady: 'Готово',
    statusProcessing: 'Обработка',
    statusError: 'Ошибка',
    recapYes: 'С рекапом',
    recapNo: 'Без рекапа',
    periodToday: 'Сегодня',
    periodWeek: 'Эта неделя',
    quickFilters: 'Быстрые фильтры',
    addFilter: 'Добавить фильтр',
    searchInTitles: 'Искать «{q}» в названиях',
    viewLabel: 'Представление',
    viewList: 'Список',
    viewCards: 'Карточки',
    viewWeek: 'Неделя',
    viewMonth: 'Месяц',
    todayBtn: 'Сегодня',
    filter: 'Фильтр',
    recordShort: 'Записать',
    colName: 'Название',
    colParticipants: 'Участники',
    colDuration: 'Длит.',
    colDate: 'Дата',
    rowActions: 'Действия',
    rowOpen: 'Открыть',
    rowReprocess: 'Переобработать',
    rowExport: 'Экспорт…',
    calPrev: 'Назад',
    calNext: 'Вперёд',
    yearPrev: 'Предыдущий год',
    yearNext: 'Следующий год',
    periodCustom: 'Произвольный период',
    periodFrom: 'С',
    periodTo: 'По',
    reprocessStarted: 'Переобработка запущена',
    exported: 'Экспортировано',
    deleted: 'Звонок удалён',
    removeFilter: 'Убрать фильтр: {label}',
    removeText: 'Убрать поиск: {q}',
    removeRange: 'Убрать период',
  },

  // ── Common buttons / labels / states ────────────────────────────────────
  speakerLabel: {
    voice: 'Голос',
    me: 'Я',
    unknown: 'Спикер ?',
    speakerN: 'Спикер {n}',
    voiceN: 'Голос {n}',
  },
  callTitle: {
    byDate: 'Звонок · {date}',
    byId: 'Звонок {id}',
  },
  // [TD-25] Тексты ошибок — были захардкожены в api/errors.ts мимо словаря.
  errors: {
    unknown: 'Неизвестная ошибка',
    network: {
      human: 'Нет соединения с сервером Wotold.',
      hint: 'Проверь интернет и попробуй ещё раз.',
    },
    llmTimeout: {
      human: 'Локальная модель не успела ответить за 10 минут.',
      hint: 'Попробуй preset «Light» — он быстрее. Настройки → Локальный движок.',
    },
    notReady: {
      human: 'Не хватает софта для обработки.',
      hint: 'Нажми «Скачать» — звонок обработается сам, как только модули встанут.',
    },
    modelTampered: {
      human: 'Файл модуля не прошёл проверку целостности.',
      hint: 'Нажми «Скачать» — повреждённый файл заменится.',
    },
    modelMissing: {
      human: 'Локальная модель не установлена.',
      hint: 'Скачай её в Настройках → Обработка.',
    },
    presetNotSet: {
      human: 'Не выбран preset локального движка.',
      hint: 'Выбери Light / Balanced / Quality в Настройках → Локальный движок.',
    },
    transcriptEmpty: {
      human: 'Транскрипт пустой — нечего саммаризировать.',
    },
    noAppHandle: {
      human: 'Внутренняя ошибка приложения.',
      hint: 'Перезапусти Wotold и попробуй снова.',
    },
    transcriptRead: {
      human: 'Не удалось прочитать транскрипт с диска.',
      hint: 'Попробуй переобработать звонок целиком (Действия → Переобработать).',
    },
    sttFailed: {
      human: 'Локальная транскрипция не справилась с дорожкой.',
      hint: 'Проверь что модели установлены в Настройках → Локальный движок.',
    },
    recapPersist: {
      human: 'Саммари сгенерировано, но не сохранилось.',
      hint: 'Попробуй пересоздать саммари ещё раз.',
    },
    llmFailed: {
      human: 'Локальная модель не справилась с задачей.',
      hint: 'Попробуй preset «Light» в Настройках → Локальный движок.',
    },
    recapBlank: {
      human: 'Модель вернула пустое саммари — не удалось извлечь содержание из транскрипта.',
      hint: 'Попробуй пересоздать саммари ещё раз.',
    },
    regenPanic: {
      human: 'Не удалось пересоздать саммари — внутренняя ошибка.',
      hint: 'Попробуй ещё раз. Если повторится — перезапусти Wotold.',
    },
    chunksRetry: {
      human: 'Часть сегментов не распозналась.',
      hint: 'Повтори их перед продолжением — нажми ↻ Повторить на каждом неудачном фрагменте ниже.',
    },
    timeout: {
      human: 'Запрос занял слишком долго.',
      hint: 'Попробуй ещё раз. Если повторится — проверь интернет.',
    },
    permission: {
      human: 'Нет разрешения системы.',
      hint: 'Открой «Настройки macOS → Конфиденциальность и Безопасность» и дай Wotold доступ.',
    },
    micPermission: {
      human: 'Нет доступа к микрофону.',
      hint: 'Открой Настройки → Микрофон и включи Wotold, потом перезапусти приложение.',
    },
    screenPermission: {
      human: 'Нет доступа к записи системного звука.',
      hint: 'Открой Настройки → Захват системного звука и включи Wotold, потом перезапусти приложение.',
    },
    alreadyRecording: {
      human: 'Запись уже идёт.',
    },
    notRecording: {
      human: 'Сейчас запись не идёт.',
    },
    sidecarMissing: {
      human: 'Не найден компонент записи звука.',
      hint: 'Переустанови Wotold или сообщи нам — повреждена сборка.',
    },
    diskFull: {
      human: 'Не хватает места на диске.',
      hint: 'Очисти диск или удали старые записи.',
    },
    dbLocked: {
      human: 'База данных занята — другая операция в процессе.',
      hint: 'Попробуй через секунду.',
    },
    dbCorrupt: {
      human: 'База данных повреждена.',
      hint: 'Wotold создал резервную копию (app.db.corrupt-*) и запустился с чистой. Звонки могут пропасть.',
    },
    badShape: {
      human: 'Сервис распознавания вернул неожиданный формат.',
      hint: 'Перезапусти обработку звонка — иногда помогает.',
    },
    badRequest: {
      human: 'Некорректный запрос.',
    },
    notFound: {
      human: 'Не найдено.',
    },
    cancelled: {
      human: 'Операция отменена.',
    },
  },

  common: {
    winClose: 'Закрыть окно',
    winMinimize: 'Свернуть окно',
    winMaximize: 'На весь экран',
    dismiss: 'Закрыть',
    dismissToast: 'Закрыть уведомление: {message}',
    cancel: 'Отмена',
    delete: 'Удалить',
    deleting: 'Удаляем…',
    edit: 'Редактировать',
    loading: 'Загрузка…',
    loadingShort: '…',
    next: 'Дальше →',
    back: '← Назад',
    backAll: 'Все звонки',
    skip: 'Пропустить',
    gotIt: 'Понятно ✓',
    later: 'Позже',
    add: 'Добавить',
    select: 'Выбрать',
    selectNone: '— не выбран —',
    selectSearch: 'Поиск…',
  },

  // ── HomePage ────────────────────────────────────────────────────────────
  home: {
    consentEyebrow: 'Согласие на запись',
    consentTitle: 'Перед стартом',
    consentBody:
      'Wotold будет записывать звук с микрофона и системный аудиовыход. Перед началом убедись, что собеседник предупреждён и согласен на запись. По закону РФ/РК запись переговоров без уведомления другой стороны может быть нарушением.',
    consentSubnote: 'Это окно появляется один раз. В дальнейшем будем доверять твоему решению.',
    consentAccept: 'Согласен, начать',
    updateAvailable: 'Доступна версия {version} (сейчас {current}).',
    updateInstall: 'Обновить сейчас',
    updateInstalling: 'Устанавливаем…',
  },

  // ── CallsPage ───────────────────────────────────────────────────────────
  calls: {
    filteredOf: '{filtered} из {total} {plural}',
    countOf: '{n} {plural}',
    emptyTitle: 'Звонков пока нет',
    emptyBody: 'Начни запись на «Главной» — звонок появится здесь сразу после остановки.',
    notFoundTitle: 'Ничего не нашлось',
    notFoundBody: 'Сбрось фильтры или измени запрос.',
    // [Processing status] фон-regen звонка (status остаётся ready).
    secondaryBusy: 'обрабатывается',
    fallbackCallTitle: 'Звонок {short}',
    callsForm1: 'звонок',
    callsForm2: 'звонка',
    callsForm5: 'звонков',
  },

  // ── CallDetailPage / tabs / panels ─────────────────────────────────────
  callDetail: {
    // [TD-37] Оговорки о качестве обработки. Пайплайн деградирует, а не
    // падает — пользователь должен видеть, чем именно результат неполон.
    // Плеер: merged-WAV собирается только в конце обработки.
    audioPendingTitle: 'Аудио будет доступно после обработки',
    audioPendingChunks: 'Готово частей: {done} из {total}',
    degradedTitle: 'Обработано с оговорками',
    degraded: {
      partial_transcript: 'Расшифрованы не все части записи',
      system_track_not_diarized: 'Голоса собеседников не разделены',
      mic_track_not_diarized: 'Голоса на вашей дорожке не разделены',
      speaker_clustering_failed: 'Спикеры не сгруппированы по голосам',
      language_repin_failed: 'Часть текста распознана не тем языком',
      mic_track_gap_padded: 'Микрофон пропадал — пробел заполнен тишиной',
      system_track_gap_padded: 'Системный звук пропадал — пробел заполнен тишиной',
    },
    notFound: 'Звонок не найден.',
    tabRecap: 'Саммари',
    tabTranscript: 'Расшифровка',
    // [B18.3a] Right rail (CallRail).
    railProperties: 'Свойства',
    railStatus: 'Статус',
    railDate: 'Дата',
    railDuration: 'Длительность',
    railParticipants: 'Участники',
    railUndefined: '{n} не определено',
    railNoSpeakers: 'Участники появятся после обработки.',
    railSpeakerUnknown: 'Говорящий',
    railVoicesCount: 'Голосов в записи: {n}',
    railVoicesMenu: 'Голоса участника',
    railIdentify: 'Определить',
    railActions: 'Действия',
    railExport: 'Экспортировать рекап',
    exportBusy: 'Экспортируем…',
    actionsAria: 'Действия со звонком',
    reprocess: '↻ Переобработать целиком',
    reprocessing: 'Переобработка…',
    regenerateRecap: '↻ Пересоздать саммари',
    regenerating: 'Пересоздаём…',
    // [P1.3] Elapsed timer для local LLM regen — backend шлёт каждые 15s.
    regeneratingWithElapsed: 'Пересоздаём… {sec}s',
    // [Processing status] strip над табами при фон-regen.
    bgBusyStrip: 'Пересоздаём саммари…',
    bgBusyStripElapsed: 'Пересоздаём саммари… {sec}s',
    bgBusyCancel: 'Остановить',
    generatingRecap: 'Генерируется саммари…',
    generatingTranscript: 'Распознаётся речь…',
    // [F3] Thinking-блок живых шагов генерации рекапа (RecapThinking).
    think: {
      title: 'Ход генерации',
      classify: 'Определяем тип звонка',
      refine: 'Обрабатываем часть {no} из {total}',
      generate: 'Генерируем саммари',
      postPass: 'Уточняем задачи',
      narrative: 'Пишем нарратив',
      finalize: 'Проверяем и сохраняем',
      inProgress: 'Выполняется',
      stepFailed: 'Пропущено',
    },
    // [M14 T-17] Title-only regen — отдельный lightweight LLM-call.
    regenerateTitle: '↻ Пересоздать название',
    regeneratingTitle: 'Пересоздаём название…',
    regenerateTitleFailed: 'Не удалось пересоздать название: {error}',
    exportMd: '↓ Скачать .md',
    exporting: 'Сохраняем…',
    exportTitle: 'Сохранить расшифровку звонка',
    reprocessConfirmTitle: 'Wotold',
    reprocessConfirmBody:
      'Перезапустить обработку звонка?\n\nЗапись будет заново распознана и пересоздана саммари. Текущая расшифровка и рекап перезапишутся.',
    reprocessConfirmOk: 'Перезапустить',
    deleteConfirmBody:
      'Удалить звонок «{title}»?\n\nЭто навсегда удалит запись, расшифровку, саммари, задачи и образцы голоса этого звонка.',
    deleteConfirmOk: 'Удалить',
    failBadge: '⚠ Не удалось распознать речь',
    recapFailBadge: '⚠ Не удалось создать саммари',
    // [Bug-fix] Engine label для recap-fail баннера — показывает какой движок
    // обслуживал последнюю попытку. Помогает понять stale vs свежее падение.
    retry: 'Попробовать ещё раз',
    retrying: 'Перезапускаем…',
    emptyRecap: 'Саммари ещё не сгенерировано.',
    // [P14.2] Explicit empty-state с CTA вместо silent placeholder.
    recapEmptyTitle: 'Саммари не создано',
    recapEmptyAction: 'Создать саммари',
    recapEmptyIdle:
      'Нажми кнопку чтобы Wotold проанализировал расшифровку и сделал саммари.',
    recapEmptyProcessing:
      'Pipeline ещё обрабатывает звонок — саммари появится после завершения. Подожди или обнови страницу через минуту.',
    recapEmptyFailed:
      'Прошлая попытка упала: {error}\n\nМожно попробовать ещё раз — обычно временные ошибки проходят.',
    recapEmptyNoTranscript:
      'Сначала нужна расшифровка — без неё нечего саммаризировать. Попробуй переобработать звонок.',
    emptyTranscript: 'Транскрипт ещё не готов.',
    emptyTasks:
      'Здесь будут задачи, упомянутые в звонке. Пока Wotold их не нашёл — попробуй переобработать звонок или дождись пересборки.',
    reprocessFailed: 'Не удалось перезапустить: {error}',
    regenerateFailed: 'Не удалось пересоздать саммари: {error}',
    // [V6.4] Reassurance строчка под PipelineStrip: юзер видит длинный
    // процесс и нервничает. Подтверждаем что прогресс persist-нут в DB.
    reassureCanClose: 'Можно закрыть окно — мы сохраним прогресс и закончим в фоне.',
    sttElapsed: 'Распознаём речь — {sec} с',
    // [V8] Reprocess banner — отдельный от первичной обработки. Виден когда
    // звонок уже прошёл pipeline (есть recap/transcript), и юзер запустил
    // переобработку. Старый контент остаётся под баннером, можно отменить.
    reprocessRunning: 'Идёт переобработка — старый контент остаётся, можно отменить.',
    reprocessCancel: 'Отменить переобработку',
    // [V7] auto-bound banner.
    autoBoundOne: 'Авто-привязали собеседника: {name} — по совпадению голоса.',
    autoBoundMany: 'Авто-привязали {n} собеседник(ов): {names} — по совпадению голоса.',
    autoBoundUndo: '↩ Отменить',
    // [M14 T-15] Legacy v1 → v2 upgrade banner.
    legacyRecapTitle: 'Старый формат саммари',
    legacyRecapHint:
      'Обновите чтобы получить тип звонка, решения, открытые вопросы и цитаты из расшифровки.',
    legacyRecapButton: 'Обновить до v2',
    legacyRecapUpgrading: 'Обновляем…',
    // [Bug-fix #6] Suggest recap regen after speaker→contact bind.
    recapRegenSuggestionTitle: 'Имена участников изменились',
    recapRegenSuggestionHint:
      'Пересоздать саммари чтобы оно использовало актуальные имена?',
    recapRegenSuggestionButton: 'Пересоздать саммари',
    recapRegenSuggestionBusy: 'Пересоздаём…',
    recapRegenSuggestionDismiss: 'Скрыть',
    // [V6.5] Error variant — заголовок и подзаголовок ErrorScreen.
    errorTitle: 'Что-то пошло не так',
    errorAudioSaved: 'Аудио сохранено локально, его можно прослушать ниже.',
    errorRetry: 'Попробовать ещё раз',
    errorRetryProvider: 'Попробовать через {provider}',
    errorDiagnosticsTitle: 'Диагностика',
    errorDiagnosticsCode: 'Код',
    errorDiagnosticsProvider: 'Провайдер',
    errorDiagnosticsLastAt: 'Последняя попытка',
    errorDiagnosticsQuota: 'Списано из квоты',
  },

  // ── Recap tab (dossier) ─────────────────────────────────────────────────
  recap: {
    modeRich: 'Оформленный',
    modeMd: 'Markdown',
    copyMd: 'Копировать .md',
    copied: 'Скопировано',
    summary: 'Резюме',
    summaryAlt: 'Саммари',
    keyPoints: 'Ключевые моменты',
    tasksCount: 'Задачи · {n}',
    regenerate: '↻ Пересоздать',
    regenerating: 'Пересоздаём…',
    emptyTasks: 'Wotold не нашёл задач в этом звонке.',
    emptyRecap: 'Саммари ещё не сгенерировано.',
    metadata: 'Метаданные',
    metaDate: 'Дата',
    metaProvider: 'Провайдер',
    metaLang: 'Язык',
    metaDuration: 'Длительность',
    metaId: 'ID',
    participants: 'Участники',
    exportMd: 'Экспорт в MD',
    taskDue: 'до {date}',
  },

  // ── Speakers section + SpeakerCard ─────────────────────────────────────
  speakers: {
    emptyTitle: 'Участники не распознаны',
    emptyBody: 'В этом звонке не обнаружено отдельных голосов, либо обработка ещё идёт.',
    confirmedTitle: 'Подтверждены · {n}',
    mergedVoices: '({n} голосов объединены)',
    voiceMergedNote: '{tags} · распознавание разделило на {n}',
    unbindOne: 'Отвязать',
    unbindLabeled: 'Отвязать {label}',
    unbindAria: 'Отвязать {label}',
    cardEyebrow: 'Голос {idx} из {total}',
    cardTitle: 'Кто этот голос?',
    cardSampleFallback: 'голос распознан · послушать сэмпл',
    samplePlay: '▶ {sec} сек',
    samplePlayFallback: '▶ сэмпл',
    sampleStop: '◼ стоп',
    samplePlayAria: 'Послушать сэмпл',
    sampleStopAria: 'Остановить сэмпл',
    sampleScrubAria: 'Перемотка сэмпла',
    sampleUnavailable: 'Аудиосэмпл недоступен',
    suggestion: 'Похоже на',
    confidence: 'Уверенность',
    pickContact: 'Выбрать контакт',
    pickPlaceholder: 'Имя, организация, роль…',
    pickerNone: '— не выбран —',
    confirmYes: '✓ Да, это {name}',
    confirmPickBelow: '✓ Выбери контакт ниже',
    confirmAddNew: '✓ Добавь новый контакт',
    notHimHer: 'Не он/она',
    newContact: 'Новый контакт',
    newContactName: 'Имя нового контакта',
    rememberVoice: 'Запоминать голос для авто-определения',
    addingAndBinding: 'Добавляем…',
    addAndBind: 'Добавить и привязать',
    finePrint: 'Подтверждение сохранит голос в профиль контакта (если включена опция) ',
    needContactFirst: 'Сначала выбери контакт из списка.',
    needContactSelect: 'Сначала выбери контакт.',
    needContactName: 'Введи имя контакта.',
    confirmModalAria: 'Подтверждение голоса',
    sourceVoiceLlm: 'голос + LLM',
    sourceVoice: 'голос',
    sourceLlm: 'LLM',
    suggestionRoleNone: '—',
  },

  // ── Participants row (CallDetail header) ───────────────────────────────
  participants: {
    one: 'участник',
    few: 'участника',
    many: 'участников',
    // [Bug-fix] Anonymous chip — sortformer выделил голос, контакт не привязан.
    // [P14.3] Hint когда спикеров много — overflow noise обычно от
    // sortformer'а на перекрытиях. User может уточнить через Labs.
    tooManyHint:
      'Если реально меньше — в Настройки → Labs выбери «Принудительное количество спикеров».',
  },

  // ── Audio scrubber ─────────────────────────────────────────────────────
  scrubber: {
    play: 'Воспроизведение',
    pause: 'Пауза',
    progressAria: 'Аудио прогресс',
    jumpToCurrent: 'К текущему участку',
    trackGroup: 'Дорожка',
    trackSystemTitle: 'Звук собеседника (системный аудио)',
    trackSystemLabel: 'Собеседник',
    trackMicTitle: 'Звук с твоего микрофона',
    trackMicLabel: 'Я',
    loadingAudio: 'Загружаем…',
    audioLoadFailed: 'Не удалось загрузить аудио: {error}',
  },

  // ── ContactsPage ────────────────────────────────────────────────────────
  contacts: {
    collapsePanel: 'Свернуть список контактов',
    expandPanel: 'Развернуть список контактов',
    title: 'Контакты',
    addAria: 'Добавить контакт',
    searchPlaceholder: 'Поиск…',
    emptyTitle: 'Контактов нет',
    emptyAddCue:
      'Добавь первый — кнопка «+» слева сверху. Контакты помогают Wotold подписывать спикеров в расшифровках.',
    notFoundTitle: 'Ничего не нашлось',
    newContact: 'Новый контакт',
    editTitle: 'Редактировать контакт',
    kind: {
      phone: 'Телефон',
      email: 'Email',
      telegram: 'Telegram',
      whatsapp: 'WhatsApp',
      signal: 'Signal',
      slack: 'Slack',
      other: 'Другое',
    },
    statCalls: 'Звонков',
    statRecorded: 'Записано',
    statVoiceSamples: 'Голосовые семплы',
    recentCalls: 'Недавние звонки',
    voiceConfirmed: 'Голос подтверждён',
    notes: 'Заметки',
    owner: 'владелец',
    deleteConfirmBody: 'Удалить контакт «{name}»?',
    roleNone: '—',
    submitCreate: 'Создать',
    submitSave: 'Сохранить',
    fieldName: 'Имя',
    fieldRole: 'Должность / роль',
    fieldOrg: 'Организация',
    fieldNotes: 'Заметки',
    rememberVoiceTitle: 'Запоминать голос для авто-определения',
    rememberVoiceHint:
      'При подтверждении этого человека в звонке Wotold сохранит короткий образец голоса — чтобы в будущем определять его автоматически. Сними галку, чтобы отключить.',
    identifiers: 'Идентификаторы',
    identifiersHint: 'Телефоны, мейлы, мессенджеры.',
    customFields: 'Расширяемые поля',
    customFieldsHint: 'Любые ключ/значение — birthday, linkedin, любые.',
    addRow: 'Добавить',
    removeRowAria: 'Удалить строку',
    removeRowTitle: 'Удалить',
    identifierValue: 'значение',
    identifierKey: 'ключ',
    durationZero: '0',
    durationM: '{m}м',
    durationH: 'ч',
    durationHM: 'ч {m}м',
  },

  // ── Voice samples section ──────────────────────────────────────────────
  voiceSamples: {
    emptyTitle: 'Образцов голоса пока нет',
    emptyBody:
      'Подтверди этого человека в любом звонке — Wotold начнёт сохранять короткие образцы голоса для авто-определения в будущем. Требует включённой опции «Запоминать голос».',
    deleteAria: 'Удалить семпл',
    deleteConfirmBody:
      'Удалить voice sample от {created}?\n\nЭто навсегда удалит embedding из профиля контакта. Биометрия не восстанавливается.',
    // [P4] Inline play — slice WAV bytes из правильной track (start..end).
    playAria: 'Прослушать семпл',
    pauseAria: 'Пауза',
    playDisabledHint: 'Старый семпл — прослушать недоступно (нет slice metadata)',
  },

  // ── Settings — sections + interior content ─────────────────────────────
  settings: {
    collapsePanel: 'Свернуть разделы',
    expandPanel: 'Развернуть разделы',
    title: 'Настройки',
    saved: '✓ Сохранено',
    sectionAppearance: 'Внешний вид',
    sectionPermissions: 'Разрешения',
    sectionProcessing: 'Обработка',
    sectionRecording: 'Запись',
    sectionSpeakers: 'Спикеры',
    sectionLabs: 'Лаборатория',
    sectionPrivacy: 'Приватность',
    speakersAutoBindLabel: 'Автоматически привязывать спикеров к контактам',
    speakersAutoBindHint:
      'Только при высокой уверенности — можно отменить прямо в звонке.',
    fieldTheme: 'Тема',
    fieldLanguage: 'Язык интерфейса',
    themeLight: 'Светлая',
    themeDark: 'Тёмная',
    themeSystem: 'Системная',
    sttLangLabel: 'Распознавание речи',
    sttLangHint: 'На тихом микрофоне для русских звонков надёжнее выбрать «Русский».',
    sttRecapLangLabel: 'Рекап и задачи',
    sttRecapLangHint: 'Авто — язык распознанной речи.',
    // [V7] Auto-bind speaker по биометрии. R2 паспорта — opt-in only, default OFF.
    autoBindThresholdLabel: 'Порог совпадения',
    autoBindThresholdOption: '{n}%',
    autoBindThresholdHint:
      'Чем выше порог, тем меньше ошибок, но и реже срабатывает. 95% — баланс по умолчанию.',
    // [W1] Configurable hotkeys для recording.
    hotkeyToggleLabel: 'Старт / стоп',
    hotkeyChange: 'Изменить',
    hotkeyCancel: 'Отмена (Esc)',
    hotkeyNeedModifier: 'Нужен модификатор (⌘/⌃/⌥) или F-клавиша',
    hotkeyReserved: '{combo} зарезервирована системой — выберите другую',
    hotkeyCurrentAria: 'Текущая горячая клавиша',
    hotkeyCapturingAria: 'Записываем комбинацию…',
    groupLanguages: 'Языки',
    groupHotkeys: 'Горячие клавиши',
    groupAutoDetect: 'Авто-определение',
    callDetectRowLabel: 'Предлагать запись',
    callDetectCooldownRowLabel: 'Не предлагать снова',
    wipeRowHint: 'Записи, контакты, образцы голоса, сессия и ключи. Необратимо.',
    wipeDoneChip: 'удалено',
    hotkeyToggleHint: 'Esc — отмена. Системные комбинации (⌘W, ⌘C…) недоступны.',
    hotkeyPauseLabel: 'Пауза / продолжить',
    hotkeyPauseHint:
      'Срабатывает только во время активной записи. По умолчанию ⌘⇧P.',
    // [S1] Auto-detect (R3 deviation, opt-in).
    callDetectHint: 'Уведомление «Записать?» при обнаружении звонка. По умолчанию выключено для приватности.',
    callDetectCooldownOption: '{n} мин',
    voiceLede:
      'Wotold может предлагать кто говорит на основе совпадения голоса — но только после скачивания биометрической модели (25 МБ, опционально). Финальное подтверждение всегда за тобой (R2 паспорта).',
    // [M14 T-14] Labs section — experimental flags.
    summaryV2Label: 'Новый формат саммари',
    summaryV2Hint:
      'Включено по умолчанию. Выключите если возникли проблемы с типом звонка, цитатами или решениями — рекапы вернутся к простому формату.',
    forceNumSpeakersLabel: 'Число собеседников (кроме вас)',
    forceNumSpeakersHint:
      'Твой голос всегда отдельно («Я»). Это число — сколько СОБЕСЕДНИКОВ на удалённой стороне. Задай точное значение если авто-разделение промахивается, и переобработай. Лимит — 3.',
    forceNumSpeakersOptions: {
      auto: 'Авто (рекомендовано)',
      '2': '2 собеседника',
      '3': '3 собеседника',
    },
    wipeBtn: 'Удалить все данные',
    wipeBusy: 'Удаляем…',
    wipeConfirmTitle: 'Полная очистка',
    wipeConfirmBody:
      'УДАЛИТЬ ВСЕ ДАННЫЕ?\n\nЭто навсегда сотрёт:\n  • все записи звонков и аудио\n  • все контакты и voice samples\n  • сессию входа и BYO API-ключи\n\nДействие необратимо.',
    wipeConfirmOk: 'Удалить всё',
    wipeDone:
      '✓ Все данные удалены. Закрой и заново открой Wotold чтобы начать с чистой установки.',
  },

  // ── Account / OIDC ──────────────────────────────────────────────────────

  // ── Permissions ─────────────────────────────────────────────────────────
  permissions: {
    rowMic: 'Микрофон',
    rowMicDesc: 'Записывает то, что говоришь ты.',
    rowScreen: 'Захват системного звука',
    rowScreenDesc:
      'Записывает голос собеседника в FaceTime, Zoom, Telegram и других звонковых приложениях. macOS обозначает это разрешение как «Screen & System Audio Recording». После того как разрешишь — перезапусти Wotold.',
    rowAccessibility: 'Универсальный доступ',
    rowAccessibilityDesc:
      'Нужно для глобальных горячих клавиш, когда на переднем плане другое приложение. Разрешить в Системных настройках → Конфиденциальность → Универсальный доступ.',
    request: 'Запросить',
    requestAgain: 'Перезапросить',
    requestTitle: 'Показать macOS-диалог запроса',
    openSettings: 'Настройки',
    openSettingsTitle: 'Открыть System Settings → Privacy & Security',
    refreshStatusTitle: 'Перечитать текущий статус',
    refreshStatusAria: 'Обновить статус',
    granted: 'выдано',
    grantedTitle: 'Доступ разрешён',
    denied: 'отказано',
    deniedTitle: 'Пользователь отказал или ещё не давал доступ. Запроси заново или открой Настройки.',
    notDetermined: 'не запрошено',
    notDeterminedTitle: 'Ещё не запрашивали. Жми «Запросить».',
    restricted: 'заблок. системой',
    restrictedTitle: 'Системная политика (MDM / родительский контроль) запретила.',
    unknown: '?',
    unknownTitle: 'Статус неизвестен',
  },

  // ── Usage ───────────────────────────────────────────────────────────────
  update: {
    sectionAbout: 'О приложении',
    version: 'Версия',
    versionHint: 'Установленная сборка. Обновления приходят с GitHub.',
    check: 'Проверить обновления',
    checking: 'Проверяем…',
    upToDate: 'Актуальная версия',
    availableChip: 'Доступна {version}',
    checkFailed: 'Не удалось проверить обновления',
    toastAvailable: 'Вышла версия {version}',
    toastAction: 'Обновить',
    mandatoryPending: 'Версия {version} обязательная — установится, как только закончится запись и обработка.',
    mandatoryPendingHint: 'Запись и обработку обновление не прерывает.',
    changelog: 'Что изменилось',
    changelogHint: 'Заметки к выпуску на GitHub.',
  },
  usage: {
    noLimit: 'лимит не настроен',
  },

  // ── [M12-v1.1] Engine chip labels ──────────────────────────────────────

  // [M14 T-11] CallTypeBadge — 9 типов звонков из CallSummaryV2.
  callType: {
    sales_discovery: 'Discovery-звонок',
    sales_demo: 'Демо продукта',
    product_sync: 'Команда',
    standup: 'Стендап',
    customer_interview: 'Интервью клиента',
    one_on_one: '1:1',
    strategy_brainstorm: 'Брейншторм',
    status_update: 'Статус',
    other: 'Звонок',
  },

  // [M14 T-11] Privacy disclaimer для 1:1 встреч (содержит личную обратную связь).
  privacyDisclaimer: {
    oneOnOneTitle: '🔒 Это была встреча 1:1',
    oneOnOneBody:
      'Содержит личную обратную связь. Саммари показывает только темы. Не делись без необходимости.',
  },

  // [M14 T-11] Action item v2 — confidence badges + категории + evidence.
  actionItem: {
    category: {
      commitment: 'обязательство',
      proposal: 'предложение',
      idea: 'идея',
    },
  },

  // ── [M12-v1.1] Failure screen kinds ────────────────────────────────────
  failure: {
    brokenRecording: {
      eyebrow: 'Ошибка файла',
      title: 'Не удалось прочитать аудио',
      body: 'Файл записи повреждён или имеет неподдерживаемый формат. Попробуй сохранить wav и пересмотреть запись, или удали звонок.',
      saveWav: 'Сохранить .wav в Finder',
      retryCloud: 'Попробовать в облаке',
      delete: 'Удалить запись',
      techLabel: 'Техническая причина',
    },
  },

  // ── Voice model section ─────────────────────────────────────────────────
  voiceModel: {
    featureOff:
      '⚠ В этой сборке фича voice-onnx не включена. Модель можно скачать, но pipeline её не использует — биометрический матчинг останется выключенным. В production-сборке (`--features voice-onnx`) скачивание автоматически активирует матчинг.',
    modelName: 'Модуль распознавания голоса',
    statusValid: 'установлена',
    statusMissing: 'нет',
    statusCorrupted: 'повреждена',
    statusDownloading: 'качаем',
    descValid:
      'Модуль готов. Wotold будет предлагать кто говорит на основе совпадения голоса с уже подтверждёнными контактами. Финальное подтверждение — всегда за тобой.',
    descCorrupted: 'Файл повреждён или сменилась версия. Удали и скачай заново.',
    descMissing:
      'Биометрический матчинг сейчас выключен. Скачай модель чтобы Wotold предлагал кто говорит. Размер ~25 МБ, скачивается один раз в фоне.',
    btnDownload: '↓ Скачать {size}',
    btnRedownload: '↻ Перекачать',
    btnDownloading: 'Скачиваем…',
    btnDelete: 'Удалить',
    mb: '{n} МБ',
    verifyFailed:
      'SHA256 не совпал — файл повреждён или сменилась версия модели. Попробуй снова.',
  },

  // ── Local engine (M12) ──────────────────────────────────────────────────
  localEngine: {
    presetLabel: 'Сборка моделей',
    keepResidentLabel: 'Держать модель активной',
    keepResidentHint:
      'Локальная модель остаётся в оперативке всю сессию — генерация быстрее (нет перезагрузки на каждый вызов), но занимает ~2–5 ГБ RAM. Выключено по умолчанию.',
    preset: {
      light: 'Лёгкий',
      balanced: 'Сбалансированный',
      quality: 'Максимальный',
    },
    presetMeta: {
      light: 'точность ~85% · быстро',
      balanced: 'точность ~93% · средне',
      quality: 'точность ~97% · медленно',
    },
    presetRecommend: 'Рекомендуем',
    // Абстрактные имена моделей для UI (storage table, delete confirm и т.д.).
    // Конкретные бренды (Whisper/Qwen/Pyannote) — только в Rust-логах и контракте.
    modelLabel: {
      whisperSmall: 'Модуль речи · S',
      whisperMedium: 'Модуль речи · M',
      whisperLarge: 'Модуль речи · L',
      qwenSmall: 'Модуль саммари · S',
      qwenMedium: 'Модуль саммари · M',
      qwenLarge: 'Модуль саммари · L',
      diarization: 'Модуль разделения · базовый',
      qwenDraft: 'Ускоритель саммари · 0.5B',
      vad: 'Детектор речи',
      embedder: 'Модуль поиска ассистента',
      embedderTokenizer: 'Словарь модуля поиска',
      voiceEmbedder: 'Модуль распознавания голоса',
    },
    statusInstalled: 'установлено',
    statusDownloading: 'качаем…',
    statusAbsent: 'не установлено',
    storageUsed: 'Модели занимают {size}',
    freeSpaceCta: 'Освободить {size}',
    freeSpaceConfirmTitle: 'Освободить место',
    freeSpaceConfirmBody:
      'Удалить модели размеров, которые сейчас не используются, и освободить {size}? Скачать их можно будет снова. Уже обработанные звонки не изменятся.',
    freeSpaceConfirm: 'Удалить',
    freeSpaceDone: 'освободили {size}',
    qualityConfirmTitle: 'Quality на этом Mac',
    qualityConfirmMsg:
      'Quality рассчитан на 16+ ГБ оперативной памяти. На вашем Mac обработка может быть очень медленной. Всё равно использовать?',
    hwBannerTitle: 'Рекомендация по железу',
    hwBannerBody:
      'У вас {cpu} · {ram} ГБ. Лучше всего подойдёт сборка {preset}.',
    hwBannerApply: 'Применить',
    hwBannerDismiss: 'Скрыть',
    probeSummary: '{cpu} · {ram} ГБ · {metal} — рекомендуем {preset}',
    probeMetalYes: 'Metal',
    probeMetalNo: 'без Metal',
    reprobe: 'Переоценить',
    // ── [M12-v1.1] Probe skeleton ─────────────────────────────────────────
    probeSkeleton: {
      measuring: 'Оцениваем железо…',
      timeout: 'Не удалось определить железо. Выберите сборку вручную.',
    },
    // ── [M12-v1.1] Storage delete confirm modal ───────────────────────────
    // ── [M12-v1.1] Rediscovery chip ───────────────────────────────────────
    // ── M12.7.3 Onboarding engine step ───────────────────────────────────
    // (вложено сюда так как онбординг строит ключи `onboarding.engine.*`,
    //  но они логически часть local-engine модуля)
  },

  // ── Onboarding ──────────────────────────────────────────────────────────
  onboarding: {
    stepLabel: 'Шаг 0{step} из 0{total} · {label}',
    step1Label: 'Знакомство',
    step2Label: 'Владелец',
    step3Label: 'Разрешения и согласие',
    step1Headline: 'Диктофон  со смыслом.',
    step2Headline: 'Ваш голос —\nпервый.',
    step3Headline: 'Готовы? \nДва разрешения и старт.',
    step1Lede:
      'Записывает звонки и встречи на твоём Mac, расшифровывает речь и кратко пересказывает что обсуждалось. Всё хранится локально — в облако ничего не утекает.',
    step2Lede:
      'Wotold отделяет вашу речь от речи собеседника. Расскажите, кто вы — мы запомним ваш голос и больше не будем спрашивать.',
    step3Lede:
      'Wotold нужны два разрешения macOS, чтобы записывать звонки. Дай доступ — без них запись не пойдёт. Записи остаются локально на твоём диске.',
    feature1: '— Запись микрофона и системного звука раздельно',
    feature2: '— Расшифровка с распознаванием участников',
    feature3: '— Авто-саммари и список задач',
    feature4: '— Локально на устройстве, бесплатно, без сети',
    feature5: '— Поиск по разговорам прямо в Claude через MCP',
    fieldName: 'Имя',
    fieldRole: 'Роль',
    fieldGreeting: 'Краткое представление',
    namePlaceholder: 'Айдар Жунусов',
    rolePlaceholder: 'Co-founder, Wotold',
    greetingPlaceholder: 'как вы здороваетесь',
    greetingHint: 'Поможет распознать вас на старте звонка.',
    enterName: 'Введи имя.',
    consentBody:
      'Wotold будет записывать твой микрофон и звук собеседника во время звонков. Перед началом убедись, что собеседник предупреждён о записи. По закону РФ/РК запись переговоров без уведомления может быть нарушением.',
    // [M12.7.3] Engine setup step (macOS only — между Owner и Permissions+Consent).
    engineStepLabel: 'Движок',
    engineHeadline: 'Подбираем\nдвижок под Mac.',
    engineLede:
      'Wotold может работать полностью локально, без отправки звонков в облако. Подобрали оптимальный для вашего железа.',
    engine: {
      probeEyebrow: 'Ваш Mac',
      downloadCta: 'Скачать и продолжить (~{size} GB)',
      chooseAnotherCta: 'Выбрать другой пресет',
      collapsePickerCta: 'Скрыть пресеты',
      downloadingLabel: 'Качаем модули',
      continueInBackgroundCta: 'Свернуть — докачается в фоне',
      recommendedTag: 'РЕКОМ.',
      feat: {
        light: {
          stt: 'Распознавание · базовое, быстро',
          llm: 'Саммари · базовое, компактно',
        },
        balanced: {
          stt: 'Распознавание · стандартное, точно',
          llm: 'Саммари · стандартное, развёрнуто',
        },
        quality: {
          stt: 'Распознавание · максимальное, очень точно',
          llm: 'Саммари · максимальное, лучшее качество',
        },
      },
      featSpeakers: 'Распознаёт до 4 голосов',
      // [M12-v1.1] Preview state
      previewCta: 'Что входит в сборку?',
      previewEyebrow: 'Предпросмотр',
      previewTitle: 'Вот как это работает',
      previewTranscript1: '— [Вы] Добрый день, как ваши дела?',
      previewTranscript2: '— [Собеседник] Всё отлично, спасибо!',
      previewTranscript3: '— [Вы] Тогда переходим к делу.',
      previewTranscript4: '— [Собеседник] Отлично, я готов.',
      previewProcessed: 'обработано за {ms} мс · локально',
      previewInstall: 'Установить · {size} ГБ',
      previewBack: '← Назад',
    },
    saving: 'Сохраняем…',
    finishBtn: 'Готово',
    stepAria: 'Шаг {step} из {total}',
  },

  // ── Coachmarks ──────────────────────────────────────────────────────────
  coachmarks: {
    step01: 'Шаг 01',
    step02: 'Шаг 02',
    step03: 'Шаг 03',
    step04: 'Шаг 04',
    stepOf: '{step} из 0{total}',
    homeTitle: 'Главная',
    homeBody:
      'Жмёшь красный кружок когда созваниваешься. Hotkey ⌘⇧R, если кнопка не на виду. После остановки звонок попадает во вкладку «Звонки».',
    callsTitle: 'Звонки',
    callsBody:
      'Все записи группируются по датам. Внутри каждого звонка — четыре вкладки: Саммари, Расшифровка, Задачи, Участники.',
    contactsTitle: 'Контакты',
    contactsBody:
      'Добавляешь людей сюда, потом в звонках подтверждаешь «этот спикер = Иван». Wotold запоминает голос и подсказывает в следующий раз. Биометрия — только с opt-in.',
    settingsTitle: 'Настройки',
    settingsBody:
      'Переключаешь STT/LLM-провайдеров, привязываешь свои ключи, видишь квоты тарифа и можешь стереть все данные одной кнопкой. Там же — тема и акцент.',
    progressAria: 'Прогресс',
  },

  // ── Live recording / waveforms (HomePage recording state) ───────────────
  recording: {
    // [W3] RecStrip labels — persistent strip rendered above main content
    // while a recording is active (or paused).
    stripRecording: 'Идёт запись',
    tooShort: 'Запись короче {sec} с — не сохранена',
    stripPaused: 'Пауза · записано',
    announceRecording: 'Идёт запись',
    announcePaused: 'Запись на паузе',
    announceIdle: 'Запись не идёт',
    pauseAction: 'Поставить на паузу',
    resumeAction: 'Продолжить запись',
    stopAction: 'Остановить запись',
    // [S5] SuggestBanner — поднимается когда S2-probe видит начавшийся звонок.
    suggestTitle: 'Похоже на звонок в {app}',
    suggestBody:
      'Wotold заметил активный микрофон. Начать запись прямо сейчас?',
    suggestStart: 'Начать запись',
    suggestDismiss: 'Скрыть',
  },

  // ── [V6.1] Async pipeline states (CallStateTag, PipelineStrip etc) ──────
  // [Q] Монитор очередей тяжёлых ресурсов (QueueMonitor + queued-строка).
  readiness: {
    eyebrow: 'Локальная обработка',
    missing: 'Не хватает части софта для обработки звонков — {size}.',
    download: 'Скачать',
    downloading: 'Скачиваем модули… {pct}%',
    downloadingAria: 'Прогресс скачивания модулей',
    retry: 'Повторить',
    verifyFailed: 'Файл модуля не прошёл проверку целостности — попробуйте ещё раз.',
    choosePreset: 'Сначала выберите размер движка — от него зависит, что качать.',
    openSettings: 'Открыть настройки',
    callParked: 'Звонок ждёт: не хватает софта. Обработается сам, как только модули встанут.',
    callParkedDownload: 'Скачать',
    queueWaiting: 'Очередь ждёт скачивания модулей',
  },
  queue: {
    monitor: 'Очереди обработки',
    res: {
      stt: 'Распознавание речи',
      diarization: 'Разделение голосов',
      llm: 'Генерация саммари',
    },
    free: 'СВОБОДЕН',
    busy: 'В РАБОТЕ',
    systemTask: 'Служебная задача',
    empty: 'Очередь пуста',
    callWaiting: 'В очереди: {resource} — сейчас обрабатывается другой звонок (позиция {pos})',
  },
  callState: {
    // Badge labels
    live: 'идёт запись',
    uploading: 'загружаем',
    queued: 'в очереди',
    processing: 'распознаём',
    ready: 'готов',
    error: 'ошибка',
    // [Processing status] generic busy (regen — реального шага нет).
    busyGeneric: 'обрабатываем',
    // CallErrorRow / PipelineStrip копи
    audioSaved: 'аудио сохранено',
    moreDetails: 'подробнее →',
    errorFallback: 'не удалось распознать',
    etaSec: 'сек',
    details: 'подробнее',
    pending: 'ожидает',
  },
  // Step labels синхронизированы с backend `Stage::step()`
  // ([apps/desktop/src-tauri/src/pipeline/stage.rs]):
  //   1 = Upload · 2 = Transcribe · 3 = RecognizeSpeakers
  //   4 = MergeArtifacts · 5 = Recap
  // Все в present continuous — соответствует active-state UX (step рендерится
  // во время выполнения; done-state видно через ✓ checkmark + muted color).
  pipeline: {
    step1: 'Сохраняем аудио',
    step2: 'Распознаём речь',
    step3: 'Соотносим голоса с контактами',
    step4: 'Сводим транскрипт',
    step5: 'Готовим саммари и задачи',
  },
  // [M13.3.3 / P11.2] Chunked pipeline (длинные звонки нарезаются на 10-мин
  // сегменты). UX модель: chunks = implementation detail STT-параллелизации,
  // часть step 2 «Распознаём речь». Видны только при failed (accordion).
  chunkProgress: {
    label: 'Сегменты',
    ofN: '{done} из {total}',
    // [P11.2] Inline badge на step 2 PipelineStrip — показывает прогресс
    // chunked-STT во время transcription stage.
    inlineBadge: '{done} из {total} сегментов',
    // [P11.2] Accordion title — collapsed по умолчанию, появляется только
    // когда есть failed chunks. User раскрывает чтобы retry.
    accordionTitle: 'Не удалось распознать сегменты',
    accordionHint: 'Раскрой чтобы повторить распознавание неудачных фрагментов',
    statusDone: 'готово',
    statusFailed: 'не удалось',
    statusProcessing: 'обрабатываем',
    statusPending: 'ожидает',
    // [Tech-debt P0.2] Retry failed chunk.
    retry: '↻ Повторить',
    retrying: 'Повторяем…',
    failedSummary: '{n} из {total} сегментов не удалось — нажми ↻ чтобы переcпавнить.',
    // [P11.3] Resume-blocked tooltip — disabled-state причина на reprocess
    // кнопке когда есть failed chunks.
    resumeBlockedHint: 'Сначала повтори неудачные сегменты',
  },
  // ── [B24] Ассистент (тексты SPEC хендоффа дословно, деловой регистр) ────
  assistant: {
    title: 'Ассистент',
    msgYesterday: 'вчера',
    searchChats: 'Поиск по чатам…',
    chatsPanel: 'Чаты',
    searchEmpty: 'Чатов по запросу не найдено',
    searchResults: 'Найдено',
    collapsePanel: 'Свернуть список чатов',
    expandPanel: 'Развернуть список чатов',
    fragExpand: 'показать целиком',
    fragCollapse: 'свернуть',
    fragLoading: 'загрузка…',
    fragLoadError: 'Не удалось загрузить фрагмент',
    fragRefLabel: 'Фрагмент {n}',
    emptyTitle: 'Поиск по всем звонкам',
    emptyDesc:
      'Вопрос — это поиск по расшифровкам и рекапам, ответ — с указанием источников. Каждый диалог — новый чат.',
    pendingGlobal: 'Поиск по {n} звонкам…',
    pendingCall: 'Поиск…',
    refusalNote: 'Вне области ассистента',
    ctxSummary: 'Контекст поиска',
    ctxMeta: 'фрагментов: {n} · ≈{tokens}K токенов · окно 8K',
    copy: 'Скопировать',
    copied: 'Скопировано',
    share: 'Поделиться',
    escalate: 'Искать во всех звонках',
    newChat: 'Новый чат',
    noChats: 'Чатов пока нет',
    deleteChat: 'Удалить чат',
    composerGlobal: 'Спросить по всем звонкам…',
    composerCall: 'Спросить об этом звонке…',
    callEmptyDesc:
      'Чат этого звонка. Ответы строятся по его расшифровке; если факт найден в другом звонке — источник будет указан.',
    sendLabel: 'Отправить',
    dayToday: 'Сегодня',
    dayYesterday: 'Вчера',
    findOrAsk: 'Найти или спросить',
    paletteCommand: 'Ассистент — поиск по звонкам',
    paletteNotFound: 'Ничего не найдено · Ассистент',
    paletteFallbackLabel: 'Спросить ассистента',
    paletteFallbackHint: '«{q}» — поиск по {n} звонкам',
  },
} as const;

// Widen recursively from literal types to `string` so other locales can
// satisfy the same shape with their own string values.
type WidenStrings<T> = {
  [K in keyof T]: T[K] extends string ? string : WidenStrings<T[K]>;
};

export type TranslationStrings = WidenStrings<typeof ruInternal>;

export const ru: TranslationStrings = ruInternal;

export default ru;
