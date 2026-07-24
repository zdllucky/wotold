/* eslint-disable */
// WOTOLD · data model for the v2 product build.
const WK_SPEAKERS = {
  you:   { name: 'Вы',              color: 'var(--sp1)' },
  arman: { name: 'Арман Сулейменов', color: 'var(--sp2)' },
  lena:  { name: 'Елена Ковач',      color: 'var(--sp3)' },
  dmitr: { name: 'Дмитрий Петров',   color: 'var(--sp4)' },
  guest: { name: 'Гость',            color: 'var(--sp5)' },
};
const av = (k) => ({ name: WK_SPEAKERS[k].name, color: WK_SPEAKERS[k].color });

const WK_ENGINES = {
  local: { id: 'local', label: 'На устройстве', sub: 'сбалансированный', icon: 'cpu',   tone: 'ok',     facts: ['офлайн', 'без отправки звука'] },
  cloud: { id: 'cloud', label: 'Облако Wotold',  sub: 'высокая точность', icon: 'cloud', tone: 'accent', facts: ['через прокси Wotold', 'квота 60 мин/день'] },
  byo:   { id: 'byo',   label: 'Свои ключи',     sub: 'ваши провайдеры',  icon: 'key',   tone: 'neutral',facts: ['без лимитов', 'оплата напрямую'] },
};

const WK_TRANSCRIPT = [
  { sp: 'you',   t: 0,   text: 'Давайте сверимся по срокам пилота. На той неделе демо для совета директоров — хочу понимать, что мы точно успеваем.' },
  { sp: 'arman', t: 11,  text: 'По нашей части всё в графике. Интеграция с биллингом закрыта вчера, сейчас гоняем нагрузочные. Отчёт будет к пятнице.' },
  { sp: 'lena',  t: 26,  text: 'У меня вопрос по разделению голосов — на длинных звонках качество проседает. Для юридического отдела это критично.' },
  { sp: 'you',   t: 39,  text: 'Понял. Дмитрий, возьмёшь на себя? Нужно понять, упираемся мы в модель или в препроцессинг.' },
  { sp: 'dmitr', t: 48,  text: 'Возьму. Предварительно — это окно сегментации. Подкручу пороги и прогоню на трёх длинных записях. Результат к среде.' },
  { sp: 'arman', t: 62,  text: 'И давайте зафиксируем: на демо показываем именно локальный режим. Совет будет спрашивать про приватность данных.' },
  { sp: 'you',   t: 74,  text: 'Согласен, это главный аргумент. Звук не покидает устройство — надо проговорить прямым текстом.' },
  { sp: 'lena',  t: 85,  text: 'Тогда подготовлю один слайд про обработку на устройстве, без жаргона. Покажу до четверга на ревью.' },
];

const WK_RECAP = {
  summary: 'Синхрон по пилоту перед демо для совета директоров. Интеграция с биллингом закрыта, идут нагрузочные. Выявлена проблема разделения голосов на длинных звонках — взята в работу. Решено делать акцент на локальной обработке как на главном аргументе по приватности.',
  decisions: [
    'На демо показываем локальный режим — приватность как ключевой тезис.',
    'Нагрузочные тесты биллинга — отчёт к пятнице.',
  ],
  actions: [
    { who: 'dmitr', text: 'Разобраться с разделением голосов на длинных звонках (окно сегментации)', due: 'среда', done: false },
    { who: 'lena',  text: 'Слайд про обработку на устройстве, без жаргона', due: 'четверг', done: false },
    { who: 'arman', text: 'Финализировать отчёт по нагрузочным', due: 'пятница', done: true },
  ],
  topics: ['пилот', 'демо совета', 'разделение голосов', 'приватность', 'биллинг'],
};

// suggested questions for the "Ask" composer (ChatGPT element)
const WK_ASK = [
  { q: 'Какие задачи на мне?', a: 'На вас прямых задач из этого звонка нет — вы их распределили. Открытые: Дмитрий — разделение голосов (до среды), Елена — слайд про локальную обработку (до четверга).' },
  { q: 'О чём договорились?', a: 'Два решения: 1) на демо показываем локальный режим и делаем акцент на приватности; 2) отчёт по нагрузочным тестам биллинга — к пятнице.' },
  { q: 'Кто отвечает за приватность?', a: 'Тезис про приватность ведёт Арман (поднял вопрос совета), а слайд про обработку на устройстве готовит Елена — до четверга.' },
];

const WK_CALLS = [
  { id: 'c1', title: 'Синхрон по пилоту',          parts: ['you','arman','lena','dmitr'], dur: 1480, when: '2026-06-21T10:12:00', status: 'ready',      via: 'local', recap: true },
  { id: 'c2', title: 'Звонок с юридическим отделом', parts: ['you','lena'],                 dur: 920,  when: '2026-06-21T08:40:00', status: 'processing', via: 'cloud', stage: 2 },
  { id: 'c3', title: 'Демо для «Контур»',           parts: ['you','guest','arman'],        dur: 2710, when: '2026-06-20T16:05:00', status: 'ready',      via: 'cloud', recap: true },
  { id: 'c4', title: 'Онбординг подрядчика',        parts: ['you','dmitr'],                dur: 1180, when: '2026-06-20T11:20:00', status: 'error',      via: 'local', err: 'broken' },
  { id: 'c5', title: 'Планёрка продукта',           parts: ['you','arman','lena'],         dur: 2030, when: '2026-06-18T09:30:00', status: 'ready',      via: 'local', recap: true },
  { id: 'c6', title: 'Интервью с кандидатом',       parts: ['you','guest'],                dur: 2440, when: '2026-06-17T14:00:00', status: 'ready',      via: 'byo',   recap: false },
  { id: 'c7', title: 'Ретро спринта 24',            parts: ['you','arman','dmitr','lena'], dur: 1990, when: '2026-06-16T17:30:00', status: 'ready',      via: 'local', recap: true },
];

const WK_CONTACTS = [
  { id: 'k1', name: 'Арман Сулейменов', role: 'CTO · Контур',         sp: 'arman', calls: 14, last: '2026-06-21', tags: ['партнёр','биллинг'], confirmed: true },
  { id: 'k2', name: 'Елена Ковач',       role: 'Юридический отдел',    sp: 'lena',  calls: 9,  last: '2026-06-21', tags: ['юристы','комплаенс'], confirmed: true },
  { id: 'k3', name: 'Дмитрий Петров',    role: 'ML-инженер',           sp: 'dmitr', calls: 22, last: '2026-06-20', tags: ['команда'], confirmed: true },
  { id: 'k4', name: 'Гость',             role: 'голос не подтверждён', sp: 'guest', calls: 3,  last: '2026-06-17', tags: ['нераспознан'], confirmed: false },
];

const WK_STAGES = [
  { icon: 'mic',      label: 'Распознавание речи' },
  { icon: 'users',    label: 'Разделение голосов' },
  { icon: 'scissors', label: 'Сборка дорожек' },
  { icon: 'doc',      label: 'Формирование рекапа' },
];

// formatters
function fmtDur(sec) { const m = Math.floor(sec / 60), s = sec % 60; return `${m}:${String(s).padStart(2, '0')}`; }
function fmtDurLong(sec) { const m = Math.floor(sec / 60); return `${m} мин`; }
function fmtClock(sec) { const m = Math.floor(sec / 60), s = sec % 60; return `${m}:${String(s).padStart(2, '0')}`; }
function fmtTime(iso) { return new Date(iso).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }); }
function fmtDay(iso) {
  const d = new Date(iso); const today = new Date('2026-06-21');
  const diff = Math.round((today - d) / 86400000);
  if (diff <= 0) return 'Сегодня';
  if (diff === 1) return 'Вчера';
  if (diff < 7) return `${diff} дн. назад`;
  return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
}
function relDay(iso) {
  const d = new Date(iso); const today = new Date('2026-06-21');
  const diff = Math.round((today - d) / 86400000);
  if (diff <= 0) return 'Сегодня';
  if (diff === 1) return 'Вчера';
  if (diff < 7) return 'На этой неделе';
  return 'Ранее';
}

Object.assign(window, {
  WK_SPEAKERS, av, WK_ENGINES, WK_TRANSCRIPT, WK_RECAP, WK_ASK, WK_CALLS, WK_CONTACTS, WK_STAGES,
  fmtDur, fmtDurLong, fmtClock, fmtTime, fmtDay, relDay,
});
