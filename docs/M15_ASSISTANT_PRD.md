# PRD: M15 «Ассистент» — локальный RAG-чат по звонкам

> Статус: **approved for implementation** (2026-07-22).
> Дизайн-хендофф: `~/Downloads/design_handoff_wotold_assistant/` — мок = истина по вёрстке и состояниям, `01-SPEC.md` = истина по бизнес-логике. Canon-addendum: [`design/wotold-v2/assistant.md`](design/wotold-v2/assistant.md).
> Декомпозиция и статус задач: [`ROADMAP.md`](ROADMAP.md) §M15 + §B24.

## TL;DR

Два уровня одной функции: раздел **«Ассистент»** (глобальные чаты, поиск по всем звонкам) и **ассистент внутри звонка** (вкладка, один персистентный тред на звонок). Под капотом строго RAG: контекстное окно локальной модели 8K → классификация запроса → retrieval (FTS5 BM25, затем гибрид с ONNX-эмбеддером) → ответ локальной LLM строго по выданным фрагментам → источники (call_id + таймкод) привязываются детерминированно. Ассистент **только ищет и разбирает** информацию из звонков; генерация текстов — отказ без поиска.

## 1. Executive Summary

- **v1 = local-only.** Ответы генерирует локальный llama.cpp sidecar (Qwen 2.5 активного пресета). Cloud-ответы (Anthropic через прокси) — пост-M15 беклог; retrieval-слой проектируется общим.
- **Retrieval — гибрид, поэтапно внутри M15**: Ph1 — FTS5 BM25 (фича работает end-to-end), Ph2 — текст-эмбеддер + RRF-fusion. Обе фазы до закрытия милстоуна.
- **8K — не блокер, а причина RAG.** Замеры на M1 Pro (реальный транскрипт, продакшн-аргументы sidecar):

| | Qwen 1.5B (Light) | Qwen 3B (Balanced) |
|---|---|---|
| Prefill | 760 tok/s | 389 tok/s |
| Генерация | 55.6 tok/s | 30.3 tok/s |
| Тёплый ответ (промпт 4.5K ток, 250 ток ответа) | ~9с | ~16с |
| Холодный старт (one-shot) | +12с | +14с |
| Follow-up при resident + KV-префикс-кэше | ~1–3с | ~1–3с |

- **Дизайн имплементируем ~1-в-1**: мок построен на uikit v2; `ask-*`/`composer-*`/`.palette` уже в `wk.css`; портируется только add-on `wk2.css` (35 строк) + `ai-field` + иконка `chat`.

## 2. Goals / Non-Goals

**Goals**
1. Раздел «Ассистент»: чаты (новый чат = первый вопрос), поиск по всем ready-звонкам, ответы с источниками и разворачиваемым «Контекстом поиска».
2. Вкладка «Ассистент» в карточке ready-звонка: тред звонка, ответы по его расшифровке с возможностью привлечь другие звонки, эскалация «Искать во всех звонках».
3. ⌘K: команда «Ассистент — поиск по звонкам» + fallback «Спросить ассистента» при нуле результатов.
4. Честные состояния: отказ для генеративных запросов (без retrieval), честное «не найдено», pending «Поиск по N звонкам…».
5. Источники кликабельны: свой звонок → seek плеера, чужой → открытие звонка.

**Non-Goals (v1)**
- Генерация текстов любого рода (письма, переводы, планы) — refusal-классификатор, отказ до retrieval.
- Cloud-движок ответов — беклог (§12).
- Токен-стриминг ответа — pending-пузыря достаточно по SPEC; SSE-стриминг — беклог.
- Query-rewrite для анафоры multi-turn («а что он обещал?») — известное ограничение, беклог.
- Индексация processing-звонков — только `ready` (SPEC §2).
- Автоматическое включение resident-сервера — default не меняем (см. риск §11.2).

## 3. Бюджет контекстного окна

Окно фиксировано: `ctx = 8192` (llm.rs `DEFAULT_CTX_SIZE`, sidecar падает при prompt > ctx−4).

| Слот | Бюджет |
|---|---|
| System-инструкция (ru, injection-hardened) | ~0.6K ток |
| Фрагменты retrieval | ≤5.5K ток |
| История чата (≤2 последних QA-пары, каждая ≤150 ток) | ≤0.6K ток |
| Вопрос | ~0.1K ток |
| Резерв на ответ (`--n-predict`) | ~1K ток |

- Оценка токенов: `estimate_tokens = bytes/4` (существующий `chunker.rs`) — для кириллицы консервативен (реально ~4.5 б/ток), переполнение исключено. Множитель не вводим.
- Порядок промпта: `[system][fragments][history][question]` — при resident-сервере с `cache_prompt: true` префикс переживает follow-up-ходы (см. §6.4).
- Mono-строка UI «фрагментов: N · ≈X.XK токенов · окно 8K» — из счётчиков budget assembly.

## 4. Архитектура бэкенда

### 4.1 Модули

```
apps/desktop/src-tauri/src/assistant/
  mod.rs          — фасад: ask(), on_call_ready(), backfill(), stats()
  types.rs        — serde-зеркало contracts (S2)
  classifier.rs   — regex-классификатор генеративных запросов (без LLM)
  indexer.rs      — passage builder + FTS/embeddings + backfill + инвалидация
  retrieval.rs    — Ph1: FTS5 BM25; Ph2: + cosine + RRF
  budget.rs       — сборка контекста ≤5.5K, дедуп, нумерация фрагментов
  answer.rs       — промпт → LLM (json_schema) → привязка источников
  embedder.rs     — Ph2: ONNX текст-эмбеддер (feature `assistant-embed`)
apps/desktop/src-tauri/src/db/assistant.rs        — repository chats/messages/passages
apps/desktop/src-tauri/src/commands/assistant.rs  — Tauri-команды
```

Каждый файл <800 строк (hook `pre-write.mjs`); при росте `indexer.rs` дробить (`indexer/passages.rs` + `indexer/sync.rs`).

### 4.2 Пайплайн запроса `assistant::ask`

```
ask(pool, app, { chat_id?, call_id?, question })
 1. classifier::is_generative(q) → kind='refusal', БЕЗ retrieval, persist, return
 2. retrieval::search(q, scope)
      scope=call:   pass A (MATCH + фильтр call_id, top 8) + pass B (глобально минус звонок, top 4)
      scope=global: top 12
      0 матчей → kind='empty' (+escalate для call-scope), БЕЗ LLM, return
 3. budget::assemble → ≤5.5K ток, cap ≤3 passage/звонок (global), дедуп overlap-окон,
      нумерация [1..N], счётчики (frags, ≈X.XK)
 4. answer::generate
      resource_queue permit Resource::Llm (сериализация с рекапом)
      json_schema: {"answer": string, "used_fragments": int[]}
      resident server если жив (+cache_prompt: true), иначе one-shot llama-cli
 5. привязка источников: used_fragments → клэмп [1..N] + дедуп → passage → {call_id, start_ms};
      пусто/мусор → fallback top-3 по retrieval-score
 6. persist (user msg + assistant msg с answer_json), return
```

**Механизм источников — ключевое решение.** Модель 1.5–3B НЕ генерирует call_id/таймкоды. Она возвращает только `used_fragments: [int]` через json_schema-форсинг (поддержан в обоих путях: one-shot `--json-schema-file`, server `body["json_schema"]`; llama.cpp конвертит схему в GBNF — форма гарантирована структурно). Маппинг «индекс → (call_id, start_ms)» детерминирован на нашей стороне — галлюцинация таймкодов исключена конструктивно.

### 4.3 События

Расширение `events.rs` по образцу `recap:step`:

```
ASSISTANT_STATUS = "assistant:status"
payload { chat_id, phase: 'retrieving' | 'generating' }
```

> Поправка M15.7: фаза `queued` снята — permit берётся внутри `provider.generate`, момент постановки в очередь снаружи недоступен; очередь LLM и так видна фронту через существующий `queue:state` (UI выводит «ждём движок» из него). Токен-стриминга в v1 нет.

## 5. Схема данных

### 5.1 Миграция `0019_assistant.sql` (Ph1)

```sql
assistant_chats(
  id TEXT PK,
  call_id TEXT NULL REFERENCES calls(id) ON DELETE CASCADE,   -- NULL = глобальный чат
  title TEXT NOT NULL,                                        -- вопрос, усечённый ~42 симв (в Rust)
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
-- один персистентный тред на звонок:
CREATE UNIQUE INDEX ... ON assistant_chats(call_id) WHERE call_id IS NOT NULL;

assistant_messages(
  id TEXT PK,
  chat_id TEXT REFERENCES assistant_chats(id) ON DELETE CASCADE,
  role TEXT CHECK(role IN ('user','assistant')),
  text TEXT NOT NULL,            -- user: вопрос; assistant: ans.text
  answer_json TEXT NULL,         -- полный AssistantAnswer для role='assistant'
  order_idx INTEGER NOT NULL, created_at TEXT NOT NULL
);

assistant_passages(
  id INTEGER PRIMARY KEY AUTOINCREMENT,   -- rowid для FTS external content
  call_id TEXT REFERENCES calls(id) ON DELETE CASCADE,
  kind TEXT,                     -- 'transcript'|'recap'|'decision'|'action_item'|'open_question'
  speaker TEXT NULL, start_ms INTEGER NULL, end_ms INTEGER NULL,
  text TEXT NOT NULL, token_est INTEGER NOT NULL
);

assistant_fts — FTS5(content='assistant_passages', content_rowid='id',
  tokenize='unicode61 remove_diacritics 2')
+ AFTER INSERT/UPDATE/DELETE триггеры на assistant_passages;

assistant_index_state(call_id TEXT PK REFERENCES calls(id) ON DELETE CASCADE,
  indexed_at TEXT, passage_count INTEGER, token_total INTEGER);
```

**Sync FTS — триггеры, не код.** `delete_call` каскадно удаляет passages, SQLite активирует delete-триггеры на каскадах → FTS чистится сам, рассинхронизация невозможна ни из одной ветки кода. (Ровно то, что предлагал комментарий миграции 0006 при дропе `call_fts`, — закрывает follow-up #30 в рамках ассистента.) Все записи в `assistant_passages` — строго через repository `db/assistant.rs` (документировать в файле).

FTS5 доступен: bundled `libsqlite3-sys 0.30.1` (проверено в Cargo.lock). Smoke-тест создания виртуальной таблицы — в тестах миграции.

### 5.2 Миграция `0020_assistant_embeddings.sql` (Ph2)

```sql
assistant_embeddings(
  passage_id INTEGER PK REFERENCES assistant_passages(id) ON DELETE CASCADE,
  dim INTEGER NOT NULL, vec BLOB NOT NULL   -- f32 LE
);
```

Отдельная таблица (не колонка): backfill эмбеддингов асинхронный и не трогает FTS-триггеры. Масштаб: 1000 звонков × ~30 passages × 384-dim f32 ≈ 46 MB — brute-force scan с in-memory кэшем, sqlite-vec не нужен.

## 6. Индексация и retrieval

### 6.1 Passage builder (`indexer.rs`)

> Поправка M15.3 (после разведки кода): первоначальный план «chunks primary + md fallback» отменён — в `call_chunks.transcript_json` секунды **относительные чанку** (абсолютизация делается в `chunk_assembly.rs`), и повторять merge/remap дорого. Один код-путь по transcript.md покрывает и legacy.

Источники (только `status='ready'`):
- **Транскрипт**: `transcript.md` — финальная склейка (абсолютные таймкоды, финальные speaker-теги; формат `**{tag}** [{m}:{ss}]:` из `merge.rs::render_transcript_md`). Окна последовательных реплик до ~350 ток, overlap 1 реплика (только для окон ≥2 реплик); `speaker` = тег первой реплики (сырой `owner`/`Speaker N`, резолв имени — при сборке ответа M15.7).
- **Рекап**: `recap.md` по абзацам (заголовки скипаются) → kind='recap', `start_ms = NULL` (чип источника без таймкода — SPEC допускает).
- **Structured rows**: `decisions` / `action_items` / `open_questions` (text + evidence_quote/speaker/start_ms) → по одному passage. Самые плотные кандидаты для «какие решения/задачи».

### 6.2 Триггеры индексации

1. `assistant::indexer::spawn_index(app, call_id)` (fire-and-forget) в ready-точках: `pipeline/mod.rs` после `mark_call_ready` и `services/pipeline_runner.rs::spawn_regen` (успех RegenKind::Recap); в `cancel` (restore в ready) — прямой `index_call`. (Поправка M15.3: упоминавшаяся ранее `pipeline_runner.rs:451` — тестовый код, не продакшн-точка.)
2. **Startup backfill**: sweep в `lib.rs` setup — ready-звонки без записи в `assistant_index_state` → фоновая последовательная индексация; также добирает headless-пайплайны (app=None).
3. **Инвалидация**: `reprocess_call` → `deindex_call` (DELETE passages, триггеры чистят FTS), переиндексация ready-хуком после завершения; `regenerate_recap` → полная переиндексация тем же `index_call` (kind-selective не нужен — replace дешёвый и идемпотентный); `delete_call` — каскад.

### 6.3 Retrieval

**Ph1 — FTS5 BM25:**
- Нормализация запроса (lowercase, знаки), **каждый токен в кавычках** — `"токен"*` — защита от FTS5 MATCH-синтаксис-инъекции.
- Морфология-lite: слова длиной >5 → префикс-запрос по основе `max(4, len−2)` симв (`приватность → "приватн"*`). Компенсация отсутствия русского стемминга в unicode61.
- Ранк `bm25(assistant_fts)`. «Пусто» на старте = 0 матчей; порог отсечения подбирается на golden-set (M15.12).
- Trigram-токенизатор — **отклонённая альтернатива**: recall лучше, но индекс ×3–4 и мусорный bm25; семантику закрывает Ph2.

**Ph2 — гибрид:**
- Эмбеддер: `intfloat/multilingual-e5-small` int8 ONNX (~30MB + tokenizer.json). Новая запись в `MODEL_CATALOG` (`models.rs`), паттерн download + SHA256 + atomic rename; SHA через `refresh-model-catalog.sh` + cross-check workflow (bootstrap-trust как у M12.4).
- Инференс: research-спайк M15.9 — `fastembed` (UserDefinedEmbeddingModel, файлы наши, НЕ авто-download) vs `ort`+`tokenizers` напрямую; fastembed предпочтителен (mean-pooling из коробки). Feature-flag `assistant-embed` по образцу `voice-onnx`.
- Fusion: **RRF k=60** над двумя ранк-листами (BM25 top-30 + cosine top-30). Cosine — brute-force по in-memory кэшу (инвалидация по `assistant_index_state`).
- Без модели/feature — graceful degradation до чистого BM25.

### 6.4 Resident-сервер и латентность

- Если `local_engine.keep_resident` включён и `llama-server` жив — идём через него с `cache_prompt: true` (доработка `generate_via_server`: opt-in параметр). Порядок промпта `[system][fragments][history][question]` максимизирует KV-префикс на follow-up (~1–3с TTFT).
- Иначе one-shot `llama-cli`: холодный ответ до ~30с (старт 12–14с + prefill 9–16с) — честный `assistant:status` с elapsed. Смена контекста sidecar'ом другого вызова (рекап) инвалидирует кэш — первый ход после чужого вызова снова дорогой.

## 7. Контракты (S2)

`packages/contracts/src/assistant.ts` + export в index.ts; Rust-зеркало `assistant/types.rs` (`#[serde(rename_all = "camelCase")]`):

```ts
export type AssistantAnswerKind = 'answer' | 'refusal' | 'empty';
export interface AssistantSource { callId: string; callTitle: string; startMs: number | null; }
export interface AssistantFragment {
  callId: string; callTitle: string; kind: 'transcript'|'recap'|'decision'|'action_item'|'open_question';
  speaker: string | null; startMs: number | null; text: string;
}
export interface AssistantAnswer {
  kind: AssistantAnswerKind; text: string;
  sources: AssistantSource[]; fragments: AssistantFragment[];
  fragmentTokens: number; windowTokens: 8192; escalate?: boolean;
}
export interface AssistantChatMeta { id: string; callId: string | null; title: string; createdAt: string; }
export interface AssistantMessage {
  id: string; role: 'user' | 'assistant'; text: string;
  answer: AssistantAnswer | null; createdAt: string;
}
export interface AssistantIndexStats { indexedCalls: number; totalCalls: number; totalDurationSec: number; }
```

`callTitle` денормализуется в ответ (фронту не нужен второй запрос); при рендере истории титулы резолвятся заново по списку звонков (как `byCall` в моке) — переименование звонка не ломает старые чаты.

### Tauri-команды (`commands/assistant.rs` → `lib.rs`)

```rust
assistant_index_stats() -> AssistantIndexStats                 // чип «в поиске X из Y»
assistant_list_chats() -> Vec<AssistantChatMeta>               // только call_id IS NULL
assistant_get_chat(chat_id) -> Vec<AssistantMessage>
assistant_get_call_thread(call_id) -> Option<(chat_id, Vec<AssistantMessage>)>
assistant_ask(chat_id: Option, call_id: Option, question) -> { chat_id, message }
assistant_delete_chat(chat_id)
```

## 8. UI

Дизайн — «точь в точь» по моку. Все строки — из мока, деловой регистр, через i18n `assistant.*` ×3 локали (ru дословно, en/kk перевод). Обязателен design-gate alignment-блок до кода (B24.0).

### 8.1 Маппинг мок → репо

| Мок | Репо |
|---|---|
| `AssistantView` (wk2-assistant.jsx) | **новый** `src/pages/AssistantPage.tsx` — view-head + чип статистики с tooltip, колонка чатов 232px (группировка Сегодня/Вчера/…, `div role="button"` без вложенных `<button>`, trash по hover), empty-state + 4 чипа-подсказки, тред, композер |
| `AnswerMsg` | **новый** `src/components/assistant/AnswerMsg.tsx` — refusal-нота (shield), pre-line текст, src-row чипы (свой: clock+таймкод→seek; чужой: doc+«Название · т/к»→открыть), escalate-чип, `details.ctx` (спикер в его `--spN`-цвете + звонок + т/к + текст, mono-строка), ans-acts по hover: copy (галка 1.4s, clipboard rejection catch), share-дропдаун («Скопировать с источниками» = текст + `\n\nИсточники: ` через `;`, «Отправить в почту…» = mailto) |
| Тред + pending | **новый** `src/components/assistant/AskThread.tsx` (общий для страницы и вкладки; pending «Поиск по N звонкам…»/«Поиск…» + Wave `--text-3`) + `AssistantComposer.tsx` (`composer composer-ask ai-field`) |
| Вкладка звонка (wk2-screens.jsx) | правка `CallDetailPage.tsx`: `type Tab += 'assistant'`, таб виден только при `ready`, composer-dock swap, чип своего звонка → `setTab('transcript')` + `audio.seek(t)` (паттерн `onJumpToCurrent`), эскалация → `askGlobal(q)` |
| Палитра (wk2-app.jsx) | правка `CommandPalette.tsx`: команда «Ассистент — поиск по звонкам», fallback-секция «Ничего не найдено · Ассистент» (Enter → новый глобальный чат), плейсхолдер «Найти звонок или спросить ассистента…», `ai-field` на `.palette` |
| Sidebar / minirail | правка `AppSidebar.tsx`: `RailView += 'assistant'`, NavItem + minirail IconBtn `chat`, кнопка «Найти или спросить» (`ai-field ai-field--panel`, sparkle, nowrap+ellipsis) |
| Роутинг | правка `App.tsx`: case 'assistant', hash в `initialView`, колбэк `askGlobal(q)` (эскалация + ⌘K-fallback) |

### 8.2 Состояние и API

- **новый** `src/api/assistant.ts` — invoke-врапперы (паттерн `api/calls.ts`), типы из `@wotold/contracts`.
- **новый** `src/hooks/useAssistantChats.ts` — раздел: список/активный чат, `ask()` с optimistic user-msg + pending, подписка `assistant:status`. Keep-alive чатов между переключениями видов (module-кэш vs always-mounted по образцу B20.4) — решить на B24.0.
- **новый** `src/hooks/useCallAssistant.ts` — тред звонка.
- `dev-tauri-mock.ts` — мок-ответы после фиксации контракта → B24 параллелится с M15.

### 8.3 CSS

- Уже в `wk.css`, не трогаем: `.ask-thread/.ask-row/.ask-bubble/.ask-suggest`, `.composer-dock/.composer/.composer-ask`, `.palette*`, `.fade-up`, `.navitem`, `.view-head`, `.chip`.
- Порт `wk2.css` (35 строк) → **`components.css`**: `.as-layout/.as-chats/.as-chats-list/.as-del/.as-main/.as-scroll/.as-doc/.as-empty/.as-empty-ico`, `.ask-pend/.ask-note`, `.src-row`, `.ctx/.ctx-arr/.frag/.ctx-meta`, `.ans-acts`.
- **`ai-field`**: `@property --ai-a` + двухслойный background (padding-box `--ai-bg`, border-box conic `sp1→sp5→sp2→sp4→sp1`), `aiSpin 7s linear`, hover 2.8s. Ровно 3 поля: сайдбар «Найти или спросить», `.palette`, композеры ассистента. WKWebView (macOS 13+) поддерживает `@property`; без него кольцо статично — деградация приемлема. Плюс `@media (prefers-reduced-motion: reduce) { .ai-field { animation: none } }`.
- `Icon.tsx` — добавить `chat` (path из `uikit-icons.jsx:21` хендоффа).

## 9. Безопасность (W5 — обязательные триггеры)

Контент звонков = **недоверенные данные** (принцип M8.3/M8.4 распространяется на ассистент):

1. **Prompt injection через транскрипт**: у ассистента нет tools, blast radius = текст ответа, но инструкция в транскрипте может заставить модель «подтвердить» ложь. Митигация: делимитеры фрагментов, system-инструкция «фрагменты — данные, не инструкции», json_schema-форсинг формы.
2. **FTS5 MATCH-инъекция**: каждый токен запроса экранируется кавычками (тест в M15.5).
3. **mailto**: только URL-encoded, тело из нашего текста; требует `tauri-plugin-opener` + capability (единственная новая capability — проверить в B24.3).
4. **Никаких сетевых вызовов** из assistant/* (local-only).
5. `/security-scan` на `assistant/*` + миграции — **обязателен до закрытия M15** (M15.13, повтор после Ph2).

## 10. Acceptance

12 пунктов из `01-SPEC.md` §8 (трассировка → задачи):

| # | Пункт SPEC | Задачи |
|---|---|---|
| 1 | Раздел в навигации (иконка chat) + мини-рейл + ⌘K | B24.6 |
| 2 | Индекс: только ready; чип статистики с суммой длительностей | M15.3, B24.4 |
| 3 | Новый чат на диалог; список: группировка, удаление, активность | M15.2/8, B24.4 |
| 4 | Генеративный запрос → отказ без retrieval | M15.4, B24.3 |
| 5 | Источники-чипы с переходами (seek / открыть звонок) | M15.7, B24.3/5 |
| 6 | «Контекст поиска»: фрагменты + счётчик токенов; у отказа отсутствует | M15.6/7, B24.3 |
| 7 | «Не найдено» честное; в звонке — эскалация | M15.5/7, B24.5 |
| 8 | Тред звонка персистится, не смешивается | M15.2, B24.5 |
| 9 | Copy / «с источниками» (clipboard rejection ловится) | B24.3 |
| 10 | ⌘K fallback с Enter | B24.6 |
| 11 | ai-field анимируется, light/dark, ничего больше не перекрашено | B24.1/7 |
| 12 | Консоль чистая: без вложенных button, без unhandled rejections | B24.4/7 |

Плюс: light/dark на всех новых поверхностях, a11y-architect ревью (B24.7), `cargo fmt/check/clippy/test` + `pnpm -r typecheck` + vitest.

## 11. Риски / открытые вопросы

1. **Качество `used_fragments` на 1.5B (Light)**: схема гарантирует форму, не осмысленность. Митигация: клэмп + fallback top-score, eval M15.12. Открыто: показывать ли «●●○»-хинт качества (аналог R12) на Light.
2. **Холодная латентность** при `keep_resident` OFF (default): первый ответ до ~30с. Предложение: one-time хинт «включите резидентный движок» при первом вопросе; default НЕ менять без явного согласования.
3. **Конкуренция resource_queue**: вопрос во время генерации рекапа встаёт за permit — честная фаза `queued`. Приоритизацию не делаем (permit=1 защищает память).
4. **Multi-turn анафора**: retrieval по текущему вопросу; query-rewrite — беклог.
5. **Keep-alive чатов** между видами: module-кэш vs always-mounted (B20.4-паттерн) — решить на design-gate B24.0.
6. **Лицензия/SHA эмбеддер-модели** (e5-small int8 на HF) — проверить в M15.9.
7. **bytes/4 vs 4.5 б/ток кириллицы** — бюджет консервативнее, риска нет, множитель не вводим.

## 12. Беклог после M15

- **Cloud-ответы**: `AnthropicProvider` через прокси на общем retrieval (меняется только answer-engine + квоты + consent на отправку фрагментов).
- **Токен-стриминг**: llama-server SSE (`stream:true`) → событие `assistant:token` (требует resident ON).
- **Map-reduce отчёты по архиву** («сводка недели по всем звонкам») — за пределами 8K, refine-chain, отдельный milestone.
- **MCP-tool `search_passages`** в `services/mcp` — read-only чтение тех же `assistant_passages`/`assistant_fts`.
- **Query-rewrite multi-turn** (анафора).
- **«Отправить в почту»** → полноценный share (не только mailto).
- **Инкрементальная индексация processing-звонков** (по чанкам до ready).
