/**
 * Тексты лендинга. Отделены от разметки, чтобы en/kk добавлялись правкой
 * одного файла, а не копированием секций (правило «i18n тотален» из CLAUDE.md
 * действует и на сайт).
 *
 * Фактура берётся из README и docs/ПАСПОРТ_ПРОЕКТА.md. Обещания здесь не
 * должны опережать код: если фича planned — она не описывается настоящим
 * временем.
 */

export type Lang = 'ru' | 'en' | 'kk';

export interface LandingCopy {
  meta: { title: string; description: string };
  hero: {
    eyebrow: string;
    title: string;
    lead: string;
    primary: string;
    secondary: string;
    note: string;
    mockupAlt: string;
  };
  features: { eyebrow: string; title: string; items: FeatureItem[] };
  how: { eyebrow: string; title: string; lead: string; steps: Step[] };
  privacy: { eyebrow: string; title: string; lead: string; points: string[]; link: string };
  cta: { title: string; lead: string; primary: string; secondary: string; note: string };
  sponsor: { title: string; lead: string; action: string };
  footer: {
    links: { label: string; href: string }[];
    /** Правовой блок подвала: отдельная колонка, не вперемешку со ссылками на GitHub. */
    legalHeading: string;
    legal: { label: string; slug: string }[];
    license: string;
  };
}

/** Общие для всех локалей: слаг один, подпись переводится. */
const LEGAL_SLUGS = ['legal/privacy', 'legal/consent', 'legal/terms', 'legal/license'] as const;

export interface FeatureItem {
  icon: 'users' | 'mic' | 'chat' | 'code' | 'cpu' | 'wifiOff';
  title: string;
  body: string;
}

export interface Step {
  title: string;
  body: string;
}

const REPO = 'https://github.com/zdllucky/wotold';

export const LANDING: Record<Lang, LandingCopy> = {
  ru: {
    meta: {
      title: 'Wotold — запись звонков с расшифровкой по спикерам, локально на Mac',
      description:
        'Десктоп-утилита: запись звонка, транскрипция, диаризация, саммари и поиск по архиву — всё считается на устройстве, без сети и подписки.',
    },
    hero: {
      eyebrow: 'macOS · локально · Apache-2.0',
      title: 'Расшифровка звонков, которая знает, кто говорил',
      lead: 'Wotold записывает звонок, разделяет реплики по голосам, собирает саммари и отвечает на вопросы по всему архиву. Всё считается на твоём Mac: без серверов, без аккаунта, без подписки. Единственный сетевой вызов за всё время работы — разовое скачивание моделей.',
      primary: 'Скачать для macOS',
      secondary: 'Как это работает',
      note: 'macOS 14+. Сборка не нотаризована — при первом запуске нужно разрешить её в настройках безопасности.',
      mockupAlt:
        'Окно приложения: список реплик звонка, разделённых по спикерам, с таймкодами и чипами решений и задач.',
    },
    features: {
      eyebrow: 'Что внутри',
      title: 'Пять вещей, ради которых это писалось',
      items: [
        {
          icon: 'users',
          title: 'Диаризация, а не стена текста',
          body: 'Существующие инструменты расшифровывают звонок сплошным потоком без атрибуции — из-за этого рекапы и поиск бесполезны. Wotold разделяет реплики по голосам, и только после этого «кто что обещал» становится вопросом, на который есть ответ.',
        },
        {
          icon: 'mic',
          title: 'Голосовые отпечатки',
          body: 'Wotold подсказывает «возможно, это Иван» по голосу из адресной книги. Привязка к контакту — всегда твоё явное подтверждение: автоматически не присваивается никогда.',
        },
        {
          icon: 'chat',
          title: 'Ассистент по всему архиву',
          body: 'Локальный чат отвечает на вопросы по всей базе звонков — гибридный поиск по ключевым словам и смыслу, ответ генерирует модель на устройстве, с ссылками на конкретные фрагменты расшифровки.',
        },
        {
          icon: 'code',
          title: 'MCP-сервер для Claude',
          body: 'Локальный read-only сервер даёт Claude прямой доступ к расшифровкам: поиск, цитирование и Q&A без копипаста. Записывать он не умеет по устройству — только читать.',
        },
        {
          icon: 'cpu',
          title: 'Движок на устройстве',
          body: 'whisper.cpp для распознавания, sherpa-onnx для диаризации, llama.cpp для саммари и ответов. Пресеты Light / Balanced / Quality — обмен качества на время и нагрев.',
        },
      ],
    },
    how: {
      eyebrow: 'Пайплайн',
      title: 'От нажатия «Запись» до ответа на вопрос',
      lead: 'Микрофон и системный звук пишутся раздельными дорожками — поэтому твою речь никогда не перепутать с чужой.',
      steps: [
        { title: 'Запись', body: 'Кнопка или ⌘⇧R. Две дорожки: микрофон и системный выход.' },
        { title: 'Распознавание', body: 'whisper.cpp на устройстве. Длинные записи режутся на части.' },
        { title: 'Диаризация', body: 'sherpa-onnx разделяет голоса и сопоставляет их с контактами.' },
        { title: 'Саммари', body: 'Локальная модель собирает рекап, решения и задачи.' },
        { title: 'Поиск', body: 'Всё уходит в индекс: ассистент, поиск и MCP работают по нему.' },
      ],
    },
    privacy: {
      eyebrow: 'Приватность',
      title: 'Обещание, которое можно проверить по коду',
      lead: 'Wotold не имеет серверной части: её не отключали ради приватности, её просто нет. Проверяется это не на слово — репозиторий открыт.',
      points: [
        'Аудио, расшифровки и рекапы лежат только в каталоге приложения на твоём диске.',
        'Нет аккаунтов, нет облачного хранения, нет телеметрии и аналитики.',
        'Единственный исходящий запрос за всё время — скачивание моделей с HuggingFace при первом включении. Дальше приложение работает офлайн.',
        'Голосовые отпечатки — opt-in по каждому контакту. Удаление контакта удаляет и его семплы.',
        'Экран не записывается: системный звук берётся через ScreenCaptureKit, но кадры не сохраняются.',
      ],
      link: 'Полная политика конфиденциальности',
    },
    cta: {
      title: 'Поставить и попробовать',
      lead: 'Нужен Mac на macOS 14 или новее. Приложение бесплатное, регистрации нет.',
      primary: 'Скачать .dmg',
      secondary: 'Собрать из исходников',
      note: 'При первом запуске Wotold скачает модели — от 2 до 7 ГБ в зависимости от выбранного пресета.',
    },
    sponsor: {
      title: 'Поддержать проект',
      lead: 'Wotold пишет один человек, и у проекта нет ни подписки, ни платной версии. Если он экономит тебе время — спонсорство помогает продолжать.',
      action: 'GitHub Sponsors',
    },
    footer: {
      links: [
        { label: 'GitHub', href: REPO },
        { label: 'Issues', href: `${REPO}/issues` },
        { label: 'Discussions', href: `${REPO}/discussions` },
        { label: 'Releases', href: `${REPO}/releases` },
      ],
      legalHeading: 'Правовая информация',
      legal: [
        { label: 'Конфиденциальность', slug: LEGAL_SLUGS[0] },
        { label: 'Уведомление о записи', slug: LEGAL_SLUGS[1] },
        { label: 'Условия использования', slug: LEGAL_SLUGS[2] },
        { label: 'Лицензия', slug: LEGAL_SLUGS[3] },
      ],
      license: 'Исходный код — под лицензией Apache 2.0.',
    },
  },

  en: {
    meta: {
      title: 'Wotold — call transcripts that know who was speaking, locally on your Mac',
      description:
        'Desktop app: record a call, transcribe it, split it by speaker, summarize, and search your archive — all computed on-device, no network, no subscription.',
    },
    hero: {
      eyebrow: 'macOS · local · Apache-2.0',
      title: 'Call transcripts that know who was speaking',
      lead: 'Wotold records the call, splits it by voice, writes the summary, and answers questions across your whole archive. All of it runs on your Mac: no servers, no account, no subscription. The single network request in the product’s lifetime is the one-time model download.',
      primary: 'Download for macOS',
      secondary: 'How it works',
      note: 'macOS 14+. The build is not notarized — the first launch needs a manual approval in Security settings.',
      mockupAlt:
        'Application window showing call turns split by speaker, with timestamps and chips for decisions and action items.',
    },
    features: {
      eyebrow: 'What is inside',
      title: 'Five things this was written for',
      items: [
        {
          icon: 'users',
          title: 'Diarization, not a wall of text',
          body: 'Existing tools transcribe a call as one undifferentiated stream, which makes recaps and search useless — you cannot get "who promised what" out of it. Wotold splits turns by voice, and only then does that question have an answer.',
        },
        {
          icon: 'mic',
          title: 'Voice fingerprints',
          body: 'Wotold suggests "this might be Ivan" based on a voice from your address book. Binding a speaker to a contact is always your explicit confirmation — it never happens automatically.',
        },
        {
          icon: 'chat',
          title: 'An assistant over the whole archive',
          body: 'A local chat answers questions across every call you have — hybrid keyword and semantic retrieval, an on-device model generating the answer, with citations pointing at specific transcript fragments.',
        },
        {
          icon: 'code',
          title: 'MCP server for Claude',
          body: 'A local read-only server gives Claude direct access to your transcripts: search, citation and Q&A without copy-paste. It has no write tools at all — by construction, not by policy.',
        },
        {
          icon: 'cpu',
          title: 'The engine runs on your machine',
          body: 'whisper.cpp for recognition, sherpa-onnx for diarization, llama.cpp for summaries and answers. Light / Balanced / Quality presets trade quality against time and heat.',
        },
      ],
    },
    how: {
      eyebrow: 'Pipeline',
      title: 'From pressing Record to getting an answer',
      lead: 'Microphone and system audio are captured as separate tracks — which is why your own speech can never be confused with somebody else’s.',
      steps: [
        { title: 'Record', body: 'A button or ⌘⇧R. Two tracks: microphone and system output.' },
        { title: 'Recognize', body: 'whisper.cpp on-device. Long recordings are split into chunks.' },
        { title: 'Diarize', body: 'sherpa-onnx separates voices and matches them against contacts.' },
        { title: 'Summarize', body: 'A local model produces the recap, decisions and action items.' },
        { title: 'Index', body: 'Everything lands in the index that powers assistant, search and MCP.' },
      ],
    },
    privacy: {
      eyebrow: 'Privacy',
      title: 'A promise you can verify by reading the code',
      lead: 'Wotold has no backend. It was not switched off for privacy — it does not exist. You do not have to take that on faith: the repository is open.',
      points: [
        'Audio, transcripts and recaps live only in the application directory on your disk.',
        'No accounts, no cloud storage, no telemetry, no analytics.',
        'The only outbound request ever made is the model download from HuggingFace on first use. After that the app works offline.',
        'Voice fingerprints are opt-in per contact. Deleting a contact deletes their samples.',
        'The screen is never recorded: system audio comes through ScreenCaptureKit, but no frames are stored.',
      ],
      link: 'Full privacy policy',
    },
    cta: {
      title: 'Install and try it',
      lead: 'Requires a Mac running macOS 14 or newer. Free, no sign-up.',
      primary: 'Download the .dmg',
      secondary: 'Build from source',
      note: 'On first launch Wotold downloads models — 2 to 7 GB depending on the preset you pick.',
    },
    sponsor: {
      title: 'Support the project',
      lead: 'Wotold is written by one person, and there is no subscription and no paid tier. If it saves you time, sponsorship is what keeps it going.',
      action: 'GitHub Sponsors',
    },
    footer: {
      links: [
        { label: 'GitHub', href: REPO },
        { label: 'Issues', href: `${REPO}/issues` },
        { label: 'Discussions', href: `${REPO}/discussions` },
        { label: 'Releases', href: `${REPO}/releases` },
      ],
      legalHeading: 'Legal',
      legal: [
        { label: 'Privacy policy', slug: LEGAL_SLUGS[0] },
        { label: 'Recording notice', slug: LEGAL_SLUGS[1] },
        { label: 'Terms of use', slug: LEGAL_SLUGS[2] },
        { label: 'License', slug: LEGAL_SLUGS[3] },
      ],
      license: 'Source code is licensed under Apache 2.0.',
    },
  },

  kk: {
    meta: {
      title: 'Wotold — кім сөйлегенін білетін қоңырау транскрипциясы, Mac-та жергілікті',
      description:
        'Десктоп қосымша: қоңырауды жазу, транскрипция, сөйлеушілерге бөлу, түйіндеме және архив бойынша іздеу — бәрі құрылғыда, желісіз және жазылымсыз.',
    },
    hero: {
      eyebrow: 'macOS · жергілікті · Apache-2.0',
      title: 'Кім сөйлегенін білетін қоңырау транскрипциясы',
      lead: 'Wotold қоңырауды жазады, репликаларды дауыс бойынша бөледі, түйіндеме жасайды және бүкіл архив бойынша сұрақтарға жауап береді. Бәрі сіздің Mac-ыңызда есептеледі: серверсіз, аккаунтсыз, жазылымсыз. Жұмыс кезіндегі жалғыз желілік сұрау — модельдерді бір рет жүктеу.',
      primary: 'macOS үшін жүктеу',
      secondary: 'Қалай жұмыс істейді',
      note: 'macOS 14+. Құрастырма нотаризацияланбаған — алғаш іске қосқанда қауіпсіздік баптауларынан рұқсат беру керек.',
      mockupAlt:
        'Қосымша терезесі: сөйлеушілерге бөлінген қоңырау репликалары, уақыт белгілері мен шешімдер және тапсырмалар чиптері.',
    },
    features: {
      eyebrow: 'Ішінде не бар',
      title: 'Бұл жоба жазылған бес себеп',
      items: [
        {
          icon: 'users',
          title: 'Диаризация, мәтін қабырғасы емес',
          body: 'Қолда бар құралдар қоңырауды тұтас ағынмен, авторлықсыз жазып береді — сондықтан түйіндеме де, іздеу де пайдасыз: «кім не уәде етті» деген сұраққа жауап шықпайды. Wotold репликаларды дауыс бойынша бөледі, тек содан кейін бұл сұрақтың жауабы пайда болады.',
        },
        {
          icon: 'mic',
          title: 'Дауыс іздері',
          body: 'Wotold мекенжай кітапшасындағы дауыс бойынша «бұл Иван болуы мүмкін» деп ұсынады. Контактіге байлау — әрқашан сіздің нақты растауыңыз, автоматты түрде ешқашан болмайды.',
        },
        {
          icon: 'chat',
          title: 'Бүкіл архив бойынша көмекші',
          body: 'Жергілікті чат барлық қоңыраулар базасы бойынша сұрақтарға жауап береді — кілт сөз бен мағына бойынша аралас іздеу, жауапты құрылғыдағы модель жасайды және транскрипцияның нақты үзінділеріне сілтеме береді.',
        },
        {
          icon: 'code',
          title: 'Claude үшін MCP сервері',
          body: 'Жергілікті read-only сервер Claude-қа транскрипцияларға тікелей қолжетімділік береді: іздеу, дәйексөз және Q&A көшіріп-қоюсыз. Жазу құралдары мүлде жоқ.',
        },
        {
          icon: 'cpu',
          title: 'Қозғалтқыш құрылғыда',
          body: 'Тану үшін whisper.cpp, диаризация үшін sherpa-onnx, түйіндеме мен жауаптар үшін llama.cpp. Light / Balanced / Quality пресеттері сапаны уақыт пен қызуға айырбастайды.',
        },
      ],
    },
    how: {
      eyebrow: 'Құбыр',
      title: '«Жазу» түймесінен жауапқа дейін',
      lead: 'Микрофон мен жүйелік дыбыс бөлек жолдармен жазылады — сондықтан сіздің сөзіңіз ешқашан бөтен дауыспен шатаспайды.',
      steps: [
        { title: 'Жазу', body: 'Түйме немесе ⌘⇧R. Екі жол: микрофон және жүйелік шығыс.' },
        { title: 'Тану', body: 'Құрылғыдағы whisper.cpp. Ұзын жазбалар бөліктерге бөлінеді.' },
        { title: 'Диаризация', body: 'sherpa-onnx дауыстарды бөліп, контактілермен салыстырады.' },
        { title: 'Түйіндеме', body: 'Жергілікті модель түйіндеме, шешімдер мен тапсырмаларды жинайды.' },
        { title: 'Индекс', body: 'Бәрі индекске түседі: көмекші, іздеу және MCP содан жұмыс істейді.' },
      ],
    },
    privacy: {
      eyebrow: 'Құпиялылық',
      title: 'Кодпен тексеруге болатын уәде',
      lead: 'Wotold-та сервер бөлігі жоқ: ол құпиялылық үшін өшірілген емес, ол мүлде жоқ. Мұны сөзге сеніп қабылдаудың қажеті жоқ — репозиторий ашық.',
      points: [
        'Аудио, транскрипциялар мен түйіндемелер тек сіздің дискідегі қосымша каталогында жатады.',
        'Аккаунттар жоқ, бұлттық сақтау жоқ, телеметрия мен аналитика жоқ.',
        'Жұмыс кезіндегі жалғыз шығыс сұрау — алғаш қосқанда HuggingFace-тен модельдерді жүктеу. Одан кейін қосымша офлайн жұмыс істейді.',
        'Дауыс іздері — әр контакт бойынша opt-in. Контактіні жою оның үлгілерін де жояды.',
        'Экран жазылмайды: жүйелік дыбыс ScreenCaptureKit арқылы алынады, бірақ кадрлар сақталмайды.',
      ],
      link: 'Толық құпиялылық саясаты',
    },
    cta: {
      title: 'Орнатып көру',
      lead: 'macOS 14 немесе жаңарақ Mac қажет. Тегін, тіркелусіз.',
      primary: '.dmg жүктеу',
      secondary: 'Бастапқы кодтан құрастыру',
      note: 'Алғаш іске қосқанда Wotold модельдерді жүктейді — таңдалған пресетке қарай 2-ден 7 ГБ-қа дейін.',
    },
    sponsor: {
      title: 'Жобаны қолдау',
      lead: 'Wotold-ты бір адам жазады, жобада жазылым да, ақылы нұсқа да жоқ. Егер ол сіздің уақытыңызды үнемдесе — демеушілік жалғастыруға көмектеседі.',
      action: 'GitHub Sponsors',
    },
    footer: {
      links: [
        { label: 'GitHub', href: REPO },
        { label: 'Issues', href: `${REPO}/issues` },
        { label: 'Discussions', href: `${REPO}/discussions` },
        { label: 'Releases', href: `${REPO}/releases` },
      ],
      legalHeading: 'Құқықтық ақпарат',
      legal: [
        { label: 'Құпиялылық саясаты', slug: LEGAL_SLUGS[0] },
        { label: 'Жазба туралы хабарлама', slug: LEGAL_SLUGS[1] },
        { label: 'Пайдалану шарттары', slug: LEGAL_SLUGS[2] },
        { label: 'Лицензия', slug: LEGAL_SLUGS[3] },
      ],
      license: 'Бастапқы код Apache 2.0 лицензиясымен таратылады.',
    },
  },
};
