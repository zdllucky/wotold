# Roadmap

> Декомпозиция Этапов раздела 11 [ПАСПОРТА](ПАСПОРТ_ПРОЕКТА.md) на единицы реализации. Файл — источник истины по статусу фич, читается и обновляется людьми. Параллельно в харнессе Claude Code лежит TaskList с теми же ID — синхронизируется вручную в этом файле при изменении статуса.
>
> Легенда: `[x]` готово · `[ ]` пендинг · `[~]` частично · `→ #N` блокируется задачей N.
>
> **История исполненного** (MVP-этапы, батчи V2–V7/W/S, B1–B19, B16–B18, M12–M14) вынесена в [`ROADMAP_ARCHIVE.md`](ROADMAP_ARCHIVE.md) — здесь только живая работа.

---

## Статус

MVP реализован и работает (этапы 1–12 паспорта + local engine M12 + chunked pipeline M13 + summary v2 M14 + редизайн Wotold v2 B18). Полный лог — в [архиве](ROADMAP_ARCHIVE.md).

**Блокеры публичного релиза** — секция A беклога ниже (#42 minisign, #44 CF production, security-scan, manual QA).

## B18 · остатки (открытое из редизайна)

> B18 закрыт (см. архив), эти пункты остались открытыми:

- [ ] **Views (saved smart-collections) + Explore** — персистентность `SavedView` (S2 контракт `{label, filter_state, view_mode, sort}`) + экраны по прототипу `wk-explore.jsx`.
- [ ] **Inbox stats** (4 hero + sparkline) — как в прототипе (`wk-screens`/`wk-inbox`).
- [x] **Assistant-таб** → поглощён милстоуном **M15** и реализован в батче **B24** (вкладка звонка B24.5 + раздел B24.4) — полноценный RAG-ассистент вместо точечного Q&A-таба.
- [x] **Contacts B18.4 доводка** — identifiers chips + derivations были реализованы ранее; форма приведена к канону в B23.
- [ ] **Manual visual QA** (light/dark, все экраны) — human follow-up, агент не скриншотит native app.
- [ ] **Manual QA: живой seek из источника-чипа** (B24.7 п.5) — клик clock-чипа во вкладке ассистента реального звонка мотает плеер и запускает воспроизведение. Логика покрыта RTL; аудио в браузер-моке нет — проверяется руками в native.
- [ ] **B18 a11y follow-up** — recording-state live-region (SC 4.1.3); отдельный aria-label тема-toggle; toast dismiss контекст (SC 4.1.2); capabilities least-privilege split для recording-widget; токен `--on-danger`; контраст `--wc-*` в dark; зачистка dead i18n `home.*`/`calls.*`.
- [ ] **Сверка контрактов (S2)** — `ActionItemV2`/`Decisions`/`OpenQuestions` уже в `packages/contracts` (M14) — переиспользовать, не дублировать.

---

## M15 · Ассистент (RAG-чат по звонкам)

> PRD: [`M15_ASSISTANT_PRD.md`](M15_ASSISTANT_PRD.md) · дизайн-канон: [`design/wotold-v2/assistant.md`](design/wotold-v2/assistant.md) · хендофф: `~/Downloads/design_handoff_wotold_assistant/`.
> Решения: v1 local-only (cloud → беклог G); retrieval гибрид поэтапно (Ph1 FTS5 → Ph2 эмбеддер+RRF, обе до закрытия M15); окно 8K, источники через json_schema `used_fragments` (детерминированная привязка). UI-часть — батч B24 ниже (параллелится с Ph1 после M15.1).

### Ph1 — FTS5 retrieval end-to-end

- [x] **M15.0** (S) PRD + canon-addendum + эта секция роадмапа.
- [x] **M15.1** (S) Контракт S2: `packages/contracts/src/assistant.ts` (AssistantAnswer/Source/Fragment/ChatMeta/Message/IndexStats) + export + Rust-зеркало `assistant/types.rs`. Тест: serde round-trip (4 шт).
- [x] **M15.2** (M) Миграция `0019_assistant.sql` (chats / messages / passages / FTS5 external-content + триггеры / index_state; partial-UNIQUE тред-на-звонок) + repository `db/assistant.rs`. TDD `fresh_db`: 16 тестов — CRUD, каскады call→chat/passages, FTS-триггер-синк, FTS5 smoke, конкурентность (get_or_create ×2, order_idx ×8), malformed MATCH → Err. → #30 закрыт (FTS5 вернулся как assistant_fts). rust-reviewer: 0 CRITICAL/HIGH, MEDIUMs пофикшены.
- [x] **M15.3** (L) Indexer: passage builder по `transcript.md` (окна ~350 ток c overlap; chunks-путь отменён — см. PRD §6.1 поправку), recap-абзацы, structured rows; `spawn_index` в ready-точках (`pipeline::run`, regen Recap, cancel-restore), `deindex_call` при reprocess, startup backfill c анти-гонкой + self-heal index_state. 14 тестов. rust-reviewer: 0 CRIT/HIGH, 4 MEDIUM пофикшены.
- [x] **M15.4** (S) Классификатор `is_generative`: точные словоформы (императив±«те» + инфинитив) вместо substring-regex — прошедшее время («написали», «отправил») не матчится. 32 кейса в 3 тест-таблицах.
- [x] **M15.5** (M) Retrieval BM25: токенизация-санитизация (MATCH-синтаксис структурно недостижим), префикс-экспансия морфологии (`приватность → "приватнос"*`), OR-recall, cap 12 токенов, two-pass call-scope (8 своих + 4 глобальных). 13 тестов, включая инъекции против живого FTS5 и case-fold кириллицы.
- [x] **M15.6** (S) Budget assembly: greedy ≤5.5K с skip-and-continue, cap ≤3/звонок (только global), дедуп текста + интервальный дедуп overlap-окон, стабильный порядок → нумерация [1..N]; API на Scope-enum, hits по значению без клонов. 7 тестов. (`estimate_tokens` остался в chunker — уже shared, вынос снят.) rust-reviewer чанка: 0 CRIT/HIGH, 4 MEDIUM пофикшены.
- [x] **M15.7** (L) Answer engine: injection-hardened system-промпт + нейтрализация `<<<`/`>>>`-маркеров в фрагментах/титулах/истории (адверсариальный тест), json_schema `{answer, used_fragments}` → детерминированная привязка источников (клэмп+дедуп+fallback top-3), история ≤2 QA×150 ток, refusal/empty short-circuit, `ask_core` (DI-провайдер) + macos-`ask` (build_local_llm_provider + `with_cache_prompt(true)`), событие `assistant:status` (retrieving/generating; queued снят — очередь видна через `queue:state`). 15 тестов на MockProvider+fresh_db.
- [x] **M15.8** (M) Команды: `commands/assistant.rs` — 6 команд (ask macos-gated R9, validate_question cap 2000 симв + unit-тест) + регистрация mod.rs/lib.rs. Логика покрыта тестами ядра; command-слой тонкий by design. rust-reviewer чанка: 1 CRITICAL (delimiter injection) + 2 HIGH (queue-метка, дубль persist) найдены и пофикшены до коммита.
- [x] **Gate Ph1** — живой e2e пройден (env-gated `live_gate_ph1`-тест: копия реальной БД + llama-server Qwen 3B): refusal 4мс без LLM, честное «не найдено» 1мс, живой вопрос → связный ответ с детерминированным источником за **3.8с** (resident, 5 фрагментов ~1K ток). Найдено и пофикшено: WAL-копирование БД (VACUUM INTO), пустой `answer` от 3B на «нет ответа» → honest no-direct-answer вместо ошибки. Вердикт: BM25-retrieval слаб на общих вопросах («о чём договорились») — ровно зона Ph2-эмбеддера.

### Ph2 — гибрид (семантический поиск)

- [x] **M15.9** (L) Эмбеддер: спайк решён в пользу **ort+tokenizers** (fastembed откат: TokenizerFiles = 4 файла → 5 SHA-записей; дефолт тянет hf-hub/image-models). Каталог: 2 записи официального intfloat ONNX (qint8 118MB — PRD-оценка «~30MB» неверна, XLM-R словарь; + tokenizer.json 17MB), MIT, SHA сняты локально. `embedder.rs`: трейт вне feature (Mock для даунстрим-тестов), OnnxTextEmbedder под `assistant-embed` (префиксы `query:`/`passage:` — решение спайка, mean-pool+L2, token_type_ids нулями, truncation 512). Замеры M1 Pro: ~5мс/пассаж, ~91мс 350-ток, load 228мс → spawn_blocking, без Resource::Embed. Download-UX: `shared_model_ids` (качается с пресетом). Тесты: 5 unit (Mock) + `#[ignore]` reference на реальной модели (fingerprint+семантика+кросс-язычность) — зелёный.
- [x] **M15.10** (M) Миграция 0020 (passage_id PK CASCADE, dim per-row, vec BLOB f32 LE — PRD §5.2 дословно) + репозиторий `db/assistant_embeddings.rs` (отдельный модуль: assistant.rs на пределе 800 строк). Embed-hook: `index_call` резолвит shared-эмбеддер по `store.app_data_dir()` (сигнатуры ready-хуков не тронуты), ошибки embed НЕ роняют FTS-индексацию; `embed_backfill` батчами по 64 из lib.rs setup. Инвалидация при смене модели: KV `assistant.embed_model_id` → mismatch = clear + пересчёт. `embed_cache.rs`: инстансный `EmbedCache` (+процессный `global()`), снимок по штампу (index_state + счётчик векторов), битые BLOB скипаются с warn. TDD: 4 repo + 1 ensure + 3 indexer (Mock) + 3 cache. Контракт IndexStats не тронут (coverage — лог backfill; UI-прогресс → беклог G). → M15.9
- [x] **M15.11** (M) Гибрид: `fusion.rs` (RRF k=60, тай-брейк по passage_id — стабильная нумерация источников; 6 TDD-тестов с ручными числами) + `retrieval::search_hybrid` (BM25 top-30 + cosine top-30 по кэшу → RRF → прежние лимиты 12/8+4; cosine-only id материализуются `fetch_passages_by_ids`; call-scope: RRF внутри проходов, свои раньше чужих). Деградация: None-эмбеддер/пустой кэш → ветка Ph1 (`search` не тронут — 13 тестов Ph1 без правок). `ask_core_with(+embedder)`, прод-обёртка `ask` резолвит shared; `rank` гибридных хитов = RRF-score (диагностика, даунстрим не читает). 5 интеграционных тестов на KeywordMock: golden-синонимы (BM25 мимо — вектор находит), эквивалентность Ph1, пустой кэш, own-first, стабильность. → M15.10
- [x] **M15.12** (S) Mini-eval harness (`assistant/eval.rs` + `eval_fixtures/`, паттерн golden_eval): корпус 3 звонка × 13 пассажей + 12 QA-кейсов. Уровень A — BM25-baseline в CI (документирует miss-кейсы лексики: синоним/семантика/кросс-язычный); уровень B — `#[ignore]`+env на реальной e5: **hit@3 7/10, hit@5 9/10, MRR 0.657**, все BM25-miss кейсы гибрид находит. Подбор empty-порога дал отрицательный результат: cosine-диапазоны перекрываются (garbage 0.819 > synonym-rel 0.7785) → **floor не введён**, честное «пусто» гибрида — ответственность answer-слоя (NO_DIRECT_ANSWER, M15.7); зафиксировано в PRD §6.3. → M15.11

### Ph3 — закрытие

- [x] **M15.13** (S) Security-scan (security-reviewer, 2026-07-22, скоуп Ph1+Ph2: assistant/* + db/assistant* + commands + e5-записи каталога + миграции 0019/0020 + Cargo deps): **0 CRITICAL/HIGH/MEDIUM**. Проверено и чисто: FTS MATCH-инъекция (единственный путь через build_match_expr, expr сам bind-параметр), prompt-инъекция (neutralize_markers + json_schema без call_id + клэмп used_fragments; XSS-эскалации нет — innerHTML не используется), логи без контента (только счётчики/id), path traversal (пути только из компилируемых ModelId-констант), supply chain (реальные SHA, `=`-пин ort, tokenizers без default-фич), SQL (всё bind, каскады полные при `foreign_keys=ON`), DoS-лимиты, Tauri-граница (6 команд за State, без путей/URL от фронта), секретов нет. 4 INFO-наблюдения (известные trade-off): (1) QUESTION_MAX_CHARS только на Tauri-границе — продублировать в ask_core_with при появлении второй точки входа (MCP/batch); (2) анти-инъекция декларативная+структурная, пересмотреть при добавлении tool-calling; (3) ort build-script качает prebuilt ORT (прецедент sherpa-onnx, задокументирован); (4) EmbedCache full-load без cap (~46MB/1000 звонков — не эксплуатируемо в single-user desktop). → M15.8, повтор после M15.11 ✓

**M15 закрыт целиком** (Ph1 gate ✓, B24 UI ✓, Ph2 гибрид ✓, security ✓). Human-verify остатки — в секции A Manual QA (живой seek, визуальный QA) + скачивание e5-модели через Storage-UI для активации гибрида.

## B24 · Ассистент UI (по хендоффу, «точь в точь»)

> Design gate обязателен (B24.0). Параллелится с M15 через `dev-tauri-mock` после M15.1. Сшивка с живым бэкендом — после M15.8.

- [x] **B24.0** (S) Design-gate пройден (alignment-блок в сессии). Решения: keep-alive = module-кэш в хуке (single-instance инвариант задокументирован); цвет спикера owner→sp1 + stable-hash→sp2..5; «Отправить в почту» = mailto с фоллбеком в копию.
- [x] **B24.1** (S) CSS-порт `wk2.css` → `components.css` (as-*, ai-field+`@property`+`prefers-reduced-motion`, ask-pend/note, src-row, ctx/frag, ans-acts; +focus-within для trash в списке чатов) + иконка `chat` в Icon.tsx.
- [x] **B24.2** (M) Контракт += AskArgs/AskOutcome/CallThread/событие; `api/assistant.ts`; `useAssistantChats` (module-кэш, optimistic, статус-listener) + `useCallAssistant`; dev-mock assistant_* (типизирован контрактом, сортировка по активности); i18n `assistant.*` ×3. 7 hook-тестов, включая гонки (двойной ask, смена чата/звонка mid-pending, out-of-order openChat).
- [x] **B24.3** (M) `AnswerMsg.tsx`: 3 kinds, чипы источников (clock→seek / doc→открыть), details «Контекст поиска» (спикер в `--spN`, mono-строка), эскалация, copy (галка только после успешной записи — ревью H6) + share («с источниками», mailto+фоллбек). 7 RTL-тестов. typescript-reviewer чанка: 0 CRIT, 7 HIGH (гонки/утечка listener/false-copied) — все пофикшены.
- [x] **B24.4** (L) `AssistantPage` + `AskThread` + `AssistantComposer`: колонка чатов (группировка по дням, trash по hover/focus), empty + 4 чипа, pending, композер ai-field (Enter-хендлер: в форме нет submit-кнопки; IME-guard; disabled при pending), чип статистики (3-формный ru-плюрал), мост `requestGlobalQuestion` (ref-стабилизированный). RTL ×3 файла.
- [x] **B24.5** (M) Вкладка звонка: `Tab += 'assistant'` (только ready; сброс таба при смене звонка и при уходе из ready — фикс пустой панели по ревью), тред `useCallAssistant`, композер вместо плеера на табе, seek из источника (ms→сек + auto-play), эскалация → раздел. Подсказки мока в звонке намеренно опущены (мок-специфичный банк). RTL: видимость таба ready/processing. Заметка B24.7: lazy-fetch треда до открытия таба — перф-полиш.
- [x] **B24.6** (M) Навигация + ⌘K: `RailView += 'assistant'`, NavItem + minirail, «Найти или спросить» (ai-field--panel + sparkle), палитра: команда + fallback «Спросить ассистента» (подстрока с N из indexStats) + новый плейсхолдер ×3 локали + ai-field на палитре, hash-роут. RTL: fallback только при 0 матчей, Enter → onAsk. Живой смоук в dev-mock браузере: раздел/чаты/refusal/empty/эскалация/⌘K/dark — пройден, консоль чистая. typescript-reviewer чанка: 4 HIGH (пустая панель таба, disabled композера, 2 тест-дыры) — пофикшены.
- [x] **B24.7** (M) Acceptance-pass: 12 пунктов SPEC прогнаны в dev-mock браузере — 11 ✓, п.5 (живой seek) → Manual QA (секция A). Поймано и пофикшено: минирейл без пункта «Ассистент» (IconBtn chat). a11y-architect ревью → фиксы: C1 вложенный button в строке чата → `ul/li` + кнопка-сосед (`aria-current`), C2 focus-visible композера/палитры (`:focus-within` box-shadow), C3 контраст `--text-faint`→`--text-3` (ctx-meta/summary/empty), H1 `role=log`+`role=status` (pending, Wave aria-hidden), H2 hitbox чипов ≥24px (`::after inset:-3px`), H3 `.tip` по `:focus-visible`, M1 disabled у send, M2 combobox/listbox палитры (`aria-activedescendant`), M3 list-семантика чатов. 540 vitest / typecheck зелёные, консоль чистая.

---

## Беклог (groomed)

> Единый groomed-беклог пост-MVP работ. Прежний `CHUNKED_PIPELINE_BACKLOG.md` влит сюда и удалён — это единственный источник истины по открытым задачам. Сгруппировано по приоритету. Когда задачу забираем в работу — оформляется как полноценная (deps, чек-боксы) и синхронизируется с TaskList харнесса.
>
> **Закрыто при последнем грумминге (не в беклоге):** live duration tracking (`[P5.2]`) · SpeakerConfirmModal sample playback (`[P-fix6]/[P-fix8]`) · recap `failed_reason`↔engine-label mismatch (`[P5.1]`) · split `db/calls.rs` · storage UI при смене preset (M12.5, R12-bis). Follow-up в manual-QA (секция A): live-переверить два бывших бага на реальном звонке.

### A. Release-блокеры (до публичного релиза)

- [ ] **#42 X1 Tauri minisign keygen** — `pnpm tauri signer generate`; public → `tauri.conf.json`, private+password → GH Secret + офлайн-бэкап (M11.1/M11.9). Без этого updater не работает.
- [~] **#44 X3 CF production provisioning** — staging закрыт полностью. Осталось: GH Secrets с суффиксом `_PRODUCTION`, Google OAuth Authorized redirect URI для prod callback, tag `v0.1.0` для триггера prod-деплоя. Процедура — `docs/DEPLOYMENT.md`.
- [ ] **`/security-scan` (W5)** на `local_engine/{models,llm,stt}.rs` + `capabilities/default.json` + `scripts/refresh-model-catalog.sh` — обязателен перед production release.
- [ ] **Manual visual QA** — 6 theme×accent (light/dark × bordeaux/persian/ink) на всех экранах, включая Engine picker (M12.5) и ChunkProgressStrip (M13.3). Сюда же — live-реверификация двух бывших багов (playback модала + failed_reason badge).

### B. Verification gaps (нужны реальные фикстуры / бинари)

- [ ] **M12.1 whisper acceptance integration test** — bundled WAV (RU + 2 спикера) → snapshot `DiarizedTranscript`. Требует реального `whisper-cli` в `binaries/`.
- [ ] **B3.7d embedding reference test** — integration против reference-эмбеддинга для зашитого WAV (sherpa-onnx fixture, `--features voice-onnx`).
- [ ] **M13.1.6 + M13.2.4 chunked smoke** — dual-run на 30-мин фикстуре (diff ≥99%) + verification на multi-speaker WAV. Deferred to end — требует real WAV.
- [ ] **`pipeline::run` / `reprocess_call` / `regenerate_recap` unit-тесты** — happy + missing audio + recap fail. Сейчас не покрыты.
- [ ] **M12 «можно стартовать» чек-лист** — sherpa-onnx version с Whisper+sortformer проверен (changelog crate); CI build matrix под feature `local-engine` (macOS arm64+x86_64 only); PRD review заказчиком (O1–O5 closed/accepted).

### C. Code / feature debt

- [~] **device-id HMAC-bind** — /16 IP rate-limit уже сделан. Осталось: HMAC-привязка device-id к server-side secret при первом контакте (контракт-change: клиент хранит bound-token).
- [ ] **M12.6 cancellation flow** — SIGTERM на sidecar при delete звонка during processing. `tauri_plugin_shell::Child::kill()` + spawn-handle tracking.
- [ ] **identify_speakers pipeline wire / reconcile** — сверить, нужен ли старый `identify_speakers` orchestrator (#25: embedding+llm+merge_signals) при работающем B3.x cluster-path (`run_cluster_pipeline`), либо он вытеснен. Переформулировать/выпилить мёртвый путь.
- [ ] **Settings auto-name из NSFullUserName** — default «Я» + edit в онбординге. Требует Swift bridge.

### D. Diarization / LLM-progress polish

- [ ] **Threshold 0.4 → 0.35** — нужен golden-set из 2-3 mic-записей с known speaker counts (локальный verify-скрипт, не CI).
- [ ] **VAD config exposure** — через sherpa-onnx `OfflineVoiceActivityDetector` (нужен FFI research — поддерживает ли Rust binding dynamic VAD params).
- [ ] **Embeddings audit для коротких сегментов (<2s)** — cosine similarity нестабильна на окнах короче threshold (WeSpeaker trained на ~5s).
- [ ] **Per-cluster centroid distances** — `log::debug` cos_dist на каждый merge в `speaker_reclustering`. Detail polish.
- [ ] **Sortformer → ECAPA-TDNN / Wespeaker v2** — отдельный milestone, heavy research. Текущий WeSpeaker — baseline.
- [ ] **LLM progress %** — parse llama-cli streaming (`print_timings` / `n_eval / n_predict`). Сейчас UI показывает только elapsed_sec.
- [ ] **Cancel button во время recap regen** — `CancelToken` + propagation через `local_orchestrator::run_v2_pipeline` + `SidecarGuard::kill()`.
- [ ] **Expected-duration hint** «~5 из 10 мин» — preset-dependent estimate из telemetry median.
- [ ] **Periodic emit во время STT** (не только LLM) — generic `with_recap_progress_emitter` переиспользовать на `LocalWhisperProvider::transcribe`, новое событие `stt:progress`.

### E. UX / прочее

- [ ] **Audio player conditional badge** — «Аудио недоступно до завершения обработки» когда merged WAV ещё processing + «X из Y чанков готово» hint (derived из `useCallDetail.chunks`).
- [ ] **Telemetry `chunk_failed`** — `db/telemetry.rs` schema extension `(call_id, chunk_idx, reason, retried_count, created_at)` + dev-only aggregate dashboard «X% chunks failed last 7 days», per-preset breakdown.
- [ ] **Reprocess incremental** — reuse `status='done'` chunks вместо полного re-STT. `chunk_assembly` уже фильтрует done, но reprocess сбрасывает все к pending. Rerun только failed → экономия для частично-успешных записей.
- [ ] **Dev hot-reload auto-restart** — `scripts/dev.sh` с watchexec/entr на `src-tauri/src/`, on change `pkill -SIGTERM wotold-desktop` → tauri dev сам re-launch'ит. Минимально-инвазивный (~10 строк bash). Сейчас edit Rust требует ручного kill + рестарта.

### F. Cross-platform / большие куски

- [ ] **R9/R4 Linux/Windows** — local-engine + audio capture за trait + `unimplemented!()` сейчас. Big chunk, MVP только macOS.
- [ ] **R10 model bundling** — bundled installer для full preset (~50MB) если CI/CD scale'ится. Сейчас on-demand download.

### G. После M15 (ассистент) — PRD §12

- [ ] **Cloud-ответы ассистента** — `AnthropicProvider` через прокси на общем retrieval-слое (answer-engine switch + квоты + consent на отправку фрагментов в облако).
- [ ] **Токен-стриминг** — llama-server SSE (`stream:true`) → событие `assistant:token`; требует resident ON.
- [ ] **Map-reduce отчёты по архиву** («сводка недели по всем звонкам») — за пределами 8K, refine-chain; отдельный milestone.
- [ ] **MCP-tool `search_passages`** — read-only чтение `assistant_passages`/`assistant_fts` из `services/mcp`.
- [ ] **Query-rewrite multi-turn** — анафора («а что он обещал?») через LLM-переформулировку запроса.
- [ ] **«Отправить в почту» → полноценный share** (сейчас mailto).
- [ ] **Инкрементальная индексация processing-звонков** — по чанкам до ready.

---

## B20 · UI polish (батчи юзера)

> Пользовательские полиш-батчи по 5–10 пунктов после ревью v2. Новые батчи добавляются подсекциями ниже; выполненное помечается на месте (сюда, не в архив, пока батч не закрыт целиком).

### Батч 1 (2026-07-21)

- [x] **B20.1** RecapThinking → reasoning-stream в стиле Claude Code: без кружков/номеров/галок, активный шаг text-shimmer, превью инлайн тихим текстом; пустое превью не рендерит аффорданс. (`RecapThinking.tsx`, `.rthink-*` в components.css)
- [x] **B20.2** Recap v2-канон: GFM task-list → `.md-tasks`+`.chk` (display-only) в `Markdown.tsx`; emoji-категории ✅/💡/📝 → локализованные `` `код-лейблы` ``-чипы (`recap.rs::RecapLabels`).
- [x] **B20.3** Жирные имена/факты: правило в narrative-промпте + render-side `bold_known_names` (whole-word, longest-first; склонения не матчатся by design) для summary-fallback и key_points. JSON-контракт не тронут.
- [x] **B20.4** Inbox keep-alive: `InboxView` всегда mounted (`active`-prop, display:none), вид/поиск/фасеты/week-month offsets/скролл переживают навигацию; refresh при реактивации. Экстракции: `useInboxRowActions.ts`, `InboxViewSwitcher.tsx` (800-line guard).
- [x] **B20.5** ПКМ context-menu (`ui/ContextMenu.tsx`: portal, clamp, Escape/outside, role=menu) в cards/week/month + `.trow`; общий `CallMenuItems`/`rowCaps` с kebab'ом.
- [x] **B20.6** CallRail: дедуп участников по `contact_id` (`participantGroups.ts`), счётчик = люди, подпись «N голоса в записи».
- [x] **B20.7** CallRail: отвязка голосов (`ParticipantRow.tsx`) — 1 голос = иконка ×, 2+ = dropdown со строками голос+сэмпл (`VoiceSampleButton.tsx`)+×; после unbind refetch + предложение regen рекапа.
- [x] **B20.8** Транскрипт follow-режим: автоскролл к активной реплике; ручной скролл (wheel/touch/pointer/keys) выключает; кнопка-crosshair «к текущему участку» в плеере включает обратно (только она).
- [x] **B20.9** Fix off-by-one: общая граница смежных реплик резолвится в следующую (`lib/transcriptActive.ts`, exclusive end + SEEK_EPS).
- [x] **B20.10** Движок/локальность убраны из call-detail UI (header-чип, строка «Движок», engine-label в fail-баннере; dead `engineLabel.ts` удалён). Остались Settings + Onboarding. Тип звонка остался.

## B21 · Settings standardization (2026-07-21)

> Аудит нашёл 66 расхождений (3 layout-грамматики, 4 стиля хинтов, 3 самописных прогресс-бара, битые классы, dead i18n). Канон — `wk-settings.jsx` прототипа.

- [x] **B21.1** Примитивы: `.setting-row` → канон Row (13px + divider + data-align/last/disabled), `ui/Progress` (первый потребитель `.progress`), `ui/GroupLabel`, Button += `danger-ghost`, OptionCard += `radio`, HotkeyCapture (i18n + `.hotkey-readout` + Esc-cancel фикс).
- [x] **B21.2** Shell: aside-rail 300, иконки shield/lock по канону, видимый lede на секцию (`settings.lede.*`), aria-label = nav-label, единый max-width 560, копирайт-синк («Обработка»/«Приватность»/«Полная очистка»).
- [x] **B21.3** Секции на Row-идиоме: Appearance, Account (danger-ghost выход), Processing (OptionCard local-first + sunken hw-plate + GroupLabel'ы + канон-статусы set-table + квота на Progress вместо legacy Card/Badge/UsageBar), Permissions (Chip у лейбла, primary «Запросить», IconBtn'ы, глиф ↻ выпилен), Запись (3 группы Row), Спикеры (компакт-Panel модуля + ⊕ threshold-Select `AUTO_BIND_THRESHOLD` + pyannote-прогресс), Labs, Maintenance (один Row c inline-состояниями), Privacy (Row + Chip «удалено»).
- [x] **B21.4** Onboarding engine-step: OptionCard-пресеты, Progress, Button (битые btn--quiet/btn--sm убраны), hooks-order фикс (crash при старте загрузки).
- [x] **B21.5** Гигиена: 49 dead i18n ×3 локали, dead `LOCAL_ENGINE_ANNOUNCEMENT_*`, useTheme → SETTINGS_KEYS, mic-diarization default выровнен на backend-истину (OFF, тумблер больше не врёт), Rust-owned keys doc-блок в settings.ts.
- [ ] **B21.6** Follow-up: roving-tabindex / стрелки для OptionCard-radiogroup (WAI-ARIA APG); WeSpeaker-строка в хранилище моделей.

## B22 · Settings polish (фидбек юзера после B21, 2026-07-21)

- [x] **B22.1** Rail секций 300 → 220px (+скелетон) — верстка справа больше не ломается.
- [x] **B22.2** Lede-абзацы секций убраны (SectionShell = aria-label + ширина); `settings.lede*` удалены ×3 локали.
- [x] **B22.3** Хинты сокращены до осмысленных: убраны languageHint / privacy-простыня call-detect / cooldownHint; sttLang/sttRecapLang/hotkeyToggle/callDetect — короткие редакции.
- [x] **B22.4** «Обслуживание» (bulk recap) удалено из UI; Rust-команды `regenerate_empty_recaps`/`cancel_bulk_recap` и события `recap:bulk_*` остаются без фронт-потребителя (вернём при надобности).
- [x] **B22.5** Таблица хранилища: имя модели `.u-trunc`+title (конец наездов), lastUsed 84→70; человеческие лейблы для `qwen25-0_5b` («Ускоритель саммари · 0.5B») и `silero-vad-v5` («Детектор речи»).

## B23 · Contacts polish + sync-ready schema (2026-07-21)

- [x] **B23.1** Миграция 0018: contacts += `source('local')/external_id/external_etag`, identifiers += `label` + дедуп исторических дублей + UNIQUE(contact_id,kind,value) + partial-UNIQUE(source,external_id). Точка расширения импорта зафиксирована (паспорт M6.4, sync не реализуется).
- [x] **B23.2** `update_contact`: replace-all → **diff-preserve** (стабильные id идентификаторов, label in-place, normalize+dedup payload'а). 8 новых cargo-тестов, включая raw-прогон 0001→дубли→0018. TODO(M6.4): политика нормализации case для value.
- [x] **B23.3** ContactFormModal — канон v2 AddContactModal (Modal 480, парные поля, footer ghost/primary c form-атрибутом, Switch consent, kind-Select переведён, error/busy внутри диалога); импорт-заглушки сознательно не показываем. ContactsPage: панель view-only, add/edit только модалкой; ContactView на Button-обёртках.
- [x] **B23.4** i18n: contacts.kind.*/editTitle ×3; −12 orphan-ключей ×3. RTL: ContactFormModal (6) + ContactsPage (4, включая failure-path в диалоге).
- [x] **B23.5** Фидбек: vCard-плашка из формы убрана; VoiceSamplesSection переверстан под канон (panel + lrow + IconBtn play/pause/trash вместо глифов ▶/❚❚/×, дубль-заголовок и техно-мета качество/байты/call-id выпилены).

---

## Уверенно НЕ делаем

> Rejected by design — см. раздел 12 паспорта + §«Принятые ограничения» ниже.

- **R3 deviation — auto-detect «идёт звонок»** как автозапуск. Запись всегда manual trigger; opt-in Labs-подсказка (Core Audio + frontmost-app whitelist) реализована (S-секция), но это подсказка, не автозапуск.
- **R11 live realtime captions.** Local STT offline-only. Chunked 10-мин post-processing (M13) допустим, live — нет.
- **Auto-fallback Cloud → Local** при cloud LLM fail. Risky — переключение движков требует явного user consent.
- **Distributed chunk processing** (multi-process). Overkill для desktop.

---

## Принятые ограничения (НЕ «чинить» в MVP)

См. раздел 12 паспорта. Здесь только маркеры — детали и причины там.

| Маркер | Что |
|---|---|
| R1 | Free-тир абьюзится переустановкой |
| R2 | LLM-догадка спикеров — только booster |
| R3 | Авто-детект звонка не делаем |
| R4 | Windows-захват = `unimplemented!()` |
| R5 | Биллинг = заглушка |
| R6 | macOS-сборка без Apple-нотаризации |
| R7 | Free Cloudflare без auto-апгрейда тарифа |
| R8 | Аудио НЕ через память воркера |
| R9 | Local-движок в MVP — только macOS (M1.4 / R4 для Win/Linux) |
| R10 | Модели не бандлятся в installer (~50MB), download по требованию |
| R11 | Real-time / streaming local STT (live captions) — НЕ делаем в MVP. Chunked post-processing с pipelining (M13) допустим и запланирован — он не нарушает offline-only характер STT, только разрезает входной аудио-файл на 10-мин куски для UX-выигрыша. |
| R12 | Качество local-LLM саммари ниже cloud — UI показывает «●●○» явно |
| R12-bis | Авто-удаление моделей при смене preset — НЕ делаем (explicit storage UI) |
| R13 | Слишком слабое железо НЕ блокирует Local — показывается с warning |
