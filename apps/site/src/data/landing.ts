/**
 * Копирайт лендинга — три локали, один типизированный файл («i18n тотален»).
 *
 * Источник текстов — копи-дек `site-i18n.js` из дизайн-хендоффа 2026-07-30
 * (design_handoff_wotold_site). Тексты писались под нетехническую аудиторию
 * и перенесены ДОСЛОВНО — «диаризация», «локально», «Apple Silicon» здесь
 * отсутствуют намеренно, не «улучшать» обратно в технические формулировки.
 * kk — черновик: вычитать носителем перед публикацией.
 *
 * Санкционированные отклонения от дека (перечислены в PR):
 * - абсолютные URL уведомления о записи в HTML-строках заменены плейсхолдером
 *   %LEGAL_CONSENT% — интерполируется base+locale-хелпером при рендере;
 * - card.done хранится без ведущего «✓ » — глиф стал line-иконкой check
 *   в разметке (канон: без emoji);
 * - meta.* составлены из hero.* (в деке ключа meta нет);
 * - инициалы аватаров карточки не хранятся: вычисляются из card.m1w[0] /
 *   m2w[0] (в эталоне «В» был захардкожен и не переводился).
 *
 * HTML-несущие ключи (рендер через set:html): card.s1–s3 (<b>), card.ref (<u>),
 * trust.consent (<a>), faq.items[].a (<a>).
 */

export type Lang = 'ru' | 'en' | 'kk';

export interface AskQa {
  /** Вопрос в поле поиска (печатается посимвольно) */
  q: string;
  /** Ответ */
  a: string;
  /** Спикер и время цитаты */
  w: string;
  /** Цитата */
  c: string;
  /** Строка «откуда ответ» */
  r: string;
  /** Цвет спикера — строка var(--spN), токен, не hex */
  col: string;
  /** Инициал аватара */
  ini: string;
}

export interface LandingCopy {
  meta: { title: string; description: string };
  nav: { features: string; download: string; faq: string; docs: string };
  hero: { title: string; lead: string; primary: string; secondary: string; note: string };
  card: {
    title: string;
    meta: string;
    rec: string;
    done: string;
    you: string;
    them: string;
    cap0: string;
    m1w: string;
    m1t: string;
    m1: string;
    m2w: string;
    m2t: string;
    m2: string;
    cap1: string;
    s1: string;
    s2: string;
    s3: string;
    cap2: string;
    q: string;
    a: string;
    quote: string;
    ref: string;
  };
  phases: [string, string, string, string];
  floats: { f0: string; f1: string; f2: string; f3: string; fixed: string };
  ask: { title: string; lead: string; button: string; disc: string; qa: AskQa[] };
  trust: {
    title: string;
    c1t: string;
    c1b: string;
    c2t: string;
    c2b: string;
    c3t: string;
    c3b: string;
    consent: string;
  };
  dl: {
    title: string;
    macOs: string;
    macD: string;
    macBtn: string;
    macReq: string;
    gkT: string;
    gkB: string;
    brewT: string;
    winTag: string;
    winOs: string;
    winD: string;
    mobTag: string;
    mobOs: string;
    mobD: string;
  };
  faq: { title: string; items: { q: string; a: string }[] };
  footer: {
    tagline: string;
    linksHeading: string;
    legalHeading: string;
    legal: { label: string; slug: string }[];
    license: string;
  };
}

export const REPO = 'https://github.com/zdllucky/wotold';

/** DMG «последний выпуск» — постоянная ссылка, сверена с docs/download.md */
export const DMG_URL = `${REPO}/releases/latest/download/Wotold-arm64.dmg`;

/** Ссылки колонки «Проект» — имена собственные, одинаковы во всех локалях */
export const PROJECT_LINKS = [
  { label: 'GitHub', href: REPO },
  { label: 'Issues', href: `${REPO}/issues` },
  { label: 'Discussions', href: `${REPO}/discussions` },
  { label: 'Releases', href: `${REPO}/releases` },
] as const;

const LEGAL_SLUGS = ['legal/privacy', 'legal/consent', 'legal/terms', 'legal/license'] as const;

/** Плейсхолдер внутри HTML-строк дека: заменяется на локальный URL consent. */
export const LEGAL_CONSENT_TOKEN = '%LEGAL_CONSENT%';

const legal = (labels: [string, string, string, string]) =>
  LEGAL_SLUGS.map((slug, i) => ({ label: labels[i], slug }));

export const LANDING: Record<Lang, LandingCopy> = {
  ru: {
    meta: {
      title: 'Wotold — записывает звонки и помнит, о чём договорились',
      description:
        'Wotold готовит расшифровку и краткие итоги каждого рабочего звонка, а затем отвечает на вопросы по записям — даже месяц спустя.',
    },
    nav: { features: 'Возможности', download: 'Скачать', faq: 'Вопросы', docs: 'Документация' },
    hero: {
      title: 'Записывает звонки. Помнит, о чём договорились.',
      lead: 'Wotold готовит расшифровку и краткие итоги каждого рабочего звонка, а затем отвечает на вопросы по записям — даже месяц спустя.',
      primary: 'Скачать для Mac',
      secondary: 'Как это работает',
      note: 'Бесплатно · без регистрации · записи хранятся только у вас',
    },
    card: {
      title: 'Звонок с подрядчиком',
      meta: 'сегодня, 14:00',
      rec: 'запись',
      done: 'готово',
      you: 'вы',
      them: 'собеседники',
      cap0: 'Запись идёт сама — вы просто разговариваете.',
      m1w: 'Марина',
      m1t: '14:02',
      m1: 'Смету пришлю до пятницы, тогда и поставим её в договор.',
      m2w: 'Вы',
      m2t: '14:02',
      m2: 'Хорошо, зафиксируем в протоколе.',
      cap1: 'Расшифровка готова: видно, кто и что сказал.',
      s1: '<b>Смета</b> — Марина пришлёт до пятницы',
      s2: '<b>Договор</b> — подписание на следующей неделе',
      s3: '<b>Начало работ</b> — со вторника',
      cap2: 'Краткие итоги: что решили и кто за что отвечает.',
      q: 'Кто обещал прислать смету?',
      a: 'Марина — до пятницы',
      quote: 'Смету пришлю до пятницы, тогда и поставим её в договор',
      ref: 'звонок с подрядчиком · <u>открыть этот момент</u>',
    },
    phases: ['Запись', 'Расшифровка', 'Итоги', 'Ответ'],
    floats: {
      f0: 'Записываются обе стороны разговора',
      f1: 'Wotold определяет, кто говорит',
      f2: 'Итоги — через несколько минут после звонка',
      f3: 'Ответ найден в записи трёхнедельной давности',
      fixed: 'Записи хранятся только на вашем компьютере',
    },
    ask: {
      title: 'Ответ найдётся даже месяц спустя',
      lead: 'Wotold ищет по всем записанным звонкам и отвечает со ссылкой на момент записи, из которого взят ответ.',
      button: 'Спросить',
      disc: 'Это пример. Настоящие ответы Wotold ищет только в ваших записях — и прямо сообщает, если ответа в них нет.',
      qa: [
        {
          q: 'Кто обещал прислать смету?',
          a: 'Марина — до пятницы',
          w: 'Марина · 14:02',
          c: '«Смету пришлю до пятницы, тогда и поставим её в договор»',
          r: 'звонок с подрядчиком, три недели назад · открыть этот момент',
          col: 'var(--sp3)',
          ini: 'М',
        },
        {
          q: 'На какое число перенесли запуск?',
          a: 'На 12 марта',
          w: 'Игорь · 11:20',
          c: '«Давайте финально: запуск двенадцатого, дальше не двигаем»',
          r: 'планёрка по запуску, прошлый понедельник · открыть этот момент',
          col: 'var(--sp5)',
          ini: 'И',
        },
        {
          q: 'Что просил поправить заказчик?',
          a: 'Убрать вторую страницу из отчёта',
          w: 'Сергей · 16:44',
          c: '«Вторая страница лишняя, уберите её из финальной версии»',
          r: 'звонок с заказчиком, вчера · открыть этот момент',
          col: 'var(--sp2)',
          ini: 'С',
        },
      ],
    },
    trust: {
      title: 'Запись звонков — дело деликатное',
      c1t: 'Данные остаются у вас',
      c1b: 'У Wotold нет своих серверов. Записи и расшифровки хранятся на вашем компьютере и никуда не передаются.',
      c2t: 'Запись включается только вручную',
      c2b: 'Wotold никогда не начинает запись сам — только по вашей команде. И останавливается так же.',
      c3t: 'Устройство программы открыто',
      c3b: 'Wotold бесплатен и распространяется с открытым исходным кодом. Как программа обращается с данными, может проверить любой специалист.',
      consent:
        'Во многих странах запись разговора требует согласия всех участников. Перед первым рабочим звонком прочитайте <a href="%LEGAL_CONSENT%">короткое уведомление о записи</a>.',
    },
    dl: {
      title: 'Скачать Wotold',
      macOs: 'Для Mac',
      macD: 'Скачайте образ, перетащите Wotold в «Программы» — и можно записывать первый звонок.',
      macBtn: 'Скачать для Mac',
      macReq:
        'Подойдёт Mac с процессором Apple (2020 года и новее) и macOS 14.4 или новее. При первой настройке программа один раз скачает языковые модели — от 2 до 7 ГБ.',
      gkT: 'Разрешение при первом запуске',
      gkB: 'macOS предупреждает о программах, установленных не из App Store. Откройте «Системные настройки → Конфиденциальность и безопасность» и нажмите «Открыть всё равно» — это понадобится один раз.',
      brewT: 'Установка через Homebrew',
      winTag: 'пока нет',
      winOs: 'Windows и Linux',
      winD: 'Запись звука опирается на возможности macOS, поэтому версий для других систем пока нет. О появлении сообщим на этой странице.',
      mobTag: 'в планах',
      mobOs: 'iPhone и Android',
      mobD: 'Планируем мобильное приложение, чтобы архив звонков был под рукой и в дороге. Анонс — на этой странице.',
    },
    faq: {
      title: 'Частые вопросы',
      items: [
        {
          q: 'Куда попадают записи?',
          a: 'Только на ваш компьютер. У проекта нет серверной части, поэтому содержимому звонков физически некуда передаваться. Программа обращается к интернету дважды: чтобы скачать языковые модели при первой настройке и чтобы проверить наличие новой версии. Ваших данных в этих запросах нет.',
        },
        {
          q: 'Сколько стоит Wotold?',
          a: 'Wotold бесплатен: без подписок и платных функций. Проект развивается открыто, исходный код опубликован на <a href="https://github.com/zdllucky/wotold">GitHub</a>.',
        },
        {
          q: 'Законно ли записывать звонки?',
          a: 'Зависит от страны: где-то достаточно согласия одного участника, где-то предупредить нужно всех. Wotold не уведомляет собеседников о записи — эта обязанность остаётся на вас. Прочитайте <a href="%LEGAL_CONSENT%">уведомление о записи</a> до первого рабочего звонка.',
        },
        {
          q: 'Почему расшифровка появляется не сразу?',
          a: 'Всю обработку выполняет ваш компьютер, а не удалённый сервер; обычно это занимает несколько минут после звонка. Длинные записи обрабатываются по частям, прогресс виден в карточке звонка. Взамен запись не покидает компьютер.',
        },
        {
          q: 'Может ли Wotold перепутать говорящих?',
          a: 'Может, поэтому программа не решает за вас: она предлагает вариант — «похоже, это Иван», — а закрепляете реплики вы. Исправление учитывается, и в следующий раз подсказка будет точнее.',
        },
        {
          q: 'Почему только для Mac?',
          a: 'Надёжная запись звука звонков построена на возможностях macOS. Версии для других систем появятся, только если их получится сделать такими же аккуратными.',
        },
      ],
    },
    footer: {
      tagline: 'Открытый проект: код, планы и обсуждения — на GitHub.',
      linksHeading: 'Проект',
      legalHeading: 'Правовая информация',
      legal: legal(['Конфиденциальность', 'Уведомление о записи', 'Условия использования', 'Лицензия']),
      license: 'Исходный код — под лицензией Apache 2.0.',
    },
  },
  en: {
    meta: {
      title: 'Wotold — records your calls and remembers what was agreed',
      description:
        'Wotold prepares a transcript and a short recap of every work call, then answers questions about your recordings — even a month later.',
    },
    nav: { features: 'Features', download: 'Download', faq: 'FAQ', docs: 'Docs' },
    hero: {
      title: 'Records your calls. Remembers what was agreed.',
      lead: 'Wotold prepares a transcript and a short recap of every work call, then answers questions about your recordings — even a month later.',
      primary: 'Download for Mac',
      secondary: 'See how it works',
      note: 'Free · no sign-up · recordings stay with you',
    },
    card: {
      title: 'Call with the contractor',
      meta: 'today, 2:00 pm',
      rec: 'recording',
      done: 'done',
      you: 'you',
      them: 'others',
      cap0: 'Recording runs by itself — you just talk.',
      m1w: 'Marina',
      m1t: '2:02 pm',
      m1: 'I’ll send the estimate by Friday, and we’ll put it into the contract.',
      m2w: 'You',
      m2t: '2:02 pm',
      m2: 'Good — let’s note that in the minutes.',
      cap1: 'The transcript is ready: you can see who said what.',
      s1: '<b>Estimate</b> — Marina sends it by Friday',
      s2: '<b>Contract</b> — signing next week',
      s3: '<b>Work starts</b> — Tuesday',
      cap2: 'A short recap: what was decided and who owns what.',
      q: 'Who promised to send the estimate?',
      a: 'Marina — by Friday',
      quote: 'I’ll send the estimate by Friday, and we’ll put it into the contract',
      ref: 'call with the contractor · <u>open this moment</u>',
    },
    phases: ['Recording', 'Transcript', 'Recap', 'Answer'],
    floats: {
      f0: 'Both sides of the call are recorded',
      f1: 'Wotold tells the speakers apart',
      f2: 'The recap is ready minutes after the call',
      f3: 'Found in a recording from three weeks ago',
      fixed: 'Recordings stay on your computer only',
    },
    ask: {
      title: 'Answers turn up even a month later',
      lead: 'Wotold searches every recorded call and answers with a link to the exact moment the answer comes from.',
      button: 'Ask',
      disc: 'This is a demo. Real answers come only from your own recordings — and Wotold says so plainly when they hold none.',
      qa: [
        {
          q: 'Who promised to send the estimate?',
          a: 'Marina — by Friday',
          w: 'Marina · 2:02 pm',
          c: '“I’ll send the estimate by Friday, and we’ll put it into the contract”',
          r: 'call with the contractor, three weeks ago · open this moment',
          col: 'var(--sp3)',
          ini: 'M',
        },
        {
          q: 'What date did the launch move to?',
          a: 'March 12',
          w: 'Igor · 11:20 am',
          c: '“Let’s make it final: we launch on the twelfth, no more moving”',
          r: 'launch planning call, last Monday · open this moment',
          col: 'var(--sp5)',
          ini: 'I',
        },
        {
          q: 'What did the client ask to fix?',
          a: 'Remove the second page from the report',
          w: 'Sergey · 4:44 pm',
          c: '“The second page is unnecessary — take it out of the final version”',
          r: 'client call, yesterday · open this moment',
          col: 'var(--sp2)',
          ini: 'S',
        },
      ],
    },
    trust: {
      title: 'Recording calls is a delicate matter',
      c1t: 'Your data stays with you',
      c1b: 'Wotold has no servers of its own. Recordings and transcripts live on your computer and are never sent anywhere.',
      c2t: 'Recording starts only by hand',
      c2b: 'Wotold never starts a recording on its own — only on your command. It stops the same way.',
      c3t: 'The inner workings are open',
      c3b: 'Wotold is free and open source. Anyone qualified can inspect exactly how it handles your data.',
      consent:
        'In many countries, recording a conversation requires everyone’s consent. Read the short <a href="%LEGAL_CONSENT%">recording notice</a> before your first work call.',
    },
    dl: {
      title: 'Download Wotold',
      macOs: 'For Mac',
      macD: 'Download the image, drag Wotold into Applications — and you are ready to record your first call.',
      macBtn: 'Download for Mac',
      macReq:
        'You’ll need a Mac with an Apple processor (2020 or newer) running macOS 14.4 or later. On first setup the app downloads language models once — 2 to 7 GB.',
      gkT: 'Approval on first launch',
      gkB: 'macOS warns about apps installed outside the App Store. Open System Settings → Privacy & Security and click “Open Anyway” — needed once.',
      brewT: 'Install via Homebrew',
      winTag: 'not yet',
      winOs: 'Windows & Linux',
      winD: 'Sound capture relies on macOS capabilities, so there are no versions for other systems yet. Any change will be announced on this page.',
      mobTag: 'planned',
      mobOs: 'iPhone & Android',
      mobD: 'A mobile app is planned, so your call archive is at hand on the go. The announcement will appear on this page.',
    },
    faq: {
      title: 'Frequently asked questions',
      items: [
        {
          q: 'Where do the recordings go?',
          a: 'Only onto your computer. The project has no server side, so there is physically nowhere for call content to be sent. The app reaches the internet twice: to download language models on first setup and to check for a new version. Neither request carries your data.',
        },
        {
          q: 'How much does Wotold cost?',
          a: 'Wotold is free: no subscriptions, no paid tiers. The project is developed in the open — the source code is on <a href="https://github.com/zdllucky/wotold">GitHub</a>.',
        },
        {
          q: 'Is it legal to record calls?',
          a: 'It depends on the country: some require one party’s consent, others require everyone’s. Wotold does not notify the other side — that duty stays with you. Read the <a href="%LEGAL_CONSENT%">recording notice</a> before your first work call.',
        },
        {
          q: 'Why isn’t the transcript instant?',
          a: 'All processing happens on your computer, not on a remote server; it usually takes a few minutes after the call. Long recordings are processed in parts, with progress visible on the call card. In return, the recording never leaves your machine.',
        },
        {
          q: 'Can Wotold mix up the speakers?',
          a: 'It can — which is why it never decides for you: it suggests “this might be Ivan”, and you confirm. Corrections are remembered, so the next suggestion is more accurate.',
        },
        {
          q: 'Why Mac only?',
          a: 'Reliable call audio capture is built on macOS capabilities. Versions for other systems will appear only if they can be made just as solid.',
        },
      ],
    },
    footer: {
      tagline: 'An open project: code, plans and discussions live on GitHub.',
      linksHeading: 'Project',
      legalHeading: 'Legal',
      legal: legal(['Privacy policy', 'Recording notice', 'Terms of use', 'License']),
      license: 'Source code is licensed under Apache 2.0.',
    },
  },
  kk: {
    meta: {
      title: 'Wotold — қоңырауларды жазады және не келісілгенін есте сақтайды',
      description:
        'Wotold әр жұмыс қоңырауының транскрипциясы мен қысқа қорытындысын дайындайды, содан кейін жазбалар бойынша сұрақтарға жауап береді — тіпті бір айдан кейін де.',
    },
    nav: { features: 'Мүмкіндіктер', download: 'Жүктеу', faq: 'Сұрақтар', docs: 'Құжаттама' },
    hero: {
      title: 'Қоңырауларды жазады. Не келісілгенін есте сақтайды.',
      lead: 'Wotold әр жұмыс қоңырауының транскрипциясы мен қысқа қорытындысын дайындайды, содан кейін жазбалар бойынша сұрақтарға жауап береді — тіпті бір айдан кейін де.',
      primary: 'Mac үшін жүктеу',
      secondary: 'Қалай жұмыс істейді',
      note: 'Тегін · тіркелусіз · жазбалар тек сізде сақталады',
    },
    card: {
      title: 'Мердігермен қоңырау',
      meta: 'бүгін, 14:00',
      rec: 'жазу',
      done: 'дайын',
      you: 'сіз',
      them: 'әңгімелесушілер',
      cap0: 'Жазу өздігінен жүреді — сіз жай сөйлесесіз.',
      m1w: 'Марина',
      m1t: '14:02',
      m1: 'Сметаны жұмаға дейін жіберемін, содан кейін келісімшартқа енгіземіз.',
      m2w: 'Сіз',
      m2t: '14:02',
      m2: 'Жақсы, хаттамаға тіркейміз.',
      cap1: 'Транскрипция дайын: кім не айтқаны көрініп тұр.',
      s1: '<b>Смета</b> — Марина жұмаға дейін жібереді',
      s2: '<b>Келісімшарт</b> — келесі аптада қол қою',
      s3: '<b>Жұмыс басы</b> — сейсенбіден',
      cap2: 'Қысқа қорытынды: не шешілді және кім не істейді.',
      q: 'Сметаны жіберуге кім уәде берді?',
      a: 'Марина — жұмаға дейін',
      quote: 'Сметаны жұмаға дейін жіберемін, содан кейін келісімшартқа енгіземіз',
      ref: 'мердігермен қоңырау · <u>осы сәтті ашу</u>',
    },
    phases: ['Жазу', 'Транскрипция', 'Қорытынды', 'Жауап'],
    floats: {
      f0: 'Әңгіменің екі жағы да жазылады',
      f1: 'Wotold кім сөйлеп тұрғанын анықтайды',
      f2: 'Қорытынды — қоңыраудан кейін бірнеше минутта',
      f3: 'Жауап үш апта бұрынғы жазбадан табылды',
      fixed: 'Жазбалар тек сіздің компьютеріңізде сақталады',
    },
    ask: {
      title: 'Жауап бір айдан кейін де табылады',
      lead: 'Wotold барлық жазылған қоңыраулар бойынша іздейді және жауап алынған жазба сәтіне сілтеме береді.',
      button: 'Сұрау',
      disc: 'Бұл — мысал. Нақты жауаптарды Wotold тек сіздің жазбаларыңыздан іздейді, ал жауап болмаса — тікелей айтады.',
      qa: [
        {
          q: 'Сметаны жіберуге кім уәде берді?',
          a: 'Марина — жұмаға дейін',
          w: 'Марина · 14:02',
          c: '«Сметаны жұмаға дейін жіберемін, содан кейін келісімшартқа енгіземіз»',
          r: 'мердігермен қоңырау, үш апта бұрын · осы сәтті ашу',
          col: 'var(--sp3)',
          ini: 'М',
        },
        {
          q: 'Іске қосу қай күнге ауыстырылды?',
          a: '12 наурызға',
          w: 'Игорь · 11:20',
          c: '«Соңғы шешім: іске қосу он екісінде, енді жылжытпаймыз»',
          r: 'іске қосу жиналысы, өткен дүйсенбі · осы сәтті ашу',
          col: 'var(--sp5)',
          ini: 'И',
        },
        {
          q: 'Тапсырыс беруші нені түзетуді сұрады?',
          a: 'Есептен екінші бетті алып тастау',
          w: 'Сергей · 16:44',
          c: '«Екінші бет артық, соңғы нұсқадан алып тастаңыз»',
          r: 'тапсырыс берушімен қоңырау, кеше · осы сәтті ашу',
          col: 'var(--sp2)',
          ini: 'С',
        },
      ],
    },
    trust: {
      title: 'Қоңырауды жазу — нәзік мәселе',
      c1t: 'Деректер сізде қалады',
      c1b: 'Wotold-тың өз серверлері жоқ. Жазбалар мен транскрипциялар сіздің компьютеріңізде сақталады және еш жерге жіберілмейді.',
      c2t: 'Жазу тек қолмен қосылады',
      c2b: 'Wotold жазуды ешқашан өздігінен бастамайды — тек сіздің әміріңізбен. Тоқтауы да солай.',
      c3t: 'Бағдарламаның құрылысы ашық',
      c3b: 'Wotold тегін және ашық бастапқы кодпен таратылады. Деректермен қалай жұмыс істейтінін кез келген маман тексере алады.',
      consent:
        'Көптеген елдерде әңгімені жазу барлық қатысушының келісімін талап етеді. Алғашқы жұмыс қоңырауына дейін <a href="%LEGAL_CONSENT%">жазба туралы қысқа хабарламаны</a> оқыңыз.',
    },
    dl: {
      title: 'Wotold жүктеу',
      macOs: 'Mac үшін',
      macD: 'Образды жүктеп, Wotold-ты «Программалар» қалтасына сүйреңіз — алғашқы қоңырауды жазуға болады.',
      macBtn: 'Mac үшін жүктеу',
      macReq:
        'Apple процессоры бар Mac (2020 жыл және жаңарақ) және macOS 14.4+ қажет. Алғашқы баптауда бағдарлама тілдік модельдерді бір рет жүктейді — 2–7 ГБ.',
      gkT: 'Алғашқы іске қосудағы рұқсат',
      gkB: 'macOS App Store-дан тыс орнатылған бағдарламалар туралы ескертеді. «Жүйе баптаулары → Құпиялылық және қауіпсіздік» ашып, «Бәрібір ашу» басыңыз — бір рет қана қажет.',
      brewT: 'Homebrew арқылы орнату',
      winTag: 'әзірге жоқ',
      winOs: 'Windows және Linux',
      winD: 'Дыбыс жазу macOS мүмкіндіктеріне сүйенеді, сондықтан басқа жүйелерге нұсқалар әзірге жоқ. Өзгеріс болса — осы бетте хабарлаймыз.',
      mobTag: 'жоспарда',
      mobOs: 'iPhone және Android',
      mobD: 'Қоңыраулар архиві жолда да қолжетімді болуы үшін мобильді қосымша жоспарлануда. Хабарландыру осы бетте шығады.',
    },
    faq: {
      title: 'Жиі қойылатын сұрақтар',
      items: [
        {
          q: 'Жазбалар қайда сақталады?',
          a: 'Тек сіздің компьютеріңізде. Жобада сервер бөлігі жоқ, сондықтан қоңырау мазмұнының жіберілетін жері де жоқ. Бағдарлама интернетке екі рет қана жүгінеді: алғашқы баптауда модельдерді жүктеу және жаңа нұсқаны тексеру үшін. Бұл сұрауларда сіздің деректеріңіз жоқ.',
        },
        {
          q: 'Wotold қанша тұрады?',
          a: 'Wotold тегін: жазылым да, ақылы функциялар да жоқ. Жоба ашық дамиды, бастапқы код <a href="https://github.com/zdllucky/wotold">GitHub-та</a> жарияланған.',
        },
        {
          q: 'Қоңырауды жазу заңды ма?',
          a: 'Елге байланысты: кейде бір қатысушының келісімі жеткілікті, кейде бәрін ескерту қажет. Wotold әңгімелесушілерге жазу туралы хабарламайды — бұл міндет сізде. Алғашқы жұмыс қоңырауына дейін <a href="%LEGAL_CONSENT%">жазба туралы хабарламаны</a> оқыңыз.',
        },
        {
          q: 'Неге транскрипция бірден шықпайды?',
          a: 'Барлық өңдеуді қашықтағы сервер емес, сіздің компьютеріңіз орындайды; әдетте бұл қоңыраудан кейін бірнеше минут алады. Ұзын жазбалар бөліктермен өңделеді, барысы қоңырау картасында көрінеді. Есесіне жазба компьютерден шықпайды.',
        },
        {
          q: 'Wotold сөйлеушілерді шатастыруы мүмкін бе?',
          a: 'Мүмкін, сондықтан бағдарлама сіз үшін шешпейді: ол «бұл Иван болуы мүмкін» деп ұсынады, ал бекітетін — сіз. Түзету ескеріледі, келесі жолы ұсыныс дәлірек болады.',
        },
        {
          q: 'Неге тек Mac үшін?',
          a: 'Қоңырау дыбысын сенімді жазу macOS мүмкіндіктеріне құрылған. Басқа жүйелерге нұсқалар тек соншалықты мұқият жасалғанда ғана шығады.',
        },
      ],
    },
    footer: {
      tagline: 'Ашық жоба: код, жоспарлар мен талқылаулар — GitHub-та.',
      linksHeading: 'Жоба',
      legalHeading: 'Құқықтық ақпарат',
      legal: legal(['Құпиялылық', 'Жазба туралы хабарлама', 'Пайдалану шарттары', 'Лицензия']),
      license: 'Бастапқы код Apache 2.0 лицензиясымен таратылады.',
    },
  },
};
