/* eslint-disable */
// WOTOLD · Ассистент — RAG-поиск по звонкам: банк ответов, движок, общий раздел (чаты)
// Механика честная: классификация запроса → поиск → фрагменты в контекст (окно 8K) → ответ с источниками.
const { useState: uSA, useRef: uRA } = React;

// ── Индекс поиска: только готовые расшифровки ──
const AS_INDEX = WK_CALLS.filter((c) => c.status === 'ready');
const AS_STATS = { calls: AS_INDEX.length, total: WK_CALLS.length, dur: AS_INDEX.reduce((s, c) => s + c.dur, 0) };
function fmtH(sec) { const h = Math.floor(sec / 3600), m = Math.round((sec % 3600) / 60); return h ? `${h} ч ${m} мин` : `${m} мин`; }

// генерация текстов — вне области (RAG: поиск и разбор, не создание)
const AS_GEN_RE = /напиш|состав|сгенерир|придума|перевед|отправ|создай|запланируй|оформи|нарисуй/i;

// ── Фрагменты (что реально попадает в контекст) ──
const FR = {
  c1_dmitr: { call: 'c1', sp: 'dmitr', t: 48, text: 'Возьму. Предварительно — это окно сегментации. Подкручу пороги и прогоню на трёх длинных записях. Результат к среде.' },
  c1_you39: { call: 'c1', sp: 'you', t: 39, text: 'Понял. Дмитрий, возьмёшь на себя? Нужно понять, упираемся мы в модель или в препроцессинг.' },
  c1_arman62: { call: 'c1', sp: 'arman', t: 62, text: 'И давайте зафиксируем: на демо показываем именно локальный режим. Совет будет спрашивать про приватность данных.' },
  c1_you74: { call: 'c1', sp: 'you', t: 74, text: 'Согласен, это главный аргумент. Звук не покидает устройство — надо проговорить прямым текстом.' },
  c1_lena85: { call: 'c1', sp: 'lena', t: 85, text: 'Тогда подготовлю один слайд про обработку на устройстве, без жаргона. Покажу до четверга на ревью.' },
  c5_lena: { call: 'c5', sp: 'lena', t: 540, text: 'Юристы пилота жалуются: на созвонах дольше сорока минут говорящие начинают путаться. Предлагаю поднять приоритет.' },
  c5_dmitr: { call: 'c5', sp: 'dmitr', t: 812, text: 'Черновик схемы хранения записей принесу на следующую планёрку — с вариантом шифрования на устройстве.' },
  c7_dmitr: { call: 'c7', sp: 'dmitr', t: 1274, text: 'В риски записываю разделение голосов: на длинных записях границы сегментов плывут. Беру стенд для нагрузочных на себя, к четвергу.' },
  c3_guest: { call: 'c3', sp: 'guest', t: 310, text: 'Нам важно, чтобы звук не уходил из контура компании — это условие службы безопасности.' },
  c3_arman: { call: 'c3', sp: 'arman', t: 1150, text: 'Фиксируем: пилот на двадцать рабочих мест, стартуем после демо совета. Доступы согласую со стороны «Контура».' },
};

// ── Банк ответов: общий поиск ──
const AS_QA = [
  { re: /дмитр/i, ans: {
    text: 'По расшифровкам за неделю у Дмитрия три открытые задачи:\n1. Разделение голосов на длинных звонках — проверить окно сегментации, срок среда.\n2. Стенд для нагрузочных тестов — к четвергу.\n3. Черновик схемы хранения записей — к следующей планёрке.',
    srcs: [{ call: 'c1', t: 48 }, { call: 'c7', t: 1274 }, { call: 'c5', t: 812 }],
    frags: [FR.c1_dmitr, FR.c7_dmitr, FR.c5_dmitr], tok: '≈1.9K' } },
  { re: /приватн|локальн/i, ans: {
    text: 'Приватность обсуждалась в двух звонках.\n«Синхрон по пилоту» (сегодня): на демо совета показываем локальный режим, тезис «звук не покидает устройство»; Елена готовит слайд без жаргона к четвергу.\n«Планёрка продукта» (18 июня): требование юристов пилота — обработка на устройстве.',
    srcs: [{ call: 'c1', t: 62 }, { call: 'c1', t: 85 }, { call: 'c5', t: 540 }],
    frags: [FR.c1_arman62, FR.c1_lena85, FR.c5_lena], tok: '≈1.9K' } },
  { re: /контур/i, ans: {
    text: 'По звонку «Демо для «Контур»» (20 июня): пилот на 20 рабочих мест, старт после демо совета; условие службы безопасности — звук не покидает контур компании; доступы со стороны «Контура» согласует Арман.',
    srcs: [{ call: 'c3', t: 310 }, { call: 'c3', t: 1150 }],
    frags: [FR.c3_guest, FR.c3_arman], tok: '≈1.4K' } },
  { re: /планёрк/i, ans: {
    text: 'Планёрка продукта (18 июня), решения: 1) приоритет — разделение голосов на длинных звонках (жалобы юристов пилота); 2) черновик схемы хранения записей готовит Дмитрий к следующей планёрке.',
    srcs: [{ call: 'c5', t: 540 }, { call: 'c5', t: 812 }],
    frags: [FR.c5_lena, FR.c5_dmitr], tok: '≈1.4K' } },
  { re: /решени|договорил/i, ans: {
    text: 'Последний звонок — «Синхрон по пилоту» (сегодня). Решения: 1) на демо совета показываем локальный режим, приватность — ключевой тезис; 2) отчёт по нагрузочным тестам биллинга — к пятнице.',
    srcs: [{ call: 'c1', t: 62 }, { call: 'c1', t: 74 }],
    frags: [FR.c1_arman62, FR.c1_you74], tok: '≈1.4K' } },
];

// ── Банк ответов: внутри звонка (c1 как образец) ──
const WK_ASK2 = [
  { q: 'Какие задачи на мне?', re: /задач|на мне/i, ans: {
    text: 'На вас прямых задач нет — вы их распределили. Открытые: Дмитрий — разделение голосов (до среды), Елена — слайд про локальную обработку (до четверга).',
    srcs: [{ call: 'c1', t: 39 }, { call: 'c1', t: 48 }],
    frags: [FR.c1_you39, FR.c1_dmitr], tok: '≈1.4K' } },
  { q: 'О чём договорились?', re: /договор|решени/i, ans: {
    text: 'Два решения: 1) на демо показываем локальный режим и делаем акцент на приватности; 2) отчёт по нагрузочным тестам биллинга — к пятнице.',
    srcs: [{ call: 'c1', t: 62 }, { call: 'c1', t: 74 }],
    frags: [FR.c1_arman62, FR.c1_you74], tok: '≈1.4K' } },
  { q: 'Обсуждали разделение голосов раньше?', re: /раньше|прошл|друг(ой|их|ие)/i, ans: {
    text: 'Да, дважды. «Планёрка продукта» (18 июня): жалобы юристов пилота на длинные созвоны, тему подняли в приоритет. «Ретро спринта 24» (16 июня): вопрос отмечен в рисках спринта.',
    srcs: [{ call: 'c5', t: 540 }, { call: 'c7', t: 1274 }],
    frags: [FR.c5_lena, FR.c7_dmitr], tok: '≈1.4K' } },
];

// ── Движок ответа ──
function WK_AS_ANSWER(q, scope) {
  if (AS_GEN_RE.test(q)) return { kind: 'refusal', q,
    text: 'Составление текстов — вне области ассистента. Область: поиск и разбор информации в записанных звонках. Могу собрать факты — решения, задачи, сроки.' };
  const bank = scope === 'call' ? WK_ASK2 : AS_QA;
  const hit = bank.find((e) => e.re.test(q));
  if (hit) return { kind: 'answer', q, ...hit.ans };
  return scope === 'call'
    ? { kind: 'empty', q, escalate: true, text: 'В этом звонке этого не нашлось.', frags: [], tok: '≈0.3K' }
    : { kind: 'empty', q, text: 'По звонкам ничего не найдено. Уточните имя участника, тему или период.', frags: [], tok: '≈0.3K' };
}

// ── Сообщение-ответ (общее для обоих ассистентов) ──
function AnswerMsg({ ans, callId, onOpenCall, onSeek, onAskGlobal }) {
  const byCall = (id) => WK_CALLS.find((c) => c.id === id);
  const [copied, setCopied] = uSA(false);
  const srcLine = () => (ans.srcs || []).map((s) => { const c = byCall(s.call); return c.title + (s.t != null ? ' · ' + fmtClock(s.t) : ''); }).join('; ');
  const doCopy = (text) => { try { navigator.clipboard.writeText(text).catch(() => {}); } catch (e) {} setCopied(true); setTimeout(() => setCopied(false), 1400); };
  return (
    <div className="ask-bubble">
      {ans.kind === 'refusal' && <div className="ask-note"><Icon name="shield" size={13} />Вне области ассистента</div>}
      <div style={{ whiteSpace: 'pre-line' }}>{ans.text}</div>
      {ans.srcs && ans.srcs.length > 0 && (
        <div className="src-row">
          {ans.srcs.map((s, i) => {
            const c = byCall(s.call); const local = callId && s.call === callId;
            return (
              <Chip key={i} size="sm" tone="line" icon={local ? 'clock' : 'doc'}
                onClick={local && onSeek ? () => onSeek(s.t) : (!local && onOpenCall ? () => onOpenCall(s.call) : undefined)}>
                {local ? fmtClock(s.t) : c.title + (s.t != null ? ' · ' + fmtClock(s.t) : '')}
              </Chip>
            );
          })}
        </div>
      )}
      {ans.escalate && onAskGlobal && (
        <div className="src-row"><Chip size="sm" tone="accent" icon="search" onClick={() => onAskGlobal(ans.q)}>Искать во всех звонках</Chip></div>
      )}
      {ans.kind !== 'refusal' && (
        <details className="ctx">
          <summary><Icon name="chevronRight" size={11} className="ctx-arr" />Контекст поиска</summary>
          {(ans.frags || []).map((f, i) => (
            <div className="frag" key={i}>
              <b style={{ color: WK_SPEAKERS[f.sp].color }}>{WK_SPEAKERS[f.sp].name}</b>
              <span className="u-faint"> · {byCall(f.call).title} · {fmtClock(f.t)}</span>
              <br />{f.text}
            </div>
          ))}
          <div className="ctx-meta mono">фрагментов: {(ans.frags || []).length} · {ans.tok} токенов · окно 8K</div>
        </details>
      )}
      {ans.kind === 'answer' && (
        <div className="ans-acts">
          <IconBtn icon={copied ? 'check' : 'copy'} size="sm" label="Скопировать ответ" tip={copied ? 'Скопировано' : 'Скопировать'} onClick={() => doCopy(ans.text)} />
          <Dropdown align="left" width={238} trigger={({ toggle }) => <IconBtn icon="send" size="sm" label="Поделиться" tip="Поделиться" onClick={toggle} />}>
            <MenuItem icon="copy" onClick={() => doCopy(ans.text + '\n\nИсточники: ' + srcLine())}>Скопировать с источниками</MenuItem>
            <MenuItem icon="external">Отправить в почту…</MenuItem>
          </Dropdown>
        </div>
      )}
    </div>
  );
}

// ── Хранилище чатов (живёт между переключениями видов) ──
const WK_AS_STORE = {
  seq: 2, active: null,
  chats: [
    { id: 'a1', title: 'Задачи Дмитрия за неделю', day: 'Сегодня', msgs: [
      { me: true, text: 'Какие задачи взял Дмитрий на этой неделе?' },
      { me: false, ans: { kind: 'answer', ...AS_QA[0].ans } },
    ] },
    { id: 'a2', title: 'Письмо Арману по итогам', day: 'Вчера', msgs: [
      { me: true, text: 'Напиши фоллоу-ап письмо Арману по итогам синхрона' },
      { me: false, ans: WK_AS_ANSWER('Напиши письмо', 'global') },
      { me: true, text: 'Тогда собери решения из последнего звонка с ним' },
      { me: false, ans: { kind: 'answer', ...AS_QA[4].ans } },
    ] },
  ],
};
function truncQ(q, n) { return q.length > n ? q.slice(0, n - 1).trimEnd() + '…' : q; }
function WK_AS_NEWCHAT(q) {
  const id = 'a' + (++WK_AS_STORE.seq);
  WK_AS_STORE.chats.unshift({ id, title: truncQ(q, 42), day: 'Сегодня', msgs: [{ me: true, text: q }, { me: false, ans: WK_AS_ANSWER(q, 'global') }] });
  WK_AS_STORE.active = id;
  return id;
}

const AS_SUGGEST = ['Когда обсуждали приватность?', 'Все задачи Дмитрия за неделю', 'Что обещали «Контуру» на демо?', 'Решения планёрки продукта'];

// ── Раздел «Ассистент» ──
function AssistantView({ onOpenCall }) {
  const [, force] = uSA(0);
  const [draft, setDraft] = uSA('');
  const [pending, setPending] = uSA(false);
  const scRef = uRA(null);
  const st = WK_AS_STORE;
  const chat = st.chats.find((c) => c.id === st.active) || null;
  const rerender = () => force((x) => x + 1);
  const scrollEnd = () => requestAnimationFrame(() => { const el = scRef.current; if (el) el.scrollTop = el.scrollHeight; });

  const ask = (q) => {
    if (pending) return;
    let c = chat;
    if (!c) { const id = 'a' + (++st.seq); c = { id, title: truncQ(q, 42), day: 'Сегодня', msgs: [] }; st.chats.unshift(c); st.active = id; }
    c.msgs.push({ me: true, text: q });
    setDraft(''); setPending(true); rerender(); scrollEnd();
    setTimeout(() => { c.msgs.push({ me: false, ans: WK_AS_ANSWER(q, 'global') }); setPending(false); scrollEnd(); }, 900);
  };
  const del = (id) => { st.chats = st.chats.filter((c) => c.id !== id); if (st.active === id) st.active = null; rerender(); };

  const groups = [];
  st.chats.forEach((c) => { if (!groups.length || groups[groups.length - 1].day !== c.day) groups.push({ day: c.day, items: [] }); groups[groups.length - 1].items.push(c); });

  return (
    <>
      <div className="view-head">
        <Icon name="chat" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650, fontSize: 'var(--t-14)' }}>Ассистент</span>
        <span className="tip tip--bottom" data-tip="Записи в обработке и с ошибкой не участвуют в поиске">
          <Chip size="sm" tone="line" icon="doc">в поиске {AS_STATS.calls} из {AS_STATS.total} звонков · {fmtH(AS_STATS.dur)}</Chip>
        </span>
      </div>
      <div className="as-layout">
        <div className="as-chats">
          <div style={{ padding: '10px 10px 4px' }}>
            <Btn variant="default" size="sm" block icon="plus" onClick={() => { st.active = null; rerender(); }}>Новый чат</Btn>
          </div>
          <div className="as-chats-list scroll">
            {groups.map((g) => (
              <React.Fragment key={g.day}>
                <SecLabel>{g.day}</SecLabel>
                {g.items.map((c) => (
                  <div key={c.id} role="button" tabIndex={0} className="navitem" data-active={st.active === c.id ? 'true' : undefined}
                    onClick={() => { st.active = c.id; rerender(); scrollEnd(); }}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); st.active = c.id; rerender(); scrollEnd(); } }}>
                    <span className="nav-ico"><Icon name="chat" size={15} /></span>
                    <span className="nav-label u-trunc">{c.title}</span>
                    <span className="as-del" onClick={(e) => { e.stopPropagation(); del(c.id); }}><IconBtn icon="trash" size="sm" label="Удалить чат" /></span>
                  </div>
                ))}
              </React.Fragment>
            ))}
            {st.chats.length === 0 && <div className="u-faint" style={{ padding: '10px 12px', fontSize: 12.5 }}>Чатов пока нет</div>}
          </div>
        </div>

        <div className="as-main">
          <div className="as-scroll scroll" ref={scRef}>
            {!chat ? (
              <div className="as-empty">
                <div className="as-empty-ico"><Icon name="chat" size={22} /></div>
                <div style={{ fontWeight: 650, fontSize: 16 }}>Поиск по всем звонкам</div>
                <p className="u-muted" style={{ fontSize: 13.5, margin: 0, lineHeight: 1.55 }}>
                  Вопрос — это поиск по расшифровкам и рекапам, ответ — с указанием источников. Каждый диалог — новый чат.
                </p>
                <div className="ask-suggest" style={{ justifyContent: 'center', marginTop: 10 }}>
                  {AS_SUGGEST.map((q) => <Chip key={q} tone="line" icon="arrowRight" onClick={() => ask(q)}>{q}</Chip>)}
                </div>
              </div>
            ) : (
              <div className="as-doc">
                <div className="ask-thread">
                  {chat.msgs.map((m, i) => m.me
                    ? <div className="ask-row fade-up" data-me="true" key={i}><div className="ask-bubble">{m.text}</div></div>
                    : <div className="ask-row fade-up" data-me="false" key={i}><AnswerMsg ans={m.ans} callId={null} onOpenCall={onOpenCall} /></div>)}
                  {pending && <div className="ask-row" data-me="false"><div className="ask-bubble ask-pend"><Wave bars={4} color="var(--text-3)" height={13} />Поиск по {AS_STATS.calls} звонкам…</div></div>}
                </div>
              </div>
            )}
          </div>
          <div className="composer-dock">
            <form className="composer composer-ask ai-field" onSubmit={(e) => { e.preventDefault(); if (draft.trim()) ask(draft.trim()); }}>
              <Icon name="search" size={16} style={{ color: 'var(--text-3)', flex: '0 0 auto' }} />
              <input placeholder="Спросить по всем звонкам…" value={draft} onChange={(e) => setDraft(e.target.value)} />
              <IconBtn icon="send" active={!!draft.trim()} label="Отправить" onClick={(e) => { e.preventDefault(); if (draft.trim()) ask(draft.trim()); }} />
            </form>
          </div>
        </div>
      </div>
    </>
  );
}

Object.assign(window, { AssistantView, AnswerMsg, WK_AS_ANSWER, WK_AS_NEWCHAT, WK_ASK2, WK_AS_STORE, AS_STATS });
