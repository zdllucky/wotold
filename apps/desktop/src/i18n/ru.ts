// Russian translation strings — source-of-truth для shape всех locale файлов.
// При добавлении ключа сюда — обязательно добавить в kk.ts и en.ts (TS enforced).
//
// We define a literal shape via `as const`, then re-export it as a widened
// `TranslationStrings` so other locale files (kk/en) can satisfy the same
// shape with their own string values.

const ruInternal = {
  // ── Top nav (App.tsx rail) ──────────────────────────────────────────────
  nav: {
    home: 'Главная',
    calls: 'Звонки',
    contacts: 'Контакты',
    settings: 'Настройки',
    ds: 'DS · dev',
    main: 'Главная навигация',
    processingOne: 'обрабатываем',
    processingMany: 'обрабатываем · {n}',
    processingTitle: 'Обработка {n} {plural}…',
    callsPluralOne: 'звонка',
    callsPluralMany: 'звонков',
    brandFooter: 'Локально · macOS',
  },

  // ── Common buttons / labels / states ────────────────────────────────────
  common: {
    save: 'Сохранить',
    cancel: 'Отмена',
    delete: 'Удалить',
    deleting: 'Удаляем…',
    confirm: 'Подтвердить',
    confirmed: 'Подтверждены',
    confirming: 'Сохраняем…',
    close: 'Закрыть',
    edit: 'Редактировать',
    download: 'Скачать',
    loading: 'Загрузка…',
    loadingShort: '…',
    ok: 'OK',
    next: 'Дальше →',
    back: '← Назад',
    backAll: '← Все звонки',
    skip: 'Пропустить',
    done: 'Готово',
    gotIt: 'Понятно ✓',
    later: 'Позже',
    refresh: 'Обновить',
    refreshNow: '↻ Обновить',
    saved: '✓ Сохранено',
    add: 'Добавить',
    create: 'Создать',
    finish: 'Готово',
    select: 'Выбрать',
    open: 'Открыть',
    unbind: 'Отвязать',
    notDetermined: 'Не определены',
    newContact: 'Новый контакт',
    error: 'Ошибка',
    actions: 'Действия',
    progress: 'Прогресс',
    yes: 'Да',
    no: 'Нет',
    none: '—',
    chooseFile: 'Выбрать файл',
    selectNone: '— не выбран —',
    selectSearch: 'Поиск…',
  },

  // ── HomePage ────────────────────────────────────────────────────────────
  home: {
    eyebrowRecording: '● Идёт запись · Локально',
    readyHeadline: 'Готов записывать.',
    readyHeadlineRecording: 'Запись идёт фоном.',
    readyHeadlinePaused: 'Запись на паузе.',
    subtitle:
      'Нажмите красный кружок когда начнёте звонок. Расшифровка приходит через 10–30 секунд.',
    subtitlePaused:
      'Звук сейчас не пишется. Продолжите, когда вернётесь к разговору — или остановите запись из полоски сверху.',
    closeWhileRecordingTitle: 'Идёт запись',
    closeWhileRecordingBody:
      'Окно будет закрыто, и запись остановится. Аудио и расшифровка будут сохранены. Продолжить?',
    closeWhileRecordingOk: 'Остановить и закрыть',
    startAria: 'Начать запись',
    startingAria: 'Запускаем',
    starting: 'Запускаем…',
    hotkeyHint: 'Или просто нажмите горячую клавишу',
    hotkeyTitle: 'Горячая клавиша: {chord}',
    stopAria: 'Остановить запись',
    savedTitle: '✓ Звонок сохранён',
    savedHint: 'Длительность: {sec} сек. Распознавание идёт в фоне — обычно занимает 10–30 секунд.',
    statTotal: 'Звонков · всего',
    statWeek: 'За неделю',
    statArchive: 'В архиве',
    statPending: 'Ждут подтверждения',
    hoursAbbr: 'ч',
    recentTitle: 'Недавно',
    allCalls: 'Все звонки →',
    fallbackCallTitle: 'Звонок {short}',
    consentEyebrow: 'Согласие на запись',
    consentTitle: 'Перед стартом',
    consentBody:
      'Wotold будет записывать звук с микрофона и системный аудиовыход. Перед началом убедись, что собеседник предупреждён и согласен на запись. По закону РФ/РК запись переговоров без уведомления другой стороны может быть нарушением.',
    consentSubnote: 'Это окно появляется один раз. В дальнейшем будем доверять твоему решению.',
    consentAccept: 'Согласен, начать',
    updateAvailable: 'Доступна версия {version} (сейчас {current}).',
    updateInstall: 'Обновить сейчас',
    updateInstalling: 'Устанавливаем…',
    // [M12.7.5] Local-engine announcement для existing users.
    engineAnnouncementAria: 'Появился локальный режим',
    engineAnnouncementTitle: 'Появился локальный режим',
    engineAnnouncementBody:
      'Теперь Wotold может работать полностью на устройстве, без облака — бесплатно навсегда. Попробовать?',
    engineAnnouncementOpen: 'Открыть',
    engineAnnouncementDismiss: 'Позже',
    // [M12-v1.1] Banner variants
    engineAnnouncementDefault: {
      eyebrow: 'Локальный режим',
      title: 'Обрабатывай звонки без облака',
      beforeLabel: 'Было',
      beforeValue: 'Облако',
      afterLabel: 'Стало',
      afterValue: 'Локально · бесплатно',
    },
    engineAnnouncementFailures: {
      eyebrow: 'Частые ошибки',
      title: 'Переключись на локальный режим',
      beforeLabel: 'Сбоев за 24ч',
      beforeValue: '{count}',
    },
    engineAnnouncementQuota: {
      eyebrow: 'Лимит облака',
      title: 'Переходи на локальный — без лимитов',
      beforeLabel: 'Использовано',
      beforeValue: '{pct}%',
    },
    channelMic: 'Вы · микрофон',
    channelSystem: 'Собеседник · системный звук',
    waveformFmt: '16 кГц моно · WAV · {time}',
    transcriptionWillStart: 'Расшифровка начнётся автоматически',
    dbInf: '−∞ dB',
  },

  // ── CallsPage ───────────────────────────────────────────────────────────
  calls: {
    title: 'Звонки',
    search: 'Найти в расшифровках…',
    searchAria: 'Поиск звонков',
    filterAll: 'Все',
    filterToday: 'Сегодня',
    filterWeek: 'Неделя',
    // [V8.2] Появляется только когда хоть один звонок recording|processing.
    filterProcessing: 'В обработке',
    filteredOf: '{filtered} из {total} {plural}',
    countOf: '{n} {plural}',
    hoursSuffix: '· {n} ч',
    emptyTitle: 'Звонков пока нет',
    emptyBody: 'Начни запись на «Главной» — звонок появится здесь сразу после остановки.',
    notFoundTitle: 'Ничего не нашлось',
    notFoundBody: 'Сбрось фильтры или измени запрос.',
    badgeProcessing: 'распознаём',
    badgeFailed: 'ошибка',
    badgeRecording: '● запись',
    // [V6.3] Глобальный activity-strip над списком звонков. Показываем когда
    // ≥1 звонок в processing — успокаиваем юзера что можно закрыть окно.
    activityStripOne: 'Обрабатываем 1 звонок · можно закрыть окно',
    activityStripMany: 'Обрабатываем {n} {plural} · можно закрыть окно',
    // [V6.8] Secondary row под title в списке звонков.
    secondaryLive: 'идёт запись',
    secondaryUploading: 'Загружаем аудио',
    secondaryQueued: 'Ждёт очередь',
    secondaryEta: 'осталось ~{sec} сек',
    tooltipRecording: 'Идёт запись прямо сейчас.',
    tooltipProcessing: 'Запись завершена, идёт транскрипция через STT.',
    tooltipReady: 'Готово — есть transcript.md и raw_stt.json.',
    tooltipFailed: 'Звонок не доведён до transcript. Аудио всё ещё на диске.',
    fallbackCallTitle: 'Звонок {short}',
    callsForm1: 'звонок',
    callsForm2: 'звонка',
    callsForm5: 'звонков',
  },

  // ── CallDetailPage / tabs / panels ─────────────────────────────────────
  callDetail: {
    notFound: 'Звонок не найден.',
    tabRecap: 'Саммари',
    tabTranscript: 'Расшифровка',
    tabTasks: 'Задачи',
    tabSpeakers: 'Участники',
    actionsAria: 'Действия со звонком',
    actionsTitle: 'Действия',
    reprocess: '↻ Переобработать целиком',
    reprocessing: 'Переобработка…',
    regenerateRecap: '↻ Пересоздать саммари',
    regenerating: 'Пересоздаём…',
    // [P1.3] Elapsed timer для local LLM regen — backend шлёт каждые 15s.
    regeneratingWithElapsed: 'Пересоздаём… {sec}s',
    regenerateNoTranscript: 'Нет транскрипта для регенерации',
    // [M14 T-17] Title-only regen — отдельный lightweight LLM-call.
    regenerateTitle: '↻ Пересоздать название',
    regeneratingTitle: 'Пересоздаём название…',
    regenerateTitleFailed: 'Не удалось пересоздать название: {error}',
    regenerateTitleNoTranscript: 'Нет транскрипта для регенерации названия',
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
    engineCloud: 'облако (Wotold proxy)',
    engineLocalLight: 'локальный Qwen 1.5B (Light)',
    engineLocalBalanced: 'локальный Qwen 3B (Balanced)',
    engineLocalQuality: 'локальный Qwen 7B (Quality)',
    engineLocalGeneric: 'локальный движок',
    retry: 'Попробовать ещё раз',
    retrying: 'Перезапускаем…',
    emptyRecap: 'Саммари ещё не сгенерировано.',
    emptyTranscript: 'Транскрипт ещё не готов.',
    emptyTasks:
      'Здесь будут задачи, упомянутые в звонке. Пока Wotold их не нашёл — попробуй переобработать звонок или дождись пересборки.',
    taskStatusDone: '✓ done',
    taskStatusOpen: 'open',
    taskDueShort: '· до {date}',
    reprocessFailed: 'Не удалось перезапустить: {error}',
    regenerateFailed: 'Не удалось пересоздать саммари: {error}',
    // [V6.4] Reassurance строчка под PipelineStrip: юзер видит длинный
    // процесс и нервничает. Подтверждаем что прогресс persist-нут в DB.
    reassureCanClose: 'Можно закрыть окно — мы сохраним прогресс и закончим в фоне.',
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
    errorOpenSettings: 'Открыть настройки',
    errorDiagnosticsTitle: 'Диагностика',
    errorDiagnosticsCode: 'Код',
    errorDiagnosticsProvider: 'Провайдер',
    errorDiagnosticsAttempts: 'Попыток',
    errorDiagnosticsFirstAt: 'Первая попытка',
    errorDiagnosticsLastAt: 'Последняя попытка',
    errorDiagnosticsQuota: 'Списано из квоты',
  },

  // ── Recap tab (dossier) ─────────────────────────────────────────────────
  recap: {
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
    ownerLabel: 'Я',
    voiceN: 'Голос {n}',
    voiceFallback: 'Голос',
    suggestionRoleNone: '—',
    transcriptIdentifyChip: '? кто это',
    transcriptIdentifyTitle: 'Кто это? Подтвердить голос',
    transcriptIdentifyAria: 'Кто это? Подтвердить голос {tag}',
  },

  // ── Participants row (CallDetail header) ───────────────────────────────
  participants: {
    one: 'участник',
    few: 'участника',
    many: 'участников',
    // [Bug-fix] Anonymous chip — sortformer выделил голос, контакт не привязан.
    anonymousLabel: 'Спикер {n}',
    anonymousHint: 'Нажмите чтобы привязать к контакту',
  },

  // ── Audio scrubber ─────────────────────────────────────────────────────
  scrubber: {
    play: 'Воспроизведение',
    pause: 'Пауза',
    progressAria: 'Аудио прогресс',
    speakerJumpTitle: 'Перейти к блоку «{name}» в расшифровке',
    pausedItalic: 'пауза',
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
    title: 'Контакты',
    addAria: 'Добавить контакт',
    searchPlaceholder: 'Поиск…',
    emptyTitle: 'Контактов нет',
    emptyAddFirst: 'Жми «+» — добавь первого.',
    emptyAddCue:
      'Добавь первый — кнопка «+» слева сверху. Контакты помогают Wotold подписывать спикеров в расшифровках.',
    notFoundTitle: 'Ничего не нашлось',
    notFoundBody: 'По запросу «{query}» нет контактов.',
    sectionAm: 'А — М',
    sectionNz: 'Н — Я',
    sectionOther: 'Прочее',
    newContact: 'Новый контакт',
    addTitle: 'Добавить.',
    editEyebrow: 'Редактирование',
    contactEyebrow: 'Контакт',
    statCalls: 'Звонков',
    statRecorded: 'Записано',
    statVoiceSamples: 'Голосовые семплы',
    voiceOptIn: 'opt-in',
    voiceOff: '—',
    contactsBlock: 'Контакты',
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
    addRow: '+ строку',
    removeRowAria: 'Удалить строку',
    removeRowTitle: 'Удалить',
    identifierValue: 'значение',
    identifierKey: 'ключ',
    durationZero: '0',
    durationM: '{m}м',
    durationH: 'ч',
    durationHM: 'ч {m}м',
    sourceOwnerLabel: 'Владелец',
  },

  // ── Voice samples section ──────────────────────────────────────────────
  voiceSamples: {
    title: 'Голосовые семплы',
    emptyTitle: 'Образцов голоса пока нет',
    emptyBody:
      'Подтверди этого человека в любом звонке — Wotold начнёт сохранять короткие образцы голоса для авто-определения в будущем. Требует включённой опции «Запоминать голос».',
    quality: 'качество {pct}',
    embedBytes: '{n} байт',
    callTag: 'call:{short}',
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
    title: 'Настройки',
    breadcrumb: 'Настройки · {section}',
    saved: '✓ Сохранено',
    sectionAppearance: 'Внешний вид',
    sectionAccount: 'Учётная запись',
    sectionPermissions: 'Разрешения',
    sectionProcessing: 'Обработка звонков',
    sectionRecording: 'Запись',
    sectionSpeakers: 'Спикеры',
    sectionLabs: 'Лаборатория',
    sectionPrivacy: 'Конфиденциальность',
    sectionProcessingSubtitle:
      'Где обрабатывать ваши звонки. Локально — бесплатно, без сети. Облако — быстрее, точнее.',
    sectionRecordingSubtitle:
      'Горячие клавиши, язык расшифровки, авто-определение звонков.',
    sectionSpeakersSubtitle:
      'Узнавайте собеседников по голосу — Wotold подставит имена сам.',
    speakersAutoBindLabel: 'Автоматически привязывать спикеров к контактам',
    speakersAutoBindHint:
      'Только при высокой уверенности — можно отменить прямо в звонке.',
    speakersMicDiarizationLabel: 'Распознавать несколько голосов на микрофоне',
    speakersMicDiarizationHint:
      'Полезно для записи живых встреч в одной комнате, когда на ваш микрофон попадают и другие участники. Замедляет обработку звонка на ~10–20%. Голос владельца устройства узнаётся по накопленным образцам или по основному говорящему.',
    micDiarizationModelMissing:
      'Для разделения голосов нужен дополнительный модуль (~6 МБ). Без него Wotold не делит голоса даже когда переключатель включён.',
    micDiarizationInstall: '↓ Установить модуль разделения голосов',
    micDiarizationInstalling: 'Устанавливаем модуль…',
    appearanceTitle: 'Внешний вид.',
    appearanceLede:
      'Тема и акцент применяются мгновенно — переключи и сравни. Все экраны реагируют одновременно.',
    fieldTheme: 'Тема',
    fieldAccent: 'Акцентный цвет',
    fieldLanguage: 'Язык интерфейса',
    languageHint: 'Язык всех меток, кнопок и подсказок. Контент звонков остаётся как есть.',
    themeLight: 'Светлая',
    themeDark: 'Тёмная',
    themeSystem: 'Системная',
    accentBordeaux: 'Бордо',
    accentPersian: 'Кобальт',
    accentInk: 'Графит',
    accountTitle: 'Аккаунт.',
    accountLede:
      'Облачная синхронизация скоро. Сейчас вход ничего не разблокирует — Wotold полностью работает локально без логина.',
    permissionsTitle: 'Разрешения системы.',
    permissionsLede:
      'Wotold нужны два разрешения macOS: микрофон и запись экрана для системного звука. Без них запись не начнётся.',
    engineTitle: 'Движок распознавания.',
    engineLede:
      'Где обрабатываются ваши звонки. Локальный — бесплатно и без сети, всё на устройстве. Облачный — лучшее качество, требуется интернет.',
    sttTitle: 'Распознавание речи.',
    sttLede:
      'Поставщик STT и язык вывода для рекапа. Auto переключается между Soniox и Gladia при сбоях.',
    sttProviderLabel: 'Провайдер',
    sttProviderAuto: 'Auto (Soniox → Gladia)',
    sttRecapLangLabel: 'Язык рекапа и задач',
    sttRecapLangHint:
      "На каком языке писать рекап и задачи. 'Авто' = язык распознанной речи. Не влияет на сам STT.",
    sttModelLabel: 'LLM-модель (опционально)',
    sttModelPlaceholder: 'auto (определяется бэкендом)',
    sttModelHint:
      'Пусто = прокси сам выбирает по LLM_BACKEND. Override только если знаешь что делаешь.',
    // [V7] Auto-bind speaker по биометрии. R2 паспорта — opt-in only, default OFF.
    autoBindLabel: 'Авто-привязка собеседников по голосу',
    autoBindCheckboxLabel:
      'Автоматически привязывать собеседника к контакту при высоком совпадении голоса. Требует ≥ 2 голосовых семплов и согласия контакта на биометрию.',
    autoBindHint:
      'Если выключено — Wotold только предлагает кандидата, окончательное подтверждение за тобой (рекомендуется).',
    autoBindThresholdLabel: 'Порог совпадения',
    autoBindThresholdOption: '{n}%',
    autoBindThresholdHint:
      'Чем выше порог, тем меньше ошибок, но и реже срабатывает. 95% — баланс по умолчанию.',
    // [W1] Configurable hotkeys для recording.
    hotkeyToggleLabel: 'Горячая клавиша · старт/стоп',
    hotkeyToggleHint:
      'Нажми «Записать» и сразу зажми комбинацию (например ⌘⇧R). Esc — отмена. Системные комбинации (⌘W, ⌘C…) недоступны.',
    hotkeyPauseLabel: 'Горячая клавиша · пауза/продолжить',
    hotkeyPauseHint:
      'Срабатывает только во время активной записи. По умолчанию ⌘⇧P.',
    // [S1] Auto-detect (R3 deviation, opt-in).
    callDetectLabel: 'Авто-предложение записи',
    callDetectCheckboxLabel:
      'Когда система детектит звонок (микрофон активен другим приложением + видны Zoom/Teams/Meet/FaceTime/Discord/Telegram), Wotold показывает уведомление «Записать?». Работает даже если окно свёрнуто. Опираемся на macOS Core Audio + список приложений; никакая аудио-дорожка чужого приложения не читается.',
    callDetectHint:
      'Если выключено — Wotold никогда не предлагает запись сам, только по ⌘⇧R или клику. Рекомендуется выключено для приватности (R3 паспорта).',
    callDetectCooldownLabel: 'Не предлагать снова в течение',
    callDetectCooldownOption: '{n} мин',
    callDetectCooldownHint:
      'Если ты закрыл предложение — для того же приложения не покажем минимум столько. Cooldown сбрасывается на каждом перезапуске Wotold.',
    pathTitle: 'Источник сервисов.',
    pathLede:
      'По умолчанию Wotold ходит через прокси с дневной бесплатной квотой. Подключи свои ключи — и запросы пойдут напрямую, без лимитов.',
    pathManagedExplain:
      'Через Wotold — managed-режим. Все запросы STT/LLM идут через наш прокси. Бесплатный тир: 60 минут STT и 50 тыс. токенов LLM в день. Превышение — мягкий отказ, без списаний.',
    pathByoExplain:
      'Свои ключи — BYO-режим. Wotold ходит напрямую к Soniox/Gladia/Anthropic с твоими ключами. Ключи хранятся в системном Keychain, не в БД и не в логах.',
    pathByoToggle: 'Свои ключи',
    pathManagedToggle: 'Через прокси',
    pathTogglePath: 'Путь',
    pathKeychainNote: 'Ключи хранятся в системном Keychain',
    keysTitle: 'Свои ключи API.',
    keysLede:
      'Подключи ключи Soniox · Gladia · Anthropic — Wotold пойдёт напрямую, мимо нашего прокси. Ключи живут в Keychain macOS.',
    keysStored: 'Ключи хранятся в системном Keychain. Не пишутся в БД, логи или телеметрию.',
    keysEmptyAll:
      '⚠ Ни один ключ не задан. Записи будут падать с ошибкой авторизации — либо добавь ключи, либо переключись на «Через Wotold» в выборе режима.',
    keysSomeMissing:
      'ⓘ Не заданы: {names}. Без них часть pipeline (STT primary / fallback / recap) работать не будет.',
    keyConnected: '● подключён',
    keyEmpty: '● пусто',
    keyReplacePlaceholder: '••••• (введи, чтобы заменить)',
    keyNeedValue: 'Введи значение ключа.',
    keySonioxHint: 'STT primary. Получить ключ — soniox.com/console.',
    keyGladiaHint: 'STT fallback. Получить ключ — app.gladia.io/api.',
    keyAnthropicHint: 'LLM рекап. Получить ключ — console.anthropic.com.',
    proxyTitle: 'Сервер Wotold.',
    proxyLede:
      'Managed-прокси — общий endpoint для STT/LLM. Можно подменить URL на staging или self-hosted, если знаешь что делаешь.',
    proxyEndpointLabel: 'Endpoint:',
    proxyDefaultMark: ' · default',
    proxyCustomLabel: 'Custom Proxy URL',
    proxyCustomHint:
      'Override для staging или self-hosted прокси. Оставь пустым для default.',
    proxyInvalidUrl: 'URL должен быть http:// или https://',
    usageTitle: 'Использование.',
    usageLede:
      'Дневная квота managed-режима — STT-минуты и LLM-токены. Сбрасывается каждые 24 часа. В BYO-режиме счётчик не действует.',
    voiceTitle: 'Распознавание голоса.',
    voiceLede:
      'Wotold может предлагать кто говорит на основе совпадения голоса — но только после скачивания биометрической модели (25 МБ, опционально). Финальное подтверждение всегда за тобой (R2 паспорта).',
    privacyTitle: 'Конфиденциальность.',
    privacyLede:
      'Полная очистка локальных данных. Полезно перед передачей устройства другому человеку или при отзыве согласия.',
    // [M14 T-14] Labs section — experimental flags.
    labsTitle: 'Лаборатория.',
    labsLede:
      'Экспериментальные функции. Включены по умолчанию — выключай если что-то ломается.',
    summaryV2Label: 'Новый формат саммари',
    summaryV2Hint:
      'Включено по умолчанию. Выключите если возникли проблемы с типом звонка, цитатами или решениями — рекапы вернутся к простому формату.',
    speculativeDecodingLabel: 'Ускорение генерации (черновая модель)',
    speculativeDecodingHint:
      'Использует малую модель 0.5B для draft-токенов параллельно с 7B Quality. 2-3× speedup. Требует preset «Quality» и скачивание дополнительной модели ~380MB.',
    forceNumSpeakersLabel: 'Принудительное количество спикеров',
    forceNumSpeakersHint:
      'Применяется к следующей переобработке. Используй если знаешь точное количество собеседников и автоматика ошибается. Лимит — 4 спикера.',
    forceNumSpeakersOptions: {
      auto: 'Авто (рекомендовано)',
      '2': '2 спикера',
      '3': '3 спикера',
      '4': '4 спикера',
    },
    wipeBtn: 'Удалить все данные',
    wipeBusy: 'Удаляем…',
    wipeConfirmTitle: 'Wotold — Полная очистка',
    wipeConfirmBody:
      'УДАЛИТЬ ВСЕ ДАННЫЕ?\n\nЭто навсегда сотрёт:\n  • все записи звонков и аудио\n  • все контакты и voice samples\n  • сессию входа и BYO API-ключи\n\nДействие необратимо.',
    wipeConfirmOk: 'Удалить всё',
    wipeDone:
      '✓ Все данные удалены. Закрой и заново открой Wotold чтобы начать с чистой установки.',
  },

  // ── Account / OIDC ──────────────────────────────────────────────────────
  account: {
    intro:
      'Облачная синхронизация скоро. Сейчас вход в аккаунт ничего не разблокирует — Wotold полностью работает локально без логина.',
    sessionUntil: 'Session действует до {date}',
    signOut: 'Выйти',
    needSessionToken: 'Введи session token из браузера.',
    step1: 'Шаг 1.',
    step1Body: 'В браузере открылась страница входа',
    step1Body2: '. Войди и подтверди.',
    step2: 'Шаг 2.',
    step2Body:
      'После успешного входа прокси покажет JSON с полем sessionId. Скопируй значение sessionId и вставь сюда.',
    sessionIdLabel: 'Session ID',
    sessionIdPlaceholder: 'UUID из ответа прокси',
    deepLinkHint: 'Авто-перехват callback (без копи-пасты) — в плане через deep-link',
    signInPrompt: 'Войти через SSO. Откроется браузер.',
    soon: 'скоро',
    insecureAuthUrl: 'Прокси вернул небезопасный authorize URL (не https://): {url}…',
  },

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
  usage: {
    refreshing: '…',
    refreshLabel: '↻ Обновить',
    loading: 'Загружаем данные…',
    errorIntro:
      'Не удалось получить данные использования. Это нормально если ты offline или прокси не настроен.',
    tier: 'tier: {name}',
    sttLabel: 'STT (распознавание речи)',
    llmLabel: 'LLM (рекапы, нудж-вопросы)',
    secAbbr: '{n} сек',
    minAbbr: '{n} мин',
    minSecAbbr: '{m} мин {s} сек',
    tokens: '{n} токенов',
    resetAt: 'Сброс счётчиков: {date}',
    noLimit: 'лимит не настроен',
  },

  // ── [M12-v1.1] Engine chip labels ──────────────────────────────────────
  engineChip: {
    local: 'Локально',
    cloud_managed: 'Облако',
    cloud_byo: 'Свои ключи',
    localAria: 'Обработано локально на устройстве',
    cloud_managedAria: 'Обработано в облаке Wotold',
    cloud_byoAria: 'Обработано через собственные ключи',
  },

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

  // [M14 T-11] Decisions block — список решений из cloud v2 recap.
  decisionsBlock: {
    title: 'Решения',
  },

  // [M14 T-11] Open questions block — нерешённые вопросы из звонка.
  openQuestionsBlock: {
    title: 'Открытые вопросы',
    raisedBy: 'поднял(а)',
  },

  // [M14 T-11] Evidence quote tooltips и confidence indicators.
  evidence: {
    fromTranscript: 'Цитата из расшифровки',
    jumpToMoment: 'К моменту в записи',
    lowConfidence: 'Невысокая уверенность — проверь цитату',
    speakerLabel: 'Говорит',
  },

  // [M14 T-11] Privacy disclaimer для 1:1 встреч (содержит личную обратную связь).
  privacyDisclaimer: {
    oneOnOneTitle: '🔒 Это была встреча 1:1',
    oneOnOneBody:
      'Содержит личную обратную связь. Саммари показывает только темы. Не делись без необходимости.',
  },

  // [M14 T-11] Action item v2 — confidence badges + категории + evidence.
  actionItem: {
    ownerInferred: 'Исполнитель определён по контексту — проверь',
    unassigned: 'Без владельца',
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
    modelEyebrow: 'Модель',
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
    btnDeleting: 'Удаляем…',
    techDetails: 'Технические детали',
    techUrl: 'URL',
    techSha: 'SHA256',
    techSize: 'Размер',
    techFeature: 'Build feature',
    featureEnabled: 'voice-onnx ✓',
    featureDisabled: '— (не включена)',
    mb: '{n} МБ',
    verifyFailed:
      'SHA256 не совпал — файл повреждён или сменилась версия модели. Попробуй снова.',
  },

  // ── Local engine (M12) ──────────────────────────────────────────────────
  localEngine: {
    engineLabel: 'Где обрабатывать звонки',
    engine: {
      local: {
        title: 'Локально на устройстве',
        body: 'Без сети, без оплат, ваши данные не покидают Mac. Скачиваются модели один раз.',
        quality: '●●○ качество',
      },
      cloud_managed: {
        title: 'Облако Wotold (Pro)',
        body: 'Лучшее качество, быстро, без локальной нагрузки. Требуется интернет.',
        quality: '●●● качество',
      },
      cloud_byo: {
        title: 'Свои ключи провайдеров',
        body: 'Напрямую к Soniox / Anthropic своими API-ключами. Без квоты Wotold.',
        quality: '●●● качество',
      },
      active: 'активен',
    },
    presetLabel: 'Сборка моделей',
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
    },
    statusInstalled: 'установлено',
    statusDownloading: 'качаем…',
    statusAbsent: 'не установлено',
    installedFootprint: 'Установлено: {size}',
    manageStorage: 'Освободить место',
    storageTitle: 'Хранилище моделей',
    storageLede:
      'Что установлено локально. Удалите неиспользуемые модели чтобы освободить место. Wotold не удаляет модели сам.',
    storageFootnote: 'Удаление не трогает уже обработанные звонки.',
    colName: 'Модель',
    colSize: 'Размер',
    colLastUsed: 'Активно',
    colState: 'Статус',
    download: 'Скачать',
    downloadAria: 'Скачать {name}',
    delete: 'Удалить',
    deleteAria: 'Удалить {name}',
    close: 'Закрыть',
    statusActive: 'активна',
    statusCorrupted: 'повреждена',
    deleteActiveConfirmTitle: 'Удалить активную модель?',
    deleteActiveConfirmMsg:
      'Модель {id} используется текущей сборкой. Удаление переключит сборку. Продолжить?',
    qualityConfirmTitle: 'Quality на этом Mac',
    qualityConfirmMsg:
      'Quality рассчитан на 16+ ГБ оперативной памяти. На вашем Mac обработка может быть очень медленной. Всё равно использовать?',
    deleteConfirmTitle: 'Удалить модель',
    deleteConfirmMsg: 'Удалить {id} с диска? Скачать можно будет снова.',
    verifyFailed: 'Контрольная сумма не совпала для {id} — файл повреждён, попробуйте снова.',
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
    storageConfirm: {
      title: 'Удалить активную модель?',
      body: 'Текущая сборка переключится на «{fallback}». Модель можно скачать снова в любое время.',
      confirm: 'Удалить',
      cancel: 'Отмена',
    },
    // ── [M12-v1.1] Rediscovery chip ───────────────────────────────────────
    rediscovery: {
      eyebrow: 'Локальный режим',
      title: 'Обрабатывай звонки прямо на Mac',
      body: 'Без облака, бесплатно навсегда. Скачивается один раз — потом работает офлайн.',
      install: 'Попробовать локальный режим',
      dismiss: 'Больше не показывать',
    },
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
      useCloudCta: 'Использовать облако вместо локального',
      downloadingLabel: 'Качаем {id}',
      cancelDownloadCta: 'Отменить и продолжить с облаком',
      verifyFailed: 'Контрольная сумма не совпала для {id}. Попробуйте ещё раз.',
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
    overlayLabel: 'Идёт запись',
    // [W3] RecStrip labels — persistent strip rendered above main content
    // while a recording is active (or paused).
    stripRecording: 'Идёт запись',
    stripPaused: 'Пауза · записано',
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
  callState: {
    // Badge labels
    live: 'идёт запись',
    uploading: 'загружаем',
    queued: 'в очереди',
    processing: 'распознаём',
    ready: 'готов',
    error: 'ошибка',
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
  // [M13.3.3] Chunked pipeline (длинные звонки нарезаются на 10-мин сегменты).
  chunkProgress: {
    label: 'Сегменты',
    ofN: '{done} из {total}',
    statusDone: 'готово',
    statusFailed: 'не удалось',
    statusProcessing: 'обрабатываем',
    statusPending: 'ожидает',
    // [Tech-debt P0.2] Retry failed chunk.
    retry: '↻ Повторить',
    retrying: 'Повторяем…',
    failedSummary: '{n} из {total} сегментов не удалось — нажми ↻ чтобы переcпавнить.',
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
