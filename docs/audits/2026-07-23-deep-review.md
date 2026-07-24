# Wotold — глубинный аудит репозитория
**Дата:** 2026-07-23 · **Ветка:** claude/repository-deep-review-f24b00 (worktree)
**Метод:** 10 параллельных ревью-агентов по слоям + adversarial-верификация critical/high находок + ручной инлайн-аудит proxy/MCP/contracts + рыночный ресёрч (WebSearch, июль 2026). Один false positive отсеян верификатором. Каждая находка привязана к реальному file:line.

---

## TL;DR

**Общая оценка: 7/10.** Редкий по дисциплине solo-проект: ~96k строк за 2 месяца, при этом 13 unwrap на весь прод-Rust, ноль дублей контрактных типов, честные coverage-гейты, археология инцидентов в комментариях, роадмап без единого фантомного чекбокса. Ядро (пайплайн, БД, ассистент) — крепкий прод-код. **Но:** релизный пайплайн сломан и ни разу не прогонялся (CRITICAL), есть path traversal с удалением всей базы данных из webview, пауза записи не останавливает захват звука (privacy-дыра), README врёт о статусе продукта, лицензии нет. Продукт «работает у владельца» — до «можно дать чужому» лежит вся секция A роадмапа плюс находки ниже.

---

## 1. Размер и метрики

| Слой | LOC (без тестов) | Тесты |
|---|---|---|
| Rust-ядро (src-tauri) | 46 780 | 810 test fn (~1300 unwrap в тестах — норм) |
| Frontend TS/TSX | 28 857 | ~750 vitest в 86 файлах (10.5k строк) |
| Proxy (CF Workers) | 1 994 | unit + workers-pool integration |
| MCP-сервер | 508 | 18 тестов |
| Contracts | 529 | serde round-trips |
| Swift sidecar | 1 193 | — (0 тестов) |
| CSS | 4 032 | — |

- **451 коммит за 2 месяца** (19.05–23.07), соло. Скорость ~1600 LOC/день с тестами — очень высокая, качество при этом не типично-вайбкодерское.
- Гигиена: 12 TODO/FIXME на весь репо, `any` — 2, unsafe — 6, prod-unwrap — **13** (10 из них в eval-харнесе). Верхний процентиль дисциплины.
- Зависимости скромные: 55 crates, **12** npm-зависимостей фронта (ни Redux, ни UI-кита — всё своё).
- Горячие точки churn: `lib.rs` (73 правки), i18n ×3 (71), `pipeline/mod.rs` (68, вырос до 2797 строк), `CallDetailPage.tsx` (63).
- 21 миграция SQLite, WAL + busy_timeout + integrity_check с карантином.

## 2. Оценки по слоям

| Слой | Оценка | Одной строкой |
|---|---|---|
| Rust: пайплайн | 7/10 | Отличная crash-recovery и degrade-don't-die; mod.rs-монолит и дубли local-recap |
| Rust: БД | 7/10 | 100% параметризация, честные каскад-тесты; но path traversal в call_store |
| Rust: local engine | 7/10 | Трёхслойная защита сайдкаров, UTF-8-инцидент закрыт тестом; llama-server без auth |
| Rust: ассистент/RAG | 7/10 | 4 детерминированных слоя до LLM — зрелый дизайн; роутер даёт ложные перехваты |
| Аудио + Swift | 6/10 | Слабейший слой: error-таксономия, пауза, гонки в ProcessTapRecorder |
| Frontend | 7/10 | Гонки чинятся системно после ревью; i18n дырявый на ошибках, нет memo |
| Дизайн-система | 7/10 | Токен-дисциплина реально enforced; focus-visible и контраст — долг |
| Contracts + CI | 6/10 | Контракты образцовые; релизный workflow сломан, хук мёртв |
| Тесты | 7/10 | Листья покрыты параноидально, glue — нет; sleep-паттерн |
| Docs + продукт | 7/10 | Паспорт/роадмап — эталон; README врёт, лицензии нет |
| Proxy (ручной аудит) | 8/10 | CORS/rate-limit/scrub/R8 — всё честно |
| MCP (ручной аудит) | 8/10 | Read-only гарантирован конструкцией |

---

## 3. Что сделано сильно (по фактам)

1. **Crash-recovery как система, а не заплатки.** `reconcile_orphan_recordings` чинит зависшие записи по фактическому размеру WAV; `auto_recover_interrupted_calls` с анти-луп маркером и cap; chunk-строки реконструируются с диска. Живой инцидент 23.07 (краш WKWebView посреди пайплайна) закрыт в B28 в тот же день с регресс-тестами.
2. **«Археология инцидентов» в комментариях.** Каждый прод-баг оставил фикс + регрессионный тест + комментарий с механикой ([M13 fix], [P-fix5], кейс 3df01365). Лучшее противоядие от «улучшения обратно».
3. **Контракты — единственный источник истины на деле:** ноль дублей типов вне `packages/contracts`, 31 файл-потребитель, deploy-триггер прокси включает contracts.
4. **Ассистент компенсирует слабость 3B-модели структурой:** классификатор → интент-роутер → темпоральный префильтр → recap-путь мимо FTS — четыре детерминированных слоя до LLM. Инъекции закрыты конструктивно (токенизация MATCH, нейтрализация делимитеров, модель физически не генерирует call_id). M16: 2/20 → 18/18 живых вопросов — измеренный, а не заявленный прогресс.
5. **Тесты на реальном SQLite, а не моках:** миграция 0018 тестируется на грязных legacy-данных, конкурентность — реальными гонками, audio_merger — 8 конкурентных merge. Соотношение assert-on-result/assert-on-mock здоровое (1096 expect vs 74 toHaveBeenCalled).
6. **CI посчитан в долларах** («6 jobs × $0.08/min → экономия ~70%»), coverage-гейты честные (50% Rust enforced, а не фейковые 80%).
7. **Токен-дисциплина дизайна реально работает:** rg по сырым hex во всём src находит только осмысленные исключения; светлая/тёмная темы 1:1, спикер-палитра формально изолирована.

---

## 4. Подтверждённые проблемы — блокеры и security

### CRITICAL

**C1. Релизный workflow сломан — дубликат ключа `args:`** — `.github/workflows/release-app.yml:124` и `:142`. Два `args:` в одном `with:`-блоке tauri-action: `--features voice-onnx` и `--target universal-apple-darwin`. Пуш первого же тега `v*` даст invalid workflow (или, при last-wins, прод-DMG молча соберётся **без биометрического матчинга**). Баг латентный: git tag пуст, workflow ни разу не исполнялся. Фикс — одна строка: слить в `args: --features voice-onnx --target universal-apple-darwin`.

### HIGH — security

**H1. Path traversal с удалением всей базы** — `apps/desktop/src-tauri/src/call_store.rs:97`. `call_dir()` делает `calls_root().join(call_id)` без валидации; Tauri-команда `delete_call` принимает id из webview. `id=".."` → `remove_dir_all(app_data_dir)` — вся БД, все записи. `read_call_artifact` уязвим так же на чтение. Ирония: `local_engine` имеет `ensure_path_under` defense-in-depth, а call_store — самый простой её потребитель — нет. Тот же класс в MCP: `services/mcp/src/tools.ts:51` не валидирует `call_id` как UUID (там read-only и ограничен именами файлов — MEDIUM). Фикс: UUID-regex на call_id в обоих местах.

**H2. Пауза не останавливает запись** — `apps/desktop/src-tauri/src/commands/recording.rs:570`. Sidecar не знает о паузе: mic и system продолжают писаться, транскрибируются и попадают в саммари. Комментарий оправдывается «тишина отрежется» — но юзер жмёт паузу именно чтобы сказать приватное, это речь, а не тишина. Прямое нарушение ожидания согласия (C1-смежное), НЕ покрыто R1–R13. Плюс duration вычитает паузы, а аудио их содержит — таймстемпы едут. Минимум: wire `pause`/`resume` в sidecar или честный warning в UI.

**H3. Транзиентная ошибка ротации убивает часовую запись** — `apps/desktop/src-tauri/src/audio/macos.rs:327`. Dispatcher трактует любой `{"event":"error"}` как терминальный, но Swift шлёт его и для non-fatal «rotate failed», после которых запись продолжается. Итог: один transient → stop получает старую ошибку → звонок помечен failed при целых WAV. Нужна error-таксономия в протоколе (fatal vs operational).

**H4. Резидентный llama-server — фиксированный порт 47331 без аутентификации** — `apps/desktop/src-tauri/src/local_engine/llm_server.rs:27`. Любой процесс на машине может дёргать `/completion` (бесплатный inference + DoS очереди), а процесс, занявший порт раньше и отвечающий на `/health`, получит все промпты с транскриптами. CLI-путь получил все W5-митигации (0600, O_EXCL, path checks) — server-путь при переезде фичи их не унаследовал. Фикс: `--api-key` со случайным ключом + случайный порт.

**H5. Партнёрские ключи — repo-wide GitHub Secrets** — `.github/workflows/sync-proxy-secrets.yml:38`. Джоба sync не привязана к GitHub Environment — прямое нарушение собственного S1 («никаких repo-wide секретов для этих значений»). Любой изменённый workflow и любой из непиненных actions имеет к ним доступ.

**H6. Ни один из 13 actions не запинен по SHA**, включая `tauri-apps/tauri-action@v0` (mutable major-тег!), который получает `TAURI_SIGNING_PRIVATE_KEY`. Компрометация тега = кража ключа подписи автообновлений = supply chain на всех юзеров. Фикс: environment-scoped secrets + SHA-пины.

**H7. Ad-hoc codesign не работает** — `.github/workflows/release-app.yml:148`. Подпись выполняется ПОСЛЕ того как tauri-action уже собрал DMG и загрузил в Release — в опубликованный артефакт она не попадает. Заявленная митигация «damaged, move to trash» на macOS 14+ фактически отсутствует. Вместе с C1: **релизный путь — единственный непрокатанный участок CI и одновременно самый нагруженный секретами**. Прогнать dry-run на тестовом теге до первого релиза — обязательно.

### HIGH — прочее подтверждённое

- **Шрифты с Google Fonts CDN** — `apps/desktop/src/styles/fonts.css:10`. Local-first приложение без сети теряет всю типографику + шлёт запрос в Google при каждом запуске (fingerprint для privacy-продукта). Файл сам содержит готовый OPTION B (self-hosted) — просто не включён.
- **`substring_fuzzy_score` — комментарий врёт про cap** — `apps/desktop/src-tauri/src/pipeline/summary_validator.rs:157`. «m, n ≤ 200 (quote length cap)» — cap нигде не enforced; sliding-window Levenshtein O(len·n²) синхронно на tokio-worker. Длинная фабрикованная цитата от LLM = секунды блокировки UI-потока.
- **post-write.sh — мёртвый хук** — `scripts/hooks/post-write.sh:14`. Читает несуществующий `CLAUDE_FILE_PATH` (payload приходит JSON'ом на stdin, как корректно делают три соседних хука) → cargo fmt/check и tsc **никогда не запускались**, хотя CLAUDE.md заявляет их активными. Тихий no-op с мая.
- **Роутер ассистента перехватывает контентные вопросы** — `apps/desktop/src-tauri/src/assistant/router.rs:158`. «Какие решения приняли на встрече?» → срабатывает ListCalls («какие»+«встрече») → юзер получает список звонков, LLM не вызывается. Direct-ответ безальтернативен, негативные тесты содержат только «что…»-формулировки. Ровно класс фейлов, ради которого M16 строился.
- **Источник истины дизайна — в `~/Downloads`** — `CLAUDE.md:72`. Design gate физически непроходим на любой другой машине; прототип не версионируется и может уехать в корзину. Вендорить snapshot в `docs/design/`.
- **16 реальных `sleep()` в тестах оркестратора** — `apps/desktop/src-tauri/src/pipeline/chunk_orchestrator.rs:487+` (+ resource_queue, call_detect, 1100ms в lifecycle). Flaky-мина под нагруженным CI + ~3s к каждому прогону. Правильный паттерн уже есть в кодовой базе (retry.rs инжектирует sleep) — не обобщён.
- **`/v1/llm` — ноль route-level тестов** при образцовом покрытии `/v1/stt`. Если сломается порядок middleware — квота llm перестанет списываться и никто не заметит.

### Находки ручного аудита (proxy/MCP)

- `POST /v1/stt` не проверяет префикс `stt/<deviceId>/` у r2Key — девайс может транскрибировать чужой staged-объект, зная ключ (MEDIUM, знание ключа = знание чужого UUID×2).
- Session-id в deep-link `wotold://` — перехват другим приложением, зарегистрировавшим схему (отложенный риск: аккаунт пока ничего не даёт, но задокументировать до облачной синхры).
- `findContactsByName` не эскейпит LIKE-wildcards — непоследовательно с `searchCalls` (LOW).
- Хорошее: CORS-allowlist, /16 rate-limit, content-type allowlist на presign, скраббинг UUID/ключей/query из ошибок, R8 соблюдён буквально (байты не через воркер), KV-resume джобов под 30s-лимит Workers Free.

---

## 5. Важные MEDIUM (сокращённо)

**Rust:** ошибка `mark_call_ready` протекает через `?` мимо `pipeline_finished` — UI виснет в processing (`pipeline/mod.rs:323`); insert-after-spawn race в реестре задач блокирует regen до рестарта; `extract_clusters` (сотни ONNX-инференсов) синхронно в async без spawn_blocking; карантин corrupt-БД забывает `-wal/-shm` — старый WAL вольётся в свежую базу; `insert_speaker_suggestions` снесёт confirmed-привязки когда `identify_speakers` подключат (мина под R2); PLACEHOLDER-SHA256 у silero-vad и qwen-0.5b — модели недокачиваемы, integrity выключен; гонка параллельных download одного id (SHA считается по потоку, не по файлу); паника на byte-slice stderr не по границе UTF-8 в error-path.

**Frontend:** ошибка любого действия (экспорт/удаление) уничтожает всю страницу звонка (`CallDetailPage.tsx:384`, подтверждено); initial-load без отмены — stale race при смене звонка; **все ~40 сообщений об ошибках захардкожены по-русски в обход типизированного i18n** (en/kk юзер видит happy-path переведённым, все ошибки — на русском); «Голос N»/«Звонок…» тоже ru-only; Tabs: roving tabindex без стрелочной навигации — вкладки недостижимы с клавиатуры; транскрипт ре-рендерится целиком 4–8 раз/сек при playback (ни одного React.memo в кодовой базе); stale closure на `semanticSearch` — эмбеддер не докачивается после включения тумблера; error инбокса никогда не сбрасывается после успеха.

**Дизайн:** примитивы wk.css (btn/iconbtn/tab/switch/menu) без `:focus-visible` — а комментарий global.css утверждает обратное; `--text-faint` (≈2.5:1) на несущем тексте (заголовки таблиц, таймкоды); Switch 34×20 — ниже таргета 24px WCAG 2.5.8 и без focus-ring; три параллельных shimmer-системы; skeleton в legacy-сетке против v2-контента — layout-jump.

**Docs:** README называет продукт «pre-MVP» и утверждает «local-движок ещё не активирован» (реализован давно, M12–M16 закрыты) и «саммари через 10–30 секунд» — **единственный внешний документ врёт в обе стороны** (проверено лично). Паспорт: managed-LLM по факту Groq llama-3.3-70b, а не Anthropic; M15/M16 (крупнейший пласт продукта) в паспорте отсутствуют — «источник истины» дрейфует в исторический документ. **LICENSE-файла нет** («TBD») при инструкции скачивать DMG из публичных Releases — для privacy-ЦА это блокер доверия.

---

## 6. Системные наблюдения (важнее отдельных багов)

1. **Тестовая пирамида инвертирована на glue.** Листья покрыты параноидально, а `pipeline::run` / `run_local_inner` / `recover_chunked_call_inner` / `useCallDetail` — центральная оркестрация — живут на ручных live-запусках. Все три M13-бага были именно в glue. Тестовая стратегия де-факто реактивная (тесты появляются после инцидента), а не TDD из деклараций харнесса.
2. **Качество неоднородно по возрасту кода.** Новые слои (assistant, chunks) имеют FSM-гейты и конкурентные тесты; старый `lifecycle.rs` — нет (finish/fail без status-гейта). AudioRecorder прошёл итерации конкуренционных фиксов — его близнец ProcessTapRecorder не получил ни одного (stop вне queue, нет idempotent-guard). Пары «одинаковый контракт, разная зрелость» — главный источник будущих багов.
3. **Наблюдаемость degrade-путей отсутствует.** Политика «degrade, don't die» проведена последовательно, но degraded-состояния живут только в логах: юзер не отличает «system-трек ушёл в speaker:0» от «правда один голос».
4. **`mod.rs` 2797 строк** — периферия декомпозирована отлично (35 модулей), но оба engine-роута и их хелперы остались в корне, local-recap продублирован трижды, настройки парсятся двумя путями с **уже разъехавшимися** clamp'ами (num_speakers 1..=3 vs 1..=4).
5. **Ratchet не крутится.** Frontend coverage-gate 10% при 599 тестах: удаление 80% тестов CI не заметит. Честнее фейковых 80%, но ratchet-механизм ручной и стоит на месте.
6. **ROADMAP — образец инженерного лога** (счётчики тестов, вердикты ревьюеров, метрики live-гейтов, нулевой дрейф от кода — проверено выборочно по 7 пунктам), но формат «плотный шифр» не масштабируется на команду >1.

---

## 7. Продукт и рынок

Проверено живым поиском (июль 2026) + анализ кода.

**Тренд — за вами.** Рынок AI-заметок в 2026 официально расколот на две архитектуры: bot-based (Otter, Fireflies, Read AI — бот в звонке, аудио в чужом облаке) и **botless local capture** (Granola, Meetily) — второй сегмент растёт именно потому, что компании банят ботов, а compliance проще одобряет локальный захват. Wotold архитектурно в правильном лагере. Отдельно: MacWhisper ($69 one-time) и superwhisper ($249 lifetime) доказали платёжеспособный спрос на «локально и без подписки» на macOS.

**Что реально дифференцирует Wotold** (по коду, не по обещаниям):
1. **Полностью локальный E2E-пайплайн** — запись → STT → диаризация → саммари → RAG-ассистент без единого сетевого вызова. Granola шлёт в облако LLM; MacWhisper не пишет звонки и не имеет ассистента; Otter/Fireflies — облако целиком. Такой комбинации в одном продукте на рынке нет.
2. **Диаризация + голосовые отпечатки контактов с подтверждением** — ровно боль из §1 паспорта («Notion не различает голоса»); cloud-диаризация у конкурентов есть, но накопление голосовой биометрии контактов локально — нет.
3. **MCP-сервер** — прямая ставка на экосистему Claude; у конкурентов интеграции = Zapier/CRM, «спроси свои звонки из Claude Desktop» — ниша, которую пока никто не занял.
4. **RU/KK code-switching** — Otter/Fireflies/Granola обслуживают этот сегмент плохо; для рынка Казахстана/СНГ это реальное преимущество.

**Что commodity:** саммари/MoM/action items — стол-ставка (у Fathom бесплатно и за 30 сек); чат по звонкам — есть у всех облачных.

**Риски, честно:**
- **Платформенный:** Zoom/Teams/Meet встраивают транскрипцию бесплатно; Apple Intelligence движется к системной транскрипции звонков. Окно для нишевого продукта — 1–2 года, защита — локальность + кросс-приложенческий захват + накопленный архив с RAG.
- **Качество на слабом железе:** R12/R13 честно приняты, но конкуренция с cloud-качеством Otter (WER 6.3%) на 3B-модели — постоянное «●●○». Гибрид BYO-ключей — правильный ответ, но UX-сложность.
- **Правовой:** two-party consent (Калифорния и ещё ~10 штатов, ЕС) — продукт пишет системный звук без уведомления второй стороны. У ботов consent встроен самим появлением бота в звонке. Нужен хотя бы FAQ/уведомление в онбординге — сейчас нет ничего.
- **Дистрибуция — слабейшее звено:** без нотаризации (R6), без лицензии, с мёртвым updater (#42), с враньём в README. Для ЦА «privacy-чувствительные профессионалы» каждый из этих пунктов — минус к доверию, которое и есть ваш продукт.

**Вердикт по нише:** ниша есть и растёт, ЦА узкая, но реальная (macOS + privacy + RU/KK). Продукт технически глубже большинства инди-конкурентов. Критично для конкурентоспособности: закрыть дистрибуцию (подпись/нотаризация/лицензия), consent-историю, и английский онбординг — сейчас README на русском при UI ×3 локали.

**Источники рынка:**
- https://www.useluminix.com/reports/industry-analysis/ai-meeting-notes-comparison-granola-vs-otter-vs-fireflies-vs-fathom-2026
- https://get-alfred.ai/blog/best-ai-meeting-notetakers
- https://meetily.ai/blog/best-meeting-notes-software-2026
- https://www.getvoibe.com/resources/macwhisper-review/
- https://max-productive.ai/ai-tools/superwhisper/
- https://www.granola.ai/blog/best-transcription-apps-for-mac-in-2026-compared-by-use-case

---

## 8. Приоритеты (если делать по порядку)

1. **release-app.yml**: слить `args:`, перенести codesign до аплоада, SHA-пины на actions, dry-run на тестовом теге. Полдня — и релизный путь перестаёт быть миной.
2. **UUID-валидация call_id** в `call_store` + MCP (path traversal). Час работы.
3. **Пауза**: wire в sidecar или честный UI-warning. Privacy-обещание продукта сейчас нарушено.
4. **Секреты**: environment-scoped партнёрские ключи; `--api-key` для llama-server.
5. **fonts self-hosted** (OPTION B уже написан) + LICENSE + переписать README.
6. **Error-таксономия sidecar-протокола** (rotate_error ≠ fatal) — защищает часовые записи.
7. **i18n ошибок и меток спикеров** — весь unhappy-path сейчас ru-only.
8. **Роутер ассистента**: guard «остались ли контентные токены» для Stats/ListCalls.
9. Планово: интеграционные тесты на recovery-glue, ratchet coverage, `:focus-visible` в wk.css, декомпозиция `run_local_inner`.

---
---

# ПРИЛОЖЕНИЕ A — Полный дайджест ревью по слоям
*(сырые результаты 10 ревью-агентов: strengths / findings / insights / отклонённые верификатором; severity-метки: CONFIRMED = прошло adversarial-верификацию, unverified = medium/low, верификация не запускалась)*


# AREA: rust-pipeline — score 7/10
## Area label: Rust — пайплайн обработки звонков (apps/desktop/src-tauri: pipeline/, commands/recording.rs, state.rs, events.rs, services/pipeline_runner.rs)

## Strengths
- Ноль production unwrap/expect: все unwrap'ы в pipeline/mod.rs, chunk_runner.rs, chunk_orchestrator.rs, recap.rs, recording.rs лежат внутри #[cfg(test)] модулей (проверено rg по всем файлам области). resource_queue.rs:124 даже обрабатывает mutex poisoning через PoisonError::into_inner вместо unwrap.
- Глубокая crash-recovery история: reconcile_orphan_recordings (recording.rs:246) чинит зависшие 'recording' по фактической длине WAV, auto_recover_interrupted_calls (recording.rs:1359) с анти-луп маркером .auto-recover-tries (max 2 попытки) и cap 3 звонка за старт, chunk_recovery::reconstruct_chunk_rows реконструирует chunk-строки из on-disk WAV. sweep_stale_calls + reconcile на старте (state.rs:80-99).
- Чёткий chunk-FSM: pending→processing→done|failed с INSERT OR IGNORE (db/chunks.rs:46), guarded-переход failed→pending, и чистые тестируемые pure-fn решения — plan_final_chunk (recording.rs:420), pick_pinned_lang (chunk_runner.rs:381), sidecar_write_paths (recording.rs:682) — все покрыты unit-тестами.
- Оркестратор спроектирован channel-first и тестируем через mock-каналы: chunk_orchestrator::run принимает rotate_fn/enqueue_fn closures + 4 канала, никакого shared state; drain_pending с per-task timeout 300s (chunk_orchestrator.rs:334) защищает от зависшего whisper-cli; pause-accounting с capped shift чтобы бесконечная пауза не блокировала ротацию навсегда (chunk_orchestrator.rs:260-280).
- Дисциплина non-fatal degradation: диаризация, cluster pipeline, auto-bind, recap, embeddings — все деградируют с warn-логом, роняет пайплайн только STT (owner-трек). Recap-blank guard (recap.rs:378) не даёт слабой local-модели молча персистить пустой рекап — возвращает Err до любых DB-записей.
- Тонкая математика пауз в stop_recording (recording.rs:335-358): учёт «висящего» окна паузы при stop-во-время-паузы, чтобы min-duration гейт и duration_sec не завышались — с полным rationale в комментарии.
- Anti-hallucination валидатор обнуляет фабрикованные цитаты, не удаляя сами пункты (summary_validator.rs:324-356) — с явным обоснованием, почему items без evidence сохраняются (local-модели не дают цитат by design).
- Blocking-код в целом корректно уводится с executor'а: audio_merger через spawn_blocking (mod.rs:1147), SortformerDiarizer.diarize_real внутри spawn_blocking (local_engine/diarization.rs:215).

## Findings

### [MEDIUM|unverified] run(): ошибка mark_call_ready «протекает» через ?, пропуская pipeline_finished emit — UI зависает в processing
apps/desktop/src-tauri/src/pipeline/mod.rs:323
В success-ветке match внутри run() стоит `db::mark_call_ready(pool, &ctx.call_id).await?;` (line 323). Если этот UPDATE упал (busy pool, disk full), `?` выходит из функции ДО `bus.pipeline_finished(&event)` (line 352) и без fail_recording_with_reason. Результат: пайплайн фактически успешен (транскрипт/рекап на диске), но статус звонка навсегда 'processing' до следующего рестарта (sweep_stale_calls пометит его failed — что тоже ложь), а фронт не получает ни finished, ни failed событие. Err-ветка того же match аккуратно делает и persist reason, и event — асимметрия только на success-пути.
FIX: Заменить `?` на match: при ошибке mark_call_ready логировать и эмитить PipelineFinishedEvent{status:"failed", reason} (или retry), но всегда доходить до bus.pipeline_finished.

### [MEDIUM|unverified] Insert-after-spawn race в реестре pipeline_tasks: stale handle навсегда блокирует spawn_regen
apps/desktop/src-tauri/src/services/pipeline_runner.rs:337
spawn_task: task спавнится (line 330-336), в конце сам удаляет себя из map (`tasks_for_task.lock().await.remove(...)` line 335), а регистрация происходит ПОСЛЕ спавна (line 337 `tasks.lock().await.insert(call_id, handle)`). Если task завершился быстро (мгновенная ошибка pipeline, например missing preset) и выиграл гонку за lock, его remove — no-op, затем insert кладёт handle уже завершённого task'а, который никто не удалит. Тот же паттерн в spawn_regen (insert на line 222, self-remove на line 220). Последствие для spawn_regen критичнее: guard `tasks.lock().await.contains_key(&call_id)` (line 147) после этого ПОСТОЯННО возвращает «call_already_processing» для этого звонка до рестарта приложения (spawn_reprocess не страдает — он abort'ит и удаляет найденный handle, line 102-105).
FIX: Регистрировать call_id в map ДО spawn (insert placeholder / занять слот под lock, затем спавнить и обновить), либо в guard проверять handle.is_finished() и вычищать stale-записи.

### [MEDIUM|unverified] extract_clusters (WAV I/O + ONNX inference на каждый сегмент) выполняется синхронно в async-контексте без spawn_blocking
apps/desktop/src-tauri/src/pipeline/mod.rs:2095
clusters::extract_clusters — синхронная функция: на каждый сегмент читает кусок WAV с диска (clusters.rs:59 read_wav_segment) и гоняет ONNX-эмбеддер (clusters.rs:82 embedder.extract). Вызывается напрямую из async-функций: run_cluster_pipeline (mod.rs:2095), relabel_owner_on_mic_full_file (mod.rs:1610), и chunk_runner::build_chunk_embeddings_json (chunk_runner.rs:334, вызов из async run_chunk на line 248). Для часового звонка это сотни ONNX-инференсов — секунды блокировки tokio worker-треда, на котором крутятся и Tauri-команды UI. Непоследовательно с соседним кодом: SortformerDiarizer корректно уводит inference в spawn_blocking (local_engine/diarization.rs:215), а audio_merger обёрнут в spawn_blocking в этом же файле (mod.rs:1147).
FIX: Обернуть вызовы extract_clusters в tokio::task::spawn_blocking (данные и так владеющие: merged: Vec, пути, Box<dyn Embedder>).

### [MEDIUM|unverified] run_local_inner — функция на ~470 строк с тройной дупликацией local-recap логики
apps/desktop/src-tauri/src/pipeline/mod.rs:1064
run_local_inner (mod.rs:1064-1534) — монолит: preset resolve, model check, upload stage, chunked gate + audio merge, full-file STT + language re-pin, диаризация двух дорожек, merge, cluster, auto-bind, recap. Локальный recap-блок (lines 1423-1530) почти дословно продублирован в regenerate_recap_local (lines 806-892): build provider + speaker_prompt_ctx + step_sink + recap_progress wrapper + persist + touch_usage. Провайдер-билд продублирован третий раз: build_local_llm_provider (line 598) существует именно для DRY, но run_local_inner держит собственную копию (lines 1429-1444) — комментарий на line 596 честно признаёт «отдельный backlog на унификацию». Мелкая копия: match llm_id → engine_label дословно повторён на lines 867-872 и 1480-1485. Каждый следующий фикс (как P5.1 atomic label) приходится вносить в 2-3 места — риск drift уже материализовался в истории коммитов.
FIX: Извлечь shared `run_local_recap(pool, ctx, provider, transcript, ...)` и использовать build_local_llm_provider в run_local_inner; label-derivation сделать методом ModelId/preset.

### [LOW|unverified] Гонка stop-сигнала и rotated-события: свежезакрытый chunk может остаться без enqueue, хвост записи теряет транскрипт
apps/desktop/src-tauri/src/pipeline/chunk_orchestrator.rs:162
В tokio::select! (line 162) ветки опрашиваются в случайном порядке. Если пользователь нажал Stop через секунды после ротации, rotated-событие может лежать в rotate_rx непрочитанным, когда сработает stop_rx (line 165) → break. Тогда chunk_idx не инкрементирован, и summary.final_chunk_idx (line 318) укажет на ЗАКРЫТЫЙ chunk k — process_final_chunk корректно STT'ит его (recording.rs:494), но уже открытый sidecar'ом chunk k+1 (несколько секунд аудио в chunks/k+1/) не получает ни chunk-строки, ни STT. Аудио не теряется (audio_merger сканирует диск, mod.rs:1143-1158), но последние секунды разговора отсутствуют в транскрипте без какого-либо warn.
FIX: После break из loop сделать неблокирующий drain rotate_rx (try_recv) и enqueue'ить полученные rotated-chunks до подсчёта final_chunk_idx.

### [LOW|unverified] Ошибки re-STT при language-pin молча проглатываются (if let Ok без else)
apps/desktop/src-tauri/src/pipeline/mod.rs:1253
В run_local_inner при пине языка: `if let Ok(re) = mic_stt.transcribe(&ctx.mic_path, pinned.clone()).await { ... mic = re; }` (line 1253) и аналогично для system (line 1263). Err-ветка отсутствует полностью — ни warn, ни счётчика. Если повторный STT упал (timeout sidecar'а, OOM), звонок молча остаётся с mis-detected языком (тот самый «[FOREIGN] спам», ради борьбы с которым фича и написана), и по логам невозможно понять, что re-STT вообще запускался и не смог. Контрастирует с остальным файлом, где каждый degraded-путь логируется.
FIX: Добавить `Err(e) => log::warn!("re-STT mic pin failed: {e}")` в обе ветки (match вместо if let Ok).

## Insights
- Архитектурная политика «degrade, don't die» проведена последовательно: фатален только STT owner-дорожки, всё остальное (диаризация, кластеры, auto-bind, recap, embeddings) деградирует с warn. Обратная сторона — наблюдаемость целиком висит на логах: у звонка нет персистентного списка degraded-состояний, и «система-трек ушёл в speaker:0» неотличим для пользователя от «правда один голос» (авторы сами это признают в диагностических комментариях mod.rs:1736-1738).
- Chunked и non-chunked пути сходятся в одной точке (run_local_inner): audio merge сканирует ДИСК, а assembly читает DB — благодаря этой развязке recovery-пути (recover_chunked_call, auto_recover, retry_chunk auto-resume) переиспользуют обычный пайплайн 1:1 вместо собственных веток. Это лучшее дизайн-решение слоя: chunk_recovery промоутит root→chunks/0 и дальше всё идёт «как обычная запись после stop».
- Оркестратор — единственный select-loop без shared mutable state (все эффекты через closures + каналы), что делает его полностью тестируемым mock-каналами. Слабое место паттерна: rotate_fn await'ится ВНУТРИ select (chunk_orchestrator.rs:296) — на время ротации RMS-события копятся в буфере 256; и нет timeout'а на rotate_pending — если sidecar подтвердил rotate, но rotated-событие потерялось, ротации прекращаются навсегда (запись продолжается одним гигантским chunk'ом).
- Комментарии кодируют историю инцидентов ([M13 fix], [P-fix4..9], кейс 3df01365 с датой) — это редкая по качеству археология, объясняющая КАЖДОЕ неочевидное решение. Но mod.rs стал свалкой маршрутизации: 2797 строк = ~620 тестов + cloud route + local route + диаризация + cluster pipeline + resident-server management. Проблема не в количестве модулей (их 35, декомпозиция периферии отличная), а в том, что оба engine-роута и все их helpers остались в корневом файле.
- Настройки читаются двумя способами: typed PipelineSettings::load (один проход, clamp, edge-cases) — и параллельно сырые db::get_setting строки с дублированной parse-логикой (mic_diarization_enabled парсится независимо в recording.rs:747-752 и mod.rs:1319-1324; num_speakers — в recording.rs:45 с clamp 1..=3 и в mod.rs:1292-1296 с clamp 1..=4 — пороги УЖЕ разъехались).
- Deadlock-риска практически нет: tokio::Mutex'ы держатся коротко, есть явные drop(guard) перед DB-вызовами (recording.rs:85), lock-ordering задокументирован в pipeline_runner.rs:12-13, и ни одного места с вложенным захватом двух Mutex'ов не найдено. Единственная системная слабость конкурентности — bookkeeping реестра pipeline_tasks (insert-after-spawn, finding #3).
- Уровень тестов для инфраструктурного кода необычно высок: оркестратор гоняется через mock-каналы с покрытием pause/resume/idempotency/drain (chunk_orchestrator.rs:364+), WAV-парсер тестируется битыми заголовками, миграции settings — пятью сценариями. При этом сами гигантские маршруты run_inner/run_local_inner интеграционно почти не покрыты — тестируется периферия, а не сборка.


# AREA: rust-db — score 7/10
## Area label: Rust — слой данных (apps/desktop/src-tauri: db/, call_store.rs, secrets.rs)

## Strengths
- SQL-инъекции отсутствуют: 100% запросов параметризованы. Единственные два места динамического SQL безопасны — placeholder-набор в prune_call_speakers_not_in (db/calls/speakers.rs:167-172, все значения через .bind) и QueryBuilder::push_bind в fetch_passages_by_ids (db/assistant_embeddings.rs:135-143).
- Покрытие репозиториев тестами реально высокое, не для галочки: file-based fresh_db (db/mod.rs:121-136) гоняет настоящие миграции; есть тест конкретной миграции на грязных данных (contacts.rs:843 migration_0018_dedups_preexisting_duplicate_identifiers применяет 0001, сидит дубли, применяет 0018), конкурентные тесты (assistant.rs:783 concurrent_get_or_create_yields_single_thread, :802 concurrent_appends_keep_order_idx_contiguous), тесты каскадов и SET NULL (voice_samples.rs:257, :339; lifecycle.rs:903, :943; chunks.rs:707; assistant.rs:557).
- C5 паспорта (остаточные voice_samples) закрыт корректно: delete_call_and_samples (db/calls/lifecycle.rs:649-664) явно удаляет voice_samples по source_call внутри транзакции — при том что FK на них сознательно SET NULL (0003) для случая «звонок удалён, биометрия остаётся у контакта». Оба поведения задокументированы и протестированы.
- FK-дисциплина: миграция 0003 ретрофитит ON DELETE-правила через каноничную create-copy-drop-rename процедуру с PRAGMA foreign_keys=OFF; все новые таблицы (0013 call_chunks, 0015 decisions/open_questions, 0019/0021 assistant_*, 0020 embeddings) объявляют CASCADE сразу; foreign_keys=ON при init (db/mod.rs:83).
- SQLite настроен грамотно: WAL + busy_timeout 5s + integrity_check на старте с карантином corrupt-файла (db/mod.rs:63-88). FTS5 через external-content таблицу с триггерами (0021) — правильный паттерн, и тест assistant.rs:588 проверяет очистку FTS каскадом.
- FSM-гейты статусов chunks через `WHERE status = ?` + rows_affected (db/chunks.rs:79-125) — защита от гонок retry/finish на уровне SQL, с тестами обоих запрещённых переходов (chunks.rs:568, :592).
- secrets.rs — образцовый keychain-слой: закрытый enum провайдеров (нельзя записать произвольный ключ), public API отдаёт только статус наличия, значение читается одним потребителем, идемпотентный delete, ни одного лога значения.

## Findings

### [HIGH|CONFIRMED] Path traversal через call_id: remove_call_dir("..") сносит весь app_data_dir
apps/desktop/src-tauri/src/call_store.rs:186
CallStore::call_dir (line 97) делает calls_root().join(call_id) без валидации. Tauri-команда delete_call (commands/calls.rs:26-28) передаёт id: String из webview напрямую в remove_call_dir → remove_dir_all. id=".." резолвится в calls/.. = app_data_dir и удаляет ВСЮ базу, keychain-независимые данные и записи. read_call_artifact (commands/calls.rs:64-72) уязвим так же (чтение recap.md/transcript.md по произвольному пути). Паспорт (W5) явно требует path-traversal defense-in-depth (ensure_path_under в local_engine) — call_store этой защиты не имеет, хотя обрабатывает те же недоверенные id из webview.
FIX: В call_dir() валидировать call_id (UUID-regex или запрет '/', '\\', '..') и/или canonicalize + ensure_path_under(calls_root) перед remove_dir_all — по аналогии с local_engine.

### [MEDIUM|unverified] Карантин corrupt-БД переименовывает только app.db, оставляя app.db-wal/-shm
apps/desktop/src-tauri/src/db/mod.rs:72
При провале integrity_check переименовывается только основной файл (std::fs::rename(&path, &corrupt_path).ok()). Файлы app.db-wal и app.db-shm от повреждённой базы остаются на месте. При создании новой пустой app.db SQLite попытается восстановить старый WAL против нового файла — фреймы старых страниц могут быть влиты в свежую БД, что даёт мусор/повторную порчу. Вдобавок .ok() глотает ошибку rename: при неудаче код молча продолжает открывать заведомо повреждённый файл, и миграции упадут с невнятной ошибкой.
FIX: Переименовывать/удалять также path-wal и path-shm; ошибку rename логировать и обрабатывать (например, fallback на удаление файла), а не .ok().

### [MEDIUM|unverified] insert_speaker_suggestions удаляет подтверждённые пользователем привязки спикеров
apps/desktop/src-tauri/src/db/calls/speakers.rs:18
DELETE FROM call_speakers WHERE call_id = ?1 AND speaker_tag != 'owner' сносит и строки с confirmed=1. Это противоречит контракту prune_call_speakers_not_in (line 157-176), который бережно сохраняет confirmed-строки («не теряем подтверждённые юзером привязки»), и духу R2 (привязка — священное действие пользователя). Сейчас функция латентна — единственный вызов identify_speakers (identify.rs:121) не подключён к pipeline::run (только тесты), но это мина: как только matching pipeline заработает (#26) и перезапустится на reprocess, все ручные подтверждения будут молча стёрты.
FIX: Добавить AND confirmed = 0 в DELETE (симметрично prune), либо переехать на UPSERT suggestion-полей через set_call_speaker_suggestion.

### [LOW|unverified] confirm_call_speaker: TOCTOU-чтение метаданных вне транзакции + stale auto_bound_at
apps/desktop/src-tauri/src/db/calls/speakers.rs:237
contact_row (line 227) и speaker_meta (line 237-243) читаются ДО pool.begin() (line 245): между чтением cluster_embedding/suggestion_score и INSERT в voice_samples конкурентный pipeline может перезаписать cluster (set_call_speaker_cluster) — в биометрию контакта уйдёт устаревший embedding. Кроме того, UPDATE (line 247) не очищает auto_bound_at: если спикер был авто-привязан, а юзер вручную сменил контакт через confirm (без unbind), поле остаётся — UI будет считать ручную привязку автоматической (баннер «↩ отменить»).
FIX: Перенести оба SELECT внутрь транзакции; в UPDATE добавить auto_bound_at = NULL.

### [LOW|unverified] finish_recording/fail_recording без FSM-гейта по статусу — в отличие от chunks
apps/desktop/src-tauri/src/db/calls/lifecycle.rs:233
UPDATE в finish_recording (line 233-245) и fail_recording_with_reason (line 601-617) выполняются по WHERE id = ?1 без проверки текущего статуса и без контроля rows_affected на legal transition. failed/ready звонок можно перевести обратно в processing поздним вызовом (например, отставший stop-flow после sweep). Для call_chunks та же команда защищена гейтом WHERE status='pending'/'processing' (chunks.rs:104-125) — непоследовательная строгость в одном слое.
FIX: Добавить AND status IN ('recording','processing') + проверку rows_affected с warn-логом, по образцу mark_chunk_*.

### [LOW|unverified] list_calls без LIMIT/пагинации — полный fetch всех звонков на каждый рендер HomePage
apps/desktop/src-tauri/src/db/calls/lifecycle.rs:634
SELECT * (23 колонки) FROM calls ORDER BY started_at DESC без ограничения. Для локального приложения на горизонте года это сотни-тысячи строк, тянущихся целиком при каждом обновлении списка (list_calls дергается на каждый event). ECC-правило «Missing pagination - add LIMIT to queries» здесь применимо буквально; started_at индексирован (0001 calls_started_at_idx), так что keyset-пагинация тривиальна.
FIX: Добавить limit/offset (или keyset по started_at) параметры и виртуализацию на фронте, когда список звонков начнёт расти.

## Insights
- FSM-через-SQL (`WHERE status = ?` + rows_affected) — лучший паттерн этого слоя, но применён только к call_chunks, а не к самим calls. Вероятная причина: chunks писались позже (M13) с учётом опыта гонок; lifecycle.rs — самый старый код. Стоит выровнять.
- Слой имеет двойной источник истины на удаление: SQL-каскад (delete_call_and_samples, транзакция) + файловая система (remove_call_dir), выполняющиеся неатомарно и в порядке «сначала БД». Провал disk-delete лишь логируется (commands/calls.rs:29-31) — orphan-аудио на диске возможно, и ни один sweep его потом не подбирает. Это осознанный компромисс, но GC-скан calls/ против таблицы calls при старте закрыл бы дыру дёшево.
- Migration 0021 демонстрирует зрелый паттерн для derived data: вместо миграции контента — DROP+DELETE и переиндексация штатным startup-backfill'ом. Это же означает, что assistant_passages/embeddings можно всегда безопасно пересобрать — хорошее свойство для восстановления после порчи.
- append_message получает order_idx через подзапрос MAX+1 в той же транзакции (assistant.rs:203) — корректно ТОЛЬКО благодаря single-writer семантике SQLite. Тест конкурентности это фиксирует, но при гипотетическом переезде на Postgres паттерн молча сломается (два INSERT увидят одинаковый MAX).
- replace-all семантика action_items сбрасывает done=0 при каждой регенерации рекапа (action_items.rs:124) — прогресс пользователя по чек-листу теряется by design. Для decisions/open_questions это безболезненно (нет user-state), для action_items — UX-мина, когда появится «Пересоздать саммари» рядом с прожитым списком задач.
- Производное поле processing_via вычисляется в Rust после fetch (Call::with_processing_via, lifecycle.rs:66-79) вместо CASE в SQL — сознательный выбор: логика ветвления по трём полям остаётся тестируемой юнитами и не дублируется в каждом SELECT. Хороший пример «тонкого SQL, толстой модели».
- Тест migration_0018 (contacts.rs:843) — единственный тест, поднимающий миграцию на legacy-данных. Паттерн отличный, но применён один раз; 0003 (перестройка трёх таблиц с копированием) такой проверки не имеет, а именно там наибольший риск потери данных при расхождении порядка колонок в INSERT ... SELECT *.


# AREA: rust-local-engine — score 7/10
## Area label: Rust — локальный движок STT/LLM (apps/desktop/src-tauri/src/local_engine/, embeddings.rs, embeddings_onnx.rs, capabilities/default.json)

## Strengths
- SidecarGuard (local_engine/sidecar.rs) — RAII kill дочернего процесса при timeout/cancel/panic, последовательно используется в llm.rs и stt.rs (release только после Terminated, kill на error/timeout). Известная проблема zombie-процессов из W5 реально закрыта, включая abort через Drop.
- Temp-файлы промптов/грамматик пишутся через write_user_only (llm.rs:519) с O_CREAT|O_EXCL + mode 0o600 — защита и от чтения чужим процессом, и от symlink-подмены; есть тесты на perms (0o600) и на AlreadyExists.
- Известный инцидент UTF-8 хрупкости parse_whisper_json починен: stt.rs:582-587 читает байты + from_utf8_lossy вместо строгого read_to_string, регрессия покрыта тестом parse_whisper_json_survives_invalid_utf8 (stt.rs:1004) с реальным сценарием (сырой 0xFF внутри кириллицы).
- Defense-in-depth по путям выстроен честно: capability-валидаторы в capabilities/default.json все заякорены (^...$), а Rust-слой дублирует контроль — запрет ParentDir-компонентов + ensure_path_under для prompt/grammar/schema/stem (llm.rs:498-513, stt.rs:163-177), с тестами на ../, relative и out-of-prefix.
- Скачивание моделей (models.rs): потоковый SHA256 без 4GB в RAM, atomic rename .partial→final, удаление партиала при mismatch, throttling progress-событий, HTTPS-only тест на весь каталог, идемпотентность download/delete.
- Сериализация тяжёлых ресурсов через pipeline::resource_queue (permit=1, FIFO) в llm.rs:285, stt.rs:302 и diarization.rs:206 — permit в диаризации сознательно переносится внутрь spawn_blocking, чтобы abort не освобождал ресурс раньше реального конца ONNX. Продуманная защита от OOM (известный инцидент 16GB).
- extract_json_object (llm.rs:694) — brace-balancer с обработкой строк/escape и письменным обоснованием UTF-8-безопасности байтовой итерации; edge-cases покрыты тестами (вложенные скобки, escaped quotes, unbalanced).
- Плотное тестовое покрытие с упором на злые входы: normalize_lang отклоняет `../etc` и `ru;rm` (stt.rs:720-725), sanitize_prompt char-aware truncate (не байтовый), proptest-инварианты cosine_similarity в embeddings.rs.

## Findings

### [MEDIUM|unverified] Паника на байтовом срезе stderr не по границе UTF-8 символа в error-path
apps/desktop/src-tauri/src/local_engine/llm.rs:637
stderr_snippet делает `&s[..s.len().min(512)]` по String из from_utf8_lossy. Если stderr длиннее 512 байт и байт 512 попадает внутрь многобайтового символа (кириллица в путях/метаданных GGUF, либо 3-байтовый U+FFFD, который сам же lossy-decode и вставляет), срез паникует «byte index 512 is not a char boundary». Вызывается ровно в аварийных ветках (exit code != 0 и timeout) — вместо чистой ошибки `local_llm_timeout` пользователь получает панику async-таски пайплайна.
FIX: Char-boundary-безопасный срез: `s.char_indices().nth(...)` либо `s.floor_char_boundary(512)` (или простой цикл до is_char_boundary), плюс тест с кириллическим stderr >512 байт.

### [MEDIUM|unverified] PLACEHOLDER-SHA256 записи каталога: недокачиваемые модели + полная потеря integrity-контроля для них
apps/desktop/src-tauri/src/local_engine/models.rs:201
SILERO_VAD (строка 201) и QWEN25_0_5B (строка 151) шипятся с sha256=PLACEHOLDER. Следствия: (1) download() всегда скачивает файл целиком (до 380MB для qwen-0.5b) и затем гарантированно падает VerifyFailed на строке 542 — фичи P15.2 (--vad) и T-16 (speculative decoding) невозможно активировать через штатный UI, обе записи при этом видны в local_engine_list_catalog/Storage UI; (2) check_status_fast (строка 318) для placeholder-записей деградирует до «len>0 → Present» — любой мусорный файл silero-vad-v5.bin будет молча передан whisper-cli через --vad-model (stt.rs:253 проверяет только p.exists()). Это не R-ограничение паспорта: TODO в коде сам говорит «перед production запустить refresh-скрипт», но код уже в main и ветки зависимы от него.
FIX: Прогнать scripts/refresh-model-catalog.sh и вписать реальные SHA256/size; до этого — исключить placeholder-записи из выдачи local_engine_list_catalog/storage_list либо блокировать download() для них с явной ошибкой до начала скачивания.

### [MEDIUM|unverified] Гонка параллельных download одного id: SHA считается по сетевому потоку, а не по файлу — «верифицированный» файл может быть битым
apps/desktop/src-tauri/src/local_engine/models.rs:478
Нет никакой блокировки per-model-id: два вызова local_engine_model_download(id) (двойной клик, авто-download ассистента assistant/embedder.rs:100 параллельно с ручным) пишут в один tmp `<id>.bin.partial`. Задача B делает remove_file(&tmp) (строка 479) и File::create, при этом A продолжает писать в свой (уже unlinked или пересозданный) fd; hasher у каждой задачи обновляется её собственными сетевыми байтами (строка 515), а не содержимым файла. Итог: A завершает поток, её stream-hash совпадает с каталожным, и fs::rename (строка 563) атомарно устанавливает в dest файл, содержимое которого — недокачанный/interleaved результат B. Проверка «SHA256 match → atomic rename» формально проходит, но верифицированы не те байты, что легли на диск. check_status_fast потом поймает size-mismatch, но для placeholder-записей (см. отдельную находку) — нет.
FIX: Per-model-id async Mutex/HashMap<ModelId, Mutex> вокруг download_inner, уникальное имя партиала (uuid) + после rename — контрольный file_sha256 по факту записанного файла (или хешировать при чтении из файла, а не из потока).

### [MEDIUM|unverified] Резидентный llama-server слушает фиксированный localhost-порт без аутентификации
apps/desktop/src-tauri/src/local_engine/llm_server.rs:122
Сервер поднимается на 127.0.0.1:47331 (SERVER_PORT, строка 27) без --api-key: любой процесс любого пользователя машины может дергать POST /completion (и служебные endpoints llama.cpp: /props, /tokenize и т.д.) — бесплатный inference на чужих 2-5GB RAM/GPU, DoS очереди рекапов (сервер --parallel 1), плюс расширение attack surface на llama.cpp HTTP-парсер, у которого были CVE. Через этот сервер проходят транскрипты звонков (sensitive по W5), а принцип паспорта — контент не покидает доверенный контур. Порт фиксированный, что также даёт коллизию/preoccupation-вектор: чужой процесс, занявший 47331 и отвечающий {"status":"ok"} на /health, станет «сервером» для generate_via_server и получит все промпты с транскриптами.
FIX: Генерировать случайный api-key на старте и передавать `--api-key` (валидатор в capabilities) + заголовок Authorization в generate_via_server; в идеале — случайный свободный порт вместо фиксированного.

### [LOW|unverified] Комментарии обещают ленивую SHA256-проверку перед использованием модели — её нет нигде
apps/desktop/src-tauri/src/commands/local_engine.rs:103
Комментарии «SHA256 is verified lazily before model use (check_status in STT/LLM init)» (строки 64 и 103) и док stt.rs:87 («проверка происходит в models::check_status до запуска pipeline») не соответствуют коду: pipeline/mod.rs:631/1103/1683 и assistant/embedder.rs используют только check_status_fast (размер файла). Полный SHA256 выполняется ровно один раз — на download-пути. Сам perf-tradeoff задокументирован в models.rs:299-304 и выглядит осознанным, но ложные комментарии создают у ревьюера/автора W5-аудита неверное представление о tamper-resistance гарантии.
FIX: Поправить комментарии на «SHA verified only at download time; runtime checks are size-only», либо реально добавить одноразовую фоновую SHA-проверку после первого использования модели в сессии.

### [LOW|unverified] Temp-файлы с транскриптом утекают при abort таски (cancel пайплайна)
apps/desktop/src-tauri/src/local_engine/llm.rs:464
Cleanup prompt/grammar/schema-файлов (строки 464-470) и whisper-JSON (stt.rs:328) выполняется обычным кодом после await. При JoinHandle::abort() (отмена звонка/выход) SidecarGuard убьёт процесс через Drop, но async-функция не продолжится и remove_file не выполнится — файлы wotold-llama-*.txt с полным транскриптом остаются в tmp до очистки ОС. Perms 0o600 смягчают (только владелец), но по духу W5 («temp file leak — transcript в /tmp») content-bearing файлы не должны переживать отмену.
FIX: RAII-guard на удаление (struct с Drop → std::fs::remove_file) по аналогии с SidecarGuard, либо scopeguard; плюс janitor-очистка wotold-*-паттернов в tmp на старте приложения.

### [LOW|unverified] Whisper-JSON с транскриптом создаётся сайдкаром с default umask, 0600 накладывается пост-фактум
apps/desktop/src-tauri/src/local_engine/stt.rs:568
В отличие от llm.rs, где Rust сам создаёт файлы с O_EXCL|0600, output-JSON пишет whisper-cli с обычным umask (0644); chmod до 0600 в parse_whisper_json (строки 568-575) происходит только после Terminated — на всё время транскрипции (минуты) и записи файла контент доступен на чтение по umask. На macOS tmp per-user (/var/folders, 0700), так что практический риск мал, но это признанная в комментарии M-2 полу-мера: «tighten ДО чтения», а не до записи.
FIX: Передавать -of в приватную поддиректорию, созданную Rust'ом заранее с mode 0o700 (mkdir + set_permissions до spawn) — тогда perms файла внутри неважны.

## Insights
- Архитектура безопасности сайдкаров — трёхслойная и честно задокументирована: primary control = построение путей в Rust из констант + UUID, второй слой = ensure_path_under/запрет ParentDir, последний = anchored regex в capabilities/default.json. Важно понимать: path-валидатор `^[A-Za-z0-9._/ \-]+$` сознательно пропускает и `..`, и абсолютные пути — вся реальная защита живёт в Rust-слое, и capability-файл сам об этом предупреждает в description. Это правильный дизайн, но он делает Rust-слой единственной настоящей границей.
- Криптографическая верификация моделей фактически действует только в момент скачивания: рантайм доверяет диску по exact-size (check_status_fast). Это осознанный perf-tradeoff (SHA256 на 4.7GB перед каждым прогоном — десятки секунд), но он тихо сужает заявление W5 «SHA256-only защита от подмены» до «защита от подмены в момент download» — подмена файла на диске тем же размером после установки не детектируется никогда.
- Sanitize-функции в Rust (normalize_lang, sanitize_prompt) спроектированы как зеркала regex-валидаторов из capabilities/default.json (`^[a-z]{2,5}$`, `^[^\r\n]{0,1000}$`) — связь задокументирована только комментариями. Правка default.json без синхронной правки Rust даст не уязвимость, а внезапные отказы spawn'а (валидатор строже) — рассинхрон ловится только в рантайме.
- Два пути исполнения LLM (one-shot llama-cli и резидентный llama-server) имеют разный security-профиль: CLI-путь получил все W5-митигации (0600 temp, O_EXCL, path checks), а server-путь передаёт те же транскрипты через неаутентифицированный localhost HTTP + cache_prompt держит KV с контентом в памяти долгоживущего процесса. Митигации не переехали вместе с фичей B2.
- Hallucination-фильтр (HALLUCINATIONS_EXACT/SUBSTRINGS в stt.rs) — агрессивный чёрный список: реальные короткие реплики «Thank you» (без точки), «Bye», «Thanks» молча выпадают из транскрипта. Для звонков с англоязычными вежливыми концовками это систематическая потеря контента; смягчено только debug-логом и filter-stats телеметрией.
- Паттерн SidecarGuard + перенос queue-permit внутрь spawn_blocking (diarization.rs) показывает зрелое понимание tokio-семантики отмены: abort не прерывает blocking-задачу, поэтому ресурс числится занятым до реального конца ONNX-вычисления. Такой уровень аккуратности с cancellation — редкость.
- Код несёт богатую «археологию инцидентов» в комментариях ([P-fix5] max-context 0 против prompt-echo, [M13 fix] UTF-8 lossy, [recap-fix] repeat-penalty против degenerate loops, NOTE про --log-disable, съедающий stdout) — каждый прод-инцидент оставил и фикс, и регрессионный тест, и объяснение. Это сильно снижает риск повторного «улучшения» обратно.


# AREA: rust-assistant — score 7/10
## Area label: Rust — ассистент/RAG (apps/desktop/src-tauri/src/assistant/*, db/assistant.rs)

## Strengths
- FTS-инъекции закрыты конструктивно: build_match_expr (retrieval.rs:148-180) токенизирует по не-алфанумерике и оборачивает каждый токен в кавычки, сырой MATCH-синтаксис до SQL не доходит; есть негативные тесты (injection_attempts_return_ok, malformed_match_expr_is_err_not_panic) и явный контракт-warning на search_fts (db/assistant.rs:374-376).
- Слоёный injection-hardening промпта: system-правило «фрагменты — данные, не инструкции» (answer.rs:93-135), нейтрализация делимитеров <<<//>>> во фрагментах, титулах И истории (answer.rs:141-146, тест injected_delimiters_in_fragments_are_neutralized), плюс детерминированная привязка источников — модель возвращает только номера фрагментов, call_id/таймкоды она физически не генерирует (resolve_used_fragments с клэмпом и fallback).
- Продуманная деградация по всей цепочке: нет эмбеддера → чистый BM25 (retrieval.rs:242-243), пустой кэш векторов → BM25 (253), dim-mismatch вектора скипаются (retrieval.rs:381, embed_cache.rs:57-70), ошибка эмбеддинга не роняет индексацию — добирает startup-backfill (indexer.rs:371-375). Каждая ветка покрыта тестом.
- Детерминизм как контракт: RRF-тай-брейк по passage_id (fusion.rs:23-27), cosine-тай-брейк (retrieval.rs:387-391), стабильная нумерация [1..N] для промпта, тест hybrid_output_is_stable_between_runs — нумерация источников не плавает между запусками.
- Конкурентность решена на уровне БД, а не «на удачу»: get_or_create_call_chat через ON CONFLICT + перечитка с vanish-check (db/assistant.rs:84-104), order_idx атомарным подзапросом внутри INSERT (199-213), backfill c TOCTOU-перепроверкой still_pending (indexer.rs:511-526); есть тесты concurrent_get_or_create_yields_single_thread и concurrent_appends_keep_order_idx_contiguous.
- Идемпотентная переиндексация: replace_call_passages = DELETE+INSERT+upsert index_state одной транзакцией (db/assistant.rs:282-331), self-heal через clear_index_state при фейле fire-and-forget индексации (indexer.rs:472-477) — regen-случай не оставляет в поиске устаревший контент.
- Тестовая культура выше средней: негативные тесты роутинга (negatives_content_questions_are_not_routed_words), атакующий тест на подделку делимитеров, live-gate e2e против копии реальной БД, KeywordMockEmbedder с контролируемой семантикой для golden-кейса «BM25 мимо, вектор находит».

## Findings

### [HIGH|CONFIRMED] Роутер перехватывает контентные вопросы с якорем «встреча/звонок» и отвечает детерминированно неверно
apps/desktop/src-tauri/src/assistant/router.rs:158
ListCalls-интент срабатывает на has_any(["какие","покажи","список","перечисли"]) && has_any(CALL_WORDS). Вопрос «Какие решения приняли на встрече?» содержит «какие»+«встрече» → пользователь получает список звонков вместо решений, LLM и retrieval не вызываются вообще. Аналогично Stats (router.rs:125): «Сколько задач раздали на встрече?» → «Записано N звонков, суммарная длительность…». Direct-ответ роутера безальтернативен (нет fallthrough при низкой уверенности), поэтому false positive = гарантированно неверный ответ. Негативный тест-лист (router.rs:858-865) содержит только «что…»-формулировки и эту дыру не ловит.
FIX: Для ListCalls/Stats требовать, чтобы кроме триггера и call-слова в вопросе не оставалось значимых контентных токенов (по аналогии с when_topic: срезать служебные — если остаток непуст, вернуть None и уйти в обычный конвейер). Добавить негативы «какие решения приняли на встрече», «сколько задач раздали на встрече».

### [MEDIUM|unverified] Темпоральный префильтр применяется ПОСЛЕ LIMIT — ложные «не найдено» на больших архивах
apps/desktop/src-tauri/src/assistant/retrieval.rs:243
В BM25-ветке период фильтруется пост-фактум: search() возвращает глобальный top-12 (GLOBAL_LIMIT), затем keep() выкидывает звонки вне периода (retrieval.rs:233-243). В гибриде то же с top-30 кандидатами (fuse_pass, retrieval.rs:320-323). Вопрос «что обсуждали вчера про бюджет» на архиве, где по «бюджет» много сильных матчей из других дней, а вчерашние пассажи ранжируются ниже top-12/30, даст пустой результат → ask_core ответит «По звонкам ничего не найдено», хотя вчерашний звонок содержит ответ. Cosine-канал фильтрует корректно (в отборе кандидатов), BM25 — нет.
FIX: Прокинуть period-set в search_fts как SQL-условие (p.call_id IN (...)) либо динамический QueryBuilder — фильтровать до LIMIT, как уже сделано для cosine_top_n.

### [MEDIUM|unverified] Детерминированные ответы (роутер/refusal/empty) недоступны без установленной LLM-модели
apps/desktop/src-tauri/src/assistant/mod.rs:369
Прод-обёртка ask() безусловно вызывает build_local_llm_provider ДО ask_core_with. При невыбранном пресете или отсутствующей модели тот возвращает Err(local_engine_preset_not_set / local_engine_model_missing) (pipeline/mod.rs:615-636). В итоге мета-вопросы («сколько звонков записано»), refusal и empty-ветки — весь смысл роутера M16.4 «нулевая латентность, без LLM» — падают ошибкой «модель не установлена», хотя LLM в этих путях не нужна.
FIX: Строить провайдер лениво: сначала прогнать классификатор/роутер/empty-ветки, провайдер поднимать только при входе в llm_answer_path (или обернуть в Lazy/Option с ошибкой в момент фактического generate).

### [LOW|unverified] embed_batch молча усекает при mismatch количества векторов; embed_backfill_with может зациклиться
apps/desktop/src-tauri/src/assistant/indexer.rs:406
rows.iter().zip(vecs.iter()) не проверяет, что эмбеддер вернул ровно len(rows) векторов — при частичном/пустом ответе (баг реализации TextEmbedder, OOM в ONNX) лишние строки молча выпадают без ошибки. В embed_backfill_with (indexer.rs:443-457) цикл крутится «пока list_passages_missing_embedding непуст»: если эмбеддер стабильно возвращает Ok с меньшим числом векторов для одних и тех же строк (вырожденно — пустой Vec), blobs не сокращает missing-набор и цикл становится бесконечным без прогресса и без лога.
FIX: После embed_passages проверять vecs.len() == rows.len(), иначе Err; в цикле backfill дополнительно страховаться проверкой прогресса (blobs.is_empty() → break с warn).

### [LOW|unverified] Нейтрализуются только маркеры <<</>>>, но не внутренние разделители фрагментов — подделка атрибуции
apps/desktop/src-tauri/src/assistant/answer.rs:141
neutralize_markers закрывает подделку внешних делимитеров блока, но пассаж-текст может содержать «\n---\n[3] «Фейковый звонок» · 01.01.2026 · owner · 0:10:\nсфабрикованный текст» — формат заголовка фрагмента ([n] «титул» · дата · спикер · т/к, build_input:186-199) и разделитель --- не нейтрализуются. Злонамеренный участник звонка может надиктовать текст, который модель воспримет как отдельный «фрагмент» с фейковым содержимым; used_fragments=[3] после клэмпа сошлётся на РЕАЛЬНЫЙ третий фрагмент — атрибуция ответа искажается. Прямого RCE/утечки нет (MCP read-only, локально), но W5 трактует контент звонков как недоверенный, а этот вектор остаётся открытым.
FIX: Нейтрализовать и внутренний паттерн заголовка: экранировать строки вида ^---$ и ^\[\d+\] « внутри текста фрагментов (например, префиксом пробела или заменой скобок), по аналогии с ‹‹‹/›››.

### [LOW|unverified] LastCall-интент не матчит женские формы: «последняя/последней встреча» уходит мимо роутера
apps/desktop/src-tauri/src/assistant/router.rs:130
Триггер has_any(["последний","последнего","последнем","крайний"]) покрывает только мужской род, при том что CALL_WORDS содержит женские «встреча/запись». «Когда была последняя встреча?» / «о чём последняя запись?» не роутятся → уходят в retrieval, где «последняя» ничего лексически не матчит → вероятный empty или нерелевантный ответ. Это ровно класс живых фейлов (Q1/Q6/Q9/Q15), ради которого роутер строился.
FIX: Дополнить список формами «последняя/последней/последнюю/последние/крайняя/крайней» и добавить тест «когда была последняя встреча».

## Insights
- Роутер — это word-set матчер без понятия «а остался ли в вопросе контент»: интенты срабатывают на пересечении двух словарей, и Direct-ответ полностью вытесняет LLM-конвейер. Паттерн when_topic (срез служебных слов, остаток → тема) — правильный, но применён только к WhenDiscussed; Stats/ListCalls его не используют, отсюда весь класс false positives.
- Архитектура сознательно компенсирует слабость локальной 3B-модели структурой: классификатор → интент-раутер → темпоральный префильтр → рекап-путь мимо FTS — четыре детерминированных слоя ДО LLM. Это редкий и правильный дизайн для local-first RAG; но чем больше детерминированных перехватов, тем дороже каждый false positive (нет самокоррекции).
- Гибридный поиск — full-scan cosine по in-memory кэшу (load_all_embeddings, ~46MB на 1000 звонков) с инвалидацией по штампу (COUNT index_state, MAX indexed_at, COUNT embeddings). Для целевого масштаба честно и просто, без ANN-индекса; цена — полная перезагрузка кэша на каждой переиндексации и tokio::Mutex на время загрузки.
- Инвариант «свои пассажи безусловно раньше чужих» в call-scope реализован порядком проходов (own-fusion top-8, затем other-fusion top-4), а не слиянием скоров — контракт retrieval→budget→answer задокументирован в доках модуля и закреплён тестами. budget при этом жадный и rank не читает — вся релевантность закодирована в порядке Vec.
- Индексация toleranta к частичным данным: транскрипт/рекап/structured — каждый источник опционален, карточка звонка (титул+дата+участники) индексируется всегда — «в каком звонке был X» отвечается даже по звонку без артефактов. Резолв speaker-тегов в имена контактов вшивает имена прямо в текст пассажа, чтобы FTS находил «что говорил Дамир» — хак, но эффективный.
- fmt_date продублирован трижды (mod.rs::fmt_call_date:390, router.rs::fmt_date:249, contacts_ctx.rs::fmt_date:156) — мелкий DRY-долг; аналогично words()/детект-паттерн «split по не-алфавитным» повторён в classifier/router/answer/contacts_ctx с чуть разными предикатами (is_alphabetic vs is_alphanumeric) — расхождение легко превратить в баг при следующем словаре.
- WINDOW_TOKENS=8192 захардкожен в mod.rs:39 с комментом «llm.rs DEFAULT_CTX_SIZE» — при смене ctx-size пресета контракт ответа (fragment_tokens/window_tokens в UI) начнёт врать молча; связать константы стоило бы через общий const.


# AREA: rust-audio — score 6/10
## Area label: Rust аудио + Swift sidecar (Core Audio process tap, IPC, WAV pipeline)

## Strengths
- Образцовый rollback-каскад при ошибках инициализации tap'а: каждый шаг ProcessTapRecorder.start (Sources/WotoldAudio/ProcessTapRecorder.swift:88-154) при фейле уничтожает все ранее созданные ресурсы (tap → aggregate → writer → IOProc) в правильном порядке.
- Crash-safety записи продумана сквозняком: WAVWriter пишет placeholder-заголовок и каждые 5с делает flushHeader()+synchronize() (WAVWriter.swift:84-92), а Rust-сторона в reconcile_orphan_recordings меряет длительность по фактическому размеру файла, а не по возможно-устаревшему полю data-чанка (commands/recording.rs:191-238).
- Sidecar-lifecycle через stdin-EOF — элегантная защита от зомби: sidecar живёт в readLine-цикле (App.swift:75) и умирает при закрытии stdin, поэтому drop(CommandChild) в любом error/timeout-path Rust'а (macos.rs:228, permissions.rs:82) гарантированно прибивает процесс без PID-tracking.
- Privacy-дизайн call-detect probe соответствует R3-deviation: CallActivityProbe читает только флаг kAudioDevicePropertyDeviceIsRunningSomewhere + frontmost bundle id, ни одного байта чужого аудио (CallActivityProbe.swift:5-11), probe отключается на время собственной записи (call_detect.rs:233-240).
- Pre-flight проверка разрешений перед стартом записи с понятными ошибками вместо молчаливого фейла sidecar'а (commands/recording.rs:103-117) — C1-гейт работает: запись возможна только по явной команде при granted microphone + screen_recording.
- Чистые алгоритмические модули покрыты тестами по TDD-стандарту проекта: silence_detector.rs (8 тестов, включая boundary/tail-run кейсы), wav_chunker.rs (5 тестов), audio_io.rs (7 тестов), call_detect.rs cooldown-логика (6 тестов).
- Гонка close-vs-in-flight-buffer в AudioRecorder.stop() осознанно найдена и починена ([M13 fix] AudioRecorder.swift:176-184): сначала removeTap, потом close на той же serial queue через queue.sync — грамотное рассуждение о конкурентности зафиксировано прямо в коде.
- Rust-сторона сознательно не доверяет duration от sidecar'а (per-rotate reset) и пересчитывает wall-clock из session.started_at с вычетом пауз, включая незакрытое окно паузы при stop-во-время-паузы (commands/recording.rs:333-358) — устойчиво к дрейфу.

## Findings

### [HIGH|CONFIRMED] Любой sidecar "error" event терминален для dispatcher'а — non-fatal ошибка rotate убивает сессию, пока запись реально продолжается
apps/desktop/src-tauri/src/audio/macos.rs:327
run_dispatcher матчит "stopped" | "error" как терминальные (macos.rs:327-332): oneshot terminal_tx потребляется и task завершается. Но Swift-протокол шлёт {"event":"error"} и для НЕ-фатальных сбоев: "mic rotate failed" / "system rotate failed" (App.swift:147,154) — после которых sidecar продолжает писать оба трека. Итог: одна неудачная ротация (например, ENOSPC-подобный transient при создании chunk-дира) останавливает level-метр и rotate-фан-аут, а последующий stop_recording получает из terminal_rx старый rotate-error → audio_macos::stop возвращает Err → звонок помечается failed (recording.rs:381-386), хотя все WAV-данные на диске целы и sidecar корректно завершился. Часовая запись теряется из-за одного transient'а.
FIX: Разделить error-scope в протоколе: {"event":"rotate_error"} (non-fatal, dispatcher логирует и продолжает) vs {"event":"error", fatal:true}. В dispatcher'е терминальным считать только fatal и Terminated.

### [HIGH|CONFIRMED] Pause — DB-only: во время «паузы» микрофон и системный звук продолжают писаться на диск и попадают в транскрипт
apps/desktop/src-tauri/src/commands/recording.rs:570
Комментарий [W2] (recording.rs:570-578) честно фиксирует: sidecar не знает о паузе, frames продолжают писаться, TODO(W2 v2). Но обоснование «тишина отрежется в silence trim» неверно по сути: юзер жмёт паузу именно чтобы сказать/услышать приватное — это не тишина, это речь, которая будет записана, транскрибирована Whisper'ом и попадёт в саммари. Это прямое нарушение ожидания согласия (C1-смежная угроза паспорта: запись без согласия), и оно НЕ входит в принятые ограничения R1–R13. Дополнительно duration вычитает paused_ms, а аудио паузу содержит — таймстемпы транскрипта расходятся с заявленной длительностью на длину пауз.
FIX: Минимум для MVP: wire {"cmd":"pause"}/{"cmd":"resume"} в sidecar — engine.pause() для mic + AudioDeviceStop для tap'а (или дешевле: флаг в handleAudio/processBuffer, дропающий буферы). Либо явно предупреждать в UI, что пауза не останавливает захват.

### [MEDIUM|unverified] Отключение входного устройства не обрабатывается — mic-дорожка молча умирает, запись «продолжается»
apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/AudioRecorder.swift:32
AudioRecorder строит AVAudioEngine от inputNode (строки 32-34) и нигде не подписывается на AVAudioEngineConfigurationChange notification (rg по Sources — 0 совпадений). Если во время записи отваливается активный вход (Bluetooth-гарнитура села, USB-микрофон выдернут), engine перестаёт доставлять буферы или останавливается — mic.wav молча замирает, никакого error-event в Rust, UI показывает идущую запись (level mic=0 — единственный намёк). Часовой звонок может оказаться без дорожки пользователя. Аналогично ProcessTapRecorder не слушает уведомления об изменении конфигурации aggregate device.
FIX: Подписаться на .AVAudioEngineConfigurationChange: перезапустить engine с новым inputNode-форматом (пересоздав converter) либо эмитить {"event":"device_lost"} чтобы Rust показал предупреждение.

### [MEDIUM|unverified] ProcessTapRecorder.stop() закрывает WAV вне serial queue — гонка с in-flight IOProc-блоком, которую в AudioRecorder уже чинили
apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/ProcessTapRecorder.swift:212
stop() вызывает AudioDeviceStop/DestroyIOProcID, затем try wavWriter?.close() и nil-ит поля на вызывающем потоке (строки 202-222), тогда как handleAudio исполняется на self.queue. Уже задиспатченный на queue блок может читать wavWriter одновременно с close/nil на другом потоке (write-after-close, недописанный tail финального system-chunk'а). В AudioRecorder.stop() ровно эту гонку осознанно чинили ([M13 fix], AudioRecorder.swift:176-184: close внутри queue.sync) — ProcessTapRecorder симметричного фикса не получил.
FIX: После AudioDeviceStop/DestroyIOProcID выполнить close()+обнуление полей внутри queue.sync { }, как в AudioRecorder.stop().

### [LOW|unverified] Повторный start без stop утекает tap + aggregate device
apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/ProcessTapRecorder.swift:49
start(systemURL:) не проверяет, что tapID/aggregateID уже заняты, и перезаписывает поля (строки 156-163) — старые CA-объекты остаются жить до выхода процесса. AudioRecorder.start при повторном вызове делает try? stop() (AudioRecorder.swift:28-30), ProcessTapRecorder — нет. Практическая экспозиция мала (Rust шлёт один start на процесс), но протокол повторный start допускает.
FIX: В начале start(): guard tapID == kAudioObjectUnknown else { _ = try? await stop() } — симметрично AudioRecorder.

### [LOW|unverified] dataBytes &+= молча оборачивает UInt32 после 4 GiB — заголовок становится мусором без диагностики
apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/WAVWriter.swift:76
dataBytes объявлен UInt32 (строка 14) и инкрементится wrapping-оператором &+= (строка 76). Непрерывная non-chunked запись 16kHz mono i16 достигает 4 GiB за ~37 часов — маловероятно, но wrap сделает RIFF/data-размеры в flushHeader() бессмысленными молча. WAV формат всё равно ограничен 4 GiB — правильное поведение это cap + warning, а не wrap.
FIX: Использовать saturating-семантику: dataBytes = min(UInt64 счётчик, UInt32.max) при записи заголовка + однократный warning в stderr при переполнении.

### [LOW|unverified] Два параллельных WAV→f32 ридера с разными константами нормализации (32767 vs 32768)
apps/desktop/src-tauri/src/audio_io.rs:35
audio_io::read_wav нормализует через `s as f32 / i16::MAX as f32` (=32767, строка 35), а wav_chunker::read_wav_segment — через `/ 32768.0` (wav_chunker.rs:82,86). Оба модуля решают одну задачу (вырезать сегмент WAV в f32 для voice embedding), оба живут в проде. Расхождение амплитуды ничтожно (~0.003%), но дублирование кода нарушает DRY и однажды разойдётся сильнее (см. уже разное поведение: audio_io отвергает stereo, wav_chunker сворачивает в mono усреднением).
FIX: Свести к одному модулю (wav_chunker более полный) и удалить/делегировать дубль; зафиксировать одну константу нормализации.

### [LOW|unverified] latestRms читается/пишется из разных потоков без синхронизации — формально data race по Swift memory model
apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/AudioRecorder.swift:23
latestRms пишется из serial queue processBuffer'а (AudioRecorder.swift:141, ProcessTapRecorder.swift:290), а читается level-таймером на другой queue (App.swift:54-55). Комментарий признаёт «не thread-safe, но atomic-fast enough» — на практике torn read Float на arm64 не случится, но это UB по модели памяти и первый кандидат на странности под TSan/будущим Swift 6 strict concurrency.
FIX: Обернуть в os_unfair_lock/Atomic<Float> или читать RMS через queue.sync — копеечная цена на 10Hz таймере.

## Rejected by verifier
- Частичная ротация не атомарна: mic уже переключён на новый chunk, system-rotate фейлится — треки расходятся по chunk-границам → Код по строкам 143-156 App.swift существует, но описанный сценарий расхождения опровергается тремя фактами. (1) ProcessTapRecorder.rotate сначала закрывает старый writer (:187), затем создаёт новый (:190): при фейле создания (единственный правдоподобный триггер — WAVWriter.init сам создаёт директорию через createDirectory(withIntermediateDirectories:true), WAVWriter.swift:20-24) system-chunk N уже закрыт на той же границе, что и mic; последующие system-фреймы падают на closed handle и дропаются с stderr-логом (handleAudio, :281-288) — system НЕ «продолжает писать в chunks/N/system.wav». (2) Каскада «все последующие chunk'и смещены» не существует: sidecar эмитит {"event":"error"} без "rotated"; Rust-диспетчер трактует любой "error" как терминальный — шлёт в terminal_tx и завершается (audio/macos.rs:327-331), при этом дропаются единственные sender'ы OrchestratorChannels → rms_rx оркестратора закрывается → цикл оркестратора выходит (chunk_orchestrator.rs:200-203). Никаких дальнейших ротаций и последующих chunk'ов нет; run_chunk(N) для сорванной границы не enqueue'ится (rotated не пришёл). (3) Тихого drift'а транскрипта нет: при stop terminal_rx уже содержит error → audio stop возвращает Err → stop_recording вызывает fail_recording и возвращает ошибку (recording.rs:380-386); звонок явно помечен failed и идёт через explicit recover_chunked_call, а не через merge со съехавшими таймстемпами. Остаточный эффект (потеря system-хвоста при дисковом IO-сбое, mic пишет до stop) — неотъемлемая деградация при отказе диска, которая surface'ится как ошибка, а не заявленный high-баг с молчаливым расхождением диаризации по chunk-границам.

## Insights
- Протокол sidecar'а конфлирует два класса ошибок в одном {"event":"error"}: фатальные (start failed) и операционные (rotate failed при живой записи). Rust-dispatcher вынужден трактовать все как терминальные — это архитектурный корень самого серьёзного бага слоя. NDJSON-протоколу нужна error-таксономия, а не одно поле message.
- Управление жизненным циклом sidecar'а через stdin-EOF (drop CommandChild → readLine возвращает nil → процесс выходит) — неочевидно элегантный паттерн: все error/timeout/panic-пути Rust'а автоматически прибивают процесс без PID-бухгалтерии и kill-гонок. Тот же механизм переиспользован в permissions one-shot и call-detect.
- Rust-ядро систематически не доверяет числам sidecar'а: duration_sec игнорируется ([P6]), длительность WAV меряется по размеру файла а не по заголовку ([B19.6]), total пересчитывается из wall-clock. Это правильная защитная позиция — Swift-процесс считается ненадёжным источником, надёжен только сам диск.
- Пауза реализована на трёх несогласованных уровнях: DB (paused_at), orchestrator (pause_tx) и sidecar (не знает о паузе вообще). Duration вычитает паузы, аудио их содержит, chunk start_ms — pause-inclusive: код уже борется с последствиями этого рассогласования (комментарий на recording.rs:524-526 про «финальный chunk схлопывался в ноль»). Это сигнал, что pause надо спускать на уровень захвата, а не компенсировать выше.
- Global mono process tap (CATapDescription(monoGlobalTapButExcludeProcesses: [])) означает: system-дорожка — это mixdown ВСЕГО системного звука, включая уведомления, музыку и другие приложения во время звонка. Для STT это шум-источник, никак не фильтруемый на этом уровне — осознанная простота, но нигде явно не задокументированная как ограничение качества.
- Уровень зрелости внутри слоя неоднороден: AudioRecorder прошёл несколько итераций конкуренционных фиксов ([M13 fix] stop-race), а его близнец ProcessTapRecorder — нет (stop вне queue, нет idempotent-guard). Парные классы с одинаковым контрактом расходятся — рефакторинг в общий протокол/базу снизил бы риск односторонних фиксов.
- Гейт macOS 14.4+ живёт только внутри sidecar'а (guard #available в main → error event). Rust узнаёт о несовместимости ОС лишь по факту фейла start — нет раннего capability-check'а при запуске приложения, юзер на macOS 14.0-14.3 увидит ошибку только нажав «Запись».


# AREA: frontend — score 7/10
## Area label: React frontend (apps/desktop/src)

## Strengths
- useFocusTrap (src/hooks/useFocusTrap.ts) — полноценный WCAG-трап: динамический пересчёт focusables на каждый Tab, restore focus только если его никто не перехватил, scroll-lock c восстановлением overflow. Используется консистентно в модалах (consent в App.tsx, Modal, SpeakerConfirmModal).
- Типизированный i18n: ключи через рекурсивный DotPath<TranslationStrings> (src/i18n/index.ts:67-75), en.ts/kk.ts аннотированы `: TranslationStrings` — рассинхрон словарей ловит tsc, есть runtime-fallback ru→en и «шумные» непросубституированные {param} для QA.
- Осознанная работа с гонками там, где она есть: useAssistantChats (src/hooks/useAssistantChats.ts) — синхронный pendingRef против двойного ask, last-click-wins через activeRef до await, guard на устаревший ответ (:157), cancelled-флаг для listen() при fast-unmount; тот же StrictMode-фикс listener-утечки в LocalEngineSection.tsx:212-260 с комментарием почему.
- Оптимистичные апдейты сделаны правильно: snapshot + functional updater, восстанавливающий ТОЛЬКО пропатченные поля, чтобы не затирать конкурентные call:progress события (CallDetailPage.tsx:243-255 и :311-317) — редкая аккуратность.
- Центральный маппер ошибок api/errors.ts: упорядоченный список паттернов с продуманным порядком (local_llm_timeout ДО generic timeout, quota_exceeded отделён от transient 429), fallback с усечением до 160 символов — юзер не видит сырые InvocationError.
- dev-tauri-mock.ts безопасен по конструкции: guard import.meta.env.DEV && !__TAURI_INTERNALS__ (строка 15) статически вырезается Vite в prod-сборке, мок типизирован против @wotold/contracts — дрейф формы ловит tsc.
- A11y-база на месте: TableRow (pages/inboxBits.tsx:225-240) — role=button + Enter/Space, Tabs с aria-controls/aria-selected парами через useId, aria-label на всех icon-кнопках, role=alert на ошибках, aria-busy на скелетонах.
- Тесты лежат рядом с модулями и покрывают именно алгоритмическую логику: RecordingContext.test.tsx, useAssistantChats.test.ts, inboxData.test.ts, errors.test.ts, transcriptActive.test.ts — соответствует заявленной в CLAUDE.md стратегии.

## Findings

### [MEDIUM|CONFIRMED] Ошибка любого действия (export/delete/unbind) уничтожает всю страницу звонка
apps/desktop/src/pages/CallDetailPage.tsx:384
Ранний return `if (error) return <p role="alert">…` использует общий error-state из useCallDetail, но в него же пишут setError() обработчики несмертельных действий: onExportMarkdown (строки 346, 355), onDelete (:378), onUnbindVoice (:288), onRegenerateRecap (:306). Упавший экспорт markdown (например, отказ записи в выбранную папку) заменяет весь открытый звонок — транскрипт, плеер, рекап — одним красным абзацем без кнопки «назад» и без retry. Контент возвращается только через навигацию по рейлу.
FIX: Разделить fatal load-error (call meta не загрузилась) и action-error: ранний return только для первого, action-ошибки рендерить баннером/тостом поверх нормального контента.

### [MEDIUM|unverified] Initial-load эффект без отмены — stale data race при смене callId
apps/desktop/src/hooks/useCallDetail.ts:149
Promise.allSettled из 12 ресурсов (:152-165) не имеет cancelled-флага и cleanup. CallDetailPage рендерится в App.tsx:557 без key={callId}, поэтому переход на другой звонок (onOpenCall из рейла/ассистента/⌘K) не размонтирует компонент — эффект перезапускается, но старый allSettled резолвится позже и setCallState/setRecap/setTranscript перезаписывают данные НОВОГО звонка данными старого. Первый .finally также преждевременно снимает loading для второго запроса. Контраст: соседний useCallAudio.ts:61-92 cancelled-флаг имеет.
FIX: let cancelled = false + return () => { cancelled = true } и проверка перед всеми сеттерами (как в useCallAudio), либо key={callId} на CallDetailPage.

### [MEDIUM|unverified] humanError — все ~40 сообщений захардкожены по-русски в обход i18n
apps/desktop/src/api/errors.ts:17
Приложение поддерживает ru/kk/en (src/i18n/index.ts), но центральный маппер ошибок возвращает только русские строки ('Нет соединения с сервером Wotold.', 'Превышен дневной лимит…' и т.д.). humanError используется ВЕЗДЕ — тосты инбокса, баннеры CallDetail, LocalEngineSection, ассистент. Пользователь с en/kk локалью получает весь happy-path переведённым, а все ошибки — на русском.
FIX: Перевести PATTERNS на TranslationKey и прокидывать t (или возвращать ключ + params), словари en/kk уже типизированы под это.

### [MEDIUM|unverified] Метки спикеров и фолбэк-названия звонков захардкожены по-русски
apps/desktop/src/utils/callMeta.ts:22
humanSpeakerLabel возвращает 'Голос N' / 'Спикер N' / 'Я' (:22-45), simpleDateTitle — 'Звонок …' (:123-130), плюрализация 'участник/участника/участников' (:145-147). Всё это рендерится в InteractiveTranscript, ParticipantRow, SpeakerCard, заголовке CallDetailPage — то есть на самых видимых поверхностях для en/kk локалей. Комментарий в шапке файла честно признаёт «Сохраняется ru-локаль форматирование», но modelLabel.ts показывает, что паттерн «маппинг через i18n» в проекте уже есть.
FIX: Прокинуть t в helpers по образцу utils/modelLabel.ts (он принимает TFn и берёт ключи из словаря).

### [MEDIUM|unverified] Roving tabindex без стрелочной навигации — неактивные табы недостижимы с клавиатуры
apps/desktop/src/ui/Tabs.tsx:76
Tabs.Trigger ставит tabIndex={active ? 0 : -1}, но в компоненте нет ни одного onKeyDown с ArrowLeft/ArrowRight (grep по файлу пуст). В результате Tab пропускает неактивные триггеры (tabIndex=-1), а стрелки фокус не двигают — клавиатурный пользователь физически не может переключить вкладку Транскрипт/Рекап/Ассистент. Это ломает паттерн ARIA tabs, ради которого и делался roving tabindex (aria-controls/aria-selected сделаны правильно).
FIX: Добавить в Tabs.List обработчик ArrowLeft/ArrowRight/Home/End, двигающий фокус и (или) активирующий таб — либо убрать tabIndex=-1, сделав все триггеры табуемыми.

### [MEDIUM|unverified] Во время воспроизведения весь транскрипт ре-рендерится 4-8 раз в секунду
apps/desktop/src/components/InteractiveTranscript.tsx:235
useCallAudio (hooks/useCallAudio.ts:101-104) вызывает setCurrentTime на каждый timeupdate обоих <audio> элементов (~4/сек каждый). currentTime живёт на уровне CallDetailPage → каждый тик ре-рендерит всё поддерево: groups.map по всем репликам (для часового звонка — сотни .turn строк), CallRail, AudioScrubber. В кодовой базе нет ни одного React.memo (grep пуст), строки не мемоизированы и не виртуализированы; useMemo покрывает только парсинг groups, но не рендер. На длинных звонках это заметный CPU-налог во время playback + smooth scrollIntoView на каждую смену activeIdx.
FIX: Вынести подсветку активной группы из рендера (memo(TurnRow) + передавать только isActive, или CSS-подход через data-атрибут и один эффект), throttle setCurrentTime до ~1/сек для непадающей точности подсветки.

### [MEDIUM|unverified] onPresetChange: stale closure на semanticSearch — эмбеддер не скачивается после включения тумблера
apps/desktop/src/pages/LocalEngineSection.tsx:303
Колбэк использует semanticSearch на строке 292 (`...(semanticSearch ? E5_IDS : [])`) для решения, докачивать ли e5-модели при смене preset'а, но deps useCallback — [hw, statuses, t] без semanticSearch. Сценарий: юзер включает «Семантический поиск» (state обновился, колбэк — нет), затем выбирает preset → замыкание видит старое false и НЕ ставит e5-small в очередь загрузки; ассистент потом молча ждёт модель (semanticWaiting).
FIX: Добавить semanticSearch в deps useCallback (эслинт exhaustive-deps это бы поймал).

### [LOW|unverified] refresh() никогда не сбрасывает error — инбокс навсегда застревает в error-состоянии
apps/desktop/src/pages/InboxView.tsx:449
refresh (:449-453) делает setError(humanError(e)) при падении listCalls, но на успехе setError(null) не вызывает. Рендер (:665) ставит error-ветку ПЕРВОЙ: `{error ? <p> : …}`. Один transient сбой (database is locked) → все последующие успешные refresh'ы (pipeline:finished, реактивация вида) обновляют calls, но юзер продолжает видеть только текст ошибки вместо списка до перезапуска приложения.
FIX: setCalls + setError(null) в .then, либо рендерить error баннером над списком, а не вместо него.

### [LOW|unverified] N+1 invoke: listCallSpeakers на каждый ready-звонок при каждом refresh
apps/desktop/src/pages/InboxView.tsx:530
Эффект (:530-548) на каждое изменение calls (включая каждый pipeline:finished и каждый refresh при реактивации) стреляет Promise.allSettled с отдельным listCallSpeakers(c.id) на КАЖДЫЙ ready-звонок. При сотнях звонков это сотни IPC-раундтрипов ради инициалов аватарок и person-фасета; InboxView всегда смонтирован (keep-alive), так что это происходит даже пока юзер сидит в настройках. Отмены нет — при частых refresh возможен out-of-order setSpeakerInitials.
FIX: Батч-команда на Rust-стороне (list_speakers_for_calls / поле в list_calls), либо кэш по call_id + докачивать только новые ready-звонки.

### [LOW|unverified] Reprocess из row-меню инбокса без подтверждения — непоследовательно с CallDetailPage
apps/desktop/src/pages/useInboxRowActions.ts:29
onRowReprocess сразу вызывает reprocessCall без ask(), тогда как та же операция на CallDetailPage.tsx:203-209 требует confirm с warning (пересоздание перетирает существующий рекап/транскрипт/привязки спикеров). Один misclick в kebab/ПКМ строки — и готовый звонок уходит в переработку (для local-движка — минуты) без вопросов. onRowDelete при этом confirm имеет.
FIX: Тот же ask(t('callDetail.reprocessConfirmBody')) перед reprocessCall, как на детальной странице.

### [LOW|unverified] Мёртвая сортировка: th-sort выглядит кликабельной, но обработчика нет
apps/desktop/src/pages/InboxView.tsx:727
Колонки «Длительность» и «Дата» (:727-734) обёрнуты в .th-sort с иконкой sort, а wk.css:303 даёт cursor:pointer и :hover-подсветку — аффорданс кликабельности. Ни onClick, ни sort-state в InboxView/inboxData не существует (grep по sortBy/sortKey пуст): клик ничего не делает, с клавиатуры span недостижим. Юзер получает обманчивый UI.
FIX: Либо реализовать сортировку (button + aria-sort), либо убрать иконку/cursor:pointer до реализации.

### [LOW|unverified] Загрузка хоткеев без .catch — unhandled promise rejection
apps/desktop/src/App.tsx:126
void getSetting(SETTINGS_KEYS.RECORDING_HOTKEY_TOGGLE).then(…) и парный PAUSE (:126-133) не имеют .catch, в отличие от соседнего consent-чтения строкой выше (:123-125, с catch). Падение get_setting (locked DB на старте) даст unhandled rejection в консоль webview вместо warn, хоткеи молча останутся дефолтными.
FIX: Добавить .catch((e) => console.warn(…)) по образцу соседних вызовов.

## Insights
- Серверное состояние управляется вручную (invoke + Tauri-события + ручные refetch) без TanStack Query/SWR. Дисциплина при этом высокая — Promise.allSettled для partial-failure, event-driven refetch с debounce и rate-limit (useCallDetail:392-434) — но каждая страница переизобретает load/refetch/error цикл, и именно на швах живут найденные баги: не сбрасываемый error в InboxView, отсутствие отмены в useCallDetail, N+1 спикеров.
- Культура фиксации гонок необычно явная — комментарии ссылаются на ревью-айдишники («ревью H1-H5», «Review HIGH-2 StrictMode race») и объясняют почему. Но применена она неравномерно: useAssistantChats и LocalEngineSection защищены образцово, а initial-load самого нагруженного хука (useCallDetail) — нет. Похоже, защита добавлялась точечно по итогам ревью, а не как общий паттерн/утилита.
- Module-level мутабельные синглтоны как шина сообщений между видами: requestGlobalQuestion + notifyQueued + cache в useAssistantChats.ts:40-51. Прагматичный keep-alive без глобального стора, но хук честно документирует ограничение «рассчитан на ОДИН смонтированный инстанс» — скрытая связность App→AssistantPage, которая сломается молча при втором маунте.
- Keep-alive инбокса через display:none + ручное сохранение scrollTop (InboxView:573-587) — обход сброса скролла в WebKit. Цена: всегда-смонтированный InboxView слушает все pipeline-события и гоняет полный refresh + N+1 fetch спикеров, даже когда юзер неделю сидит в настройках.
- dev-tauri-mock.ts — это фактически полноценный in-memory бэкенд (600 строк: чаты ассистента, speaker bindings, BYO-ключи) типизированный против @wotold/contracts. Guard по import.meta.env.DEV статически вырезается в prod — в бандл не течёт; удачное решение для визуальной разработки без tauri dev.
- i18n-система сделана сильно (типобезопасные dotted-ключи, compile-time shape equality en/kk), но два «нижних» слоя её обходят целиком: api/errors.ts и utils/callMeta.ts написаны до i18n и остались ru-only — весь unhappy-path и подписи спикеров непереводимы, при том что образец правильной интеграции (modelLabel.ts c TFn-параметром) в той же папке.
- В кодовой базе нет ни одного React.memo/useCallback-стратегии для строк списков — осознанный отказ от виртуализации задокументирован для инбокса (шапка InboxView: sticky-группы не ложатся на react-window), но на транскрипт это решение перенеслось молча, где стоимость выше из-за 4-8 Гц timeupdate во время playback.


# AREA: design-system — score 7/10
## Area label: Дизайн-система и визуальный слой (tokens.css / wk.css / components.css / src/ui / docs/design/wotold-v2)

## Strengths
- Токен-дисциплина реально выдержана: rg по сырым hex/oklch во всём src находит только осмысленные исключения — #fff на danger-заливках (wk.css:112, :432 — белый контент на красном, тема-инвариантен), ручку switch (wk.css:195) и canvas-фоллбеки в DualWaveform/LeveledWaveform. Ни одного «декоративного» сырого цвета в .tsx/.css вне tokens.css — enforcement-хук design-gate.mjs работает.
- Light/dark полнота 1:1 — каждый семантический токен (поверхности, 4 ступени текста, 3 бордера, accent×7, danger/ok/warn/info, тени) определён в обеих темах (tokens.css:41–128), причём dark не просто инверсия: спикер-палитра --sp1..5 отдельно перетюнена под тёмный фон (tokens.css:127), тени переведены на чистый чёрный с большей плотностью.
- Моно-графит + «красный только запись/деструктив» выдержаны: аудит var(--danger) по всем .tsx показывает только error-алерты (role="alert"), destructive-кнопки (danger-ghost для выхода/удаления, AccountSection.tsx:284 с комментарием канона) и запись (rec-btn/rec-eq/rec-strip). Единственное цветное исключение — ai-field ассистента — явно задокументировано с точным скоупом «ровно три поля» (assistant.md:26–35, components.css:1410–1433).
- Reduced-motion покрытие в components.css образцовое: ~12 отдельных @media (prefers-reduced-motion) блоков, каждый с осмысленным статическим фоллбеком (индетерминированный progress-rail → 100% width + opacity .5 на components.css:360, text-shimmer → плоский text-2 на :1170, frag-flash → статический highlight на :1573).
- docs/design/wotold-v2/assistant.md — эталонный canon-addendum: таблица маппинга «класс → где в репо» (:44–53), зафиксированные решения с точными параметрами анимации (7s/2.8s conic), acceptance-checklist из 12 пунктов с трассировкой на PRD.
- A11y-фиксы вшиты в CSS с референсами на WCAG SC: хитбокс-расширение чипов ::after inset -3px под SC 2.5.8 (components.css:1521–1526), контраст-фикс text-faint→text-3 с пометкой «4.5:1» (:1474), развязка вложенных button в списке чатов (:1492–1514), focus-индикатор светофоров под SC 2.4.11 (global.css:172–176). Icon.tsx: aria-hidden по умолчанию, title→role=img (Icon.tsx:297–301).
- Нативно-десктопная полировка в global.css: user-select whitelist (транскрипт/markdown/инпуты selectable, UI — нет, global.css:43–77), overscroll-behavior: none, -webkit-user-drag: none — Tauri-webview не ощущается веб-страницей.

## Findings

### [MEDIUM|unverified] Ядровые примитивы uikit (.btn/.iconbtn/.navitem/.tab/.seg/.trow/.menu-item) не имеют дизайнерского :focus-visible — а global.css лжёт, что reset есть
apps/desktop/src/styles/wk.css:93
rg по :focus-visible находит стили только для 7 точечных классов (rec-mini-btn, cal-event, as-chat-open, frag-ref, msg-time, frag-more в components.css + wc-btn в global.css). Ни один примитив wk.css — кнопки, иконки-кнопки, навигация, табы, сегменты, строки таблицы, пункты меню, switch — не имеет своего focus-ring; клавиатурный фокус там рисуется UA-дефолтным кольцом WebKit, которое визуально не совпадает с каноничным «outline: 2px solid var(--accent); offset 2px». При этом комментарий global.css:1–2 утверждает «wk.css устанавливает body / box-sizing / focus-visible reset» — в wk.css нет ни одного правила focus-visible. Стратегия фокуса получилась реактивной (добавляется по a11y-ревью новых фич), а не системной.
FIX: Один системный блок в wk.css: `:where(.btn,.iconbtn,.navitem,.tab,.switch,.seg button,.trow,.menu-item,.lrow,.chip):focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }` и поправить комментарий global.css.

### [MEDIUM|unverified] --text-faint (#9C9FAB на #FAFAFB ≈ 2.5:1) используется как цвет реального текста — заголовки колонок, section-labels, таймкоды
apps/desktop/src/styles/tokens.css:58
Контраст --text-faint в light ≈2.5:1 (провал даже 3:1 для large text), в dark (#5C5E68 на #0D0D11) ≈3.0:1. При этом им покрашен не декор, а несущий текст: заголовки колонок таблицы .tbl-head (wk.css:302, 10.5px uppercase), .sec-label (wk.css:223), .menu-label (wk.css:241), таймкоды транскрипта .turn-time (wk.css:327), .nav-meta, .set-eyebrow, .kbd (wk.css:162). Проект сам признал проблему точечно — B24.7 фикс «text-3 вместо text-faint — контраст 4.5:1» (components.css:1474) — но остальные ~15 мест на 10.5px тексте остались. Отдельно: --text-3 #71747F на --bg #FAFAFB даёт 4.46:1 — формально ниже 4.5:1 (на --panel проходит).
FIX: Для текстовых ролей (tbl-head, sec-label, menu-label, turn-time) перейти на --text-3; --text-faint оставить плейсхолдерам/декору. Либо поднять light --text-faint до ~#82858F.

### [MEDIUM|unverified] Switch 34×20px — ниже минимума 24px WCAG 2.5.8, без hit-area расширения и без focus-ring
apps/desktop/src/styles/wk.css:193
.switch — интерактивный <button> (ui/Switch.tsx:16–27) размером 34×20: высота на 4px меньше минимального таргета 24×24 SC 2.5.8 (AA в WCAG 2.2, на который проект ссылается в своих a11y-фиксах). Показательно, что для button.chip (22px) хитбокс уже расширили паттерном ::after inset:-3px (components.css:1521–1526), а Switch — контрол, который дёргают чаще всего в Settings — этот же паттерн не получил. Focus-visible у него тоже отсутствует (см. смежную находку), т.е. худший комбо: маленький, без видимого фокуса.
FIX: Добавить .switch { position:relative } и .switch::after-хитбокс inset: -2px -3px (до 40×24), плюс focus-visible ring.

### [LOW|unverified] Канонический README дизайн-системы ссылается на удалённый legacy-tokens.css shim как на действующий
docs/design/wotold-v2/README.md:37
README (канон, который CLAUDE.md обязует читать перед любой UI-работой) утверждает: «Atelier-имена … держатся через styles/legacy-tokens.css shim до B18.6». Файла styles/legacy-tokens.css не существует (ls styles/ = components/fonts/global/tokens/wk), миграция B18.6 завершена, shim удалён (что CLAUDE.md фиксирует). Раздел «Статус миграции» тоже застыл на «B18.0 → B18.6», хотя система уехала до B30. Для документа со статусом «источник истины» это прямое противоречие коду.
FIX: Убрать абзац про shim, обновить статус, добавить упоминание components.css как второго слоя и assistant.md как addendum.

### [LOW|unverified] Скелетон детали звонка мимикрирует под legacy-транскрипт (.transcript-row 130px/17px), а реальный контент рендерится v2-классом .turn (140px/15px)
apps/desktop/src/components/call-detail/CallDetailSkeleton.tsx:54
CallDetailSkeleton рисует ghost-строки классами .transcript-row/.transcript-speaker/.transcript-text (Atelier-порт: grid 130px 1fr 60px, текст 17px, components.css:144–191), а живой транскрипт — InteractiveTranscript.tsx:206+ на v2 .turn (grid 140px minmax(0,1fr), текст var(--t-15), wk.css:324–328). При загрузке → контенте меняется и колоночная сетка, и кегль — layout-jump, ровно то, от чего скелетоны должны защищать. Это видимый шов между двумя поколениями DS, живущими в одном экране.
FIX: Переписать скелетон на .turn-разметку (turn-sp/turn-text ghost), а .transcript-* либо выпилить, либо оставить только там, где legacy-вид намеренный.

### [LOW|unverified] Три параллельных shimmer-системы (sk / ds-skeleton-shimmer / wt-skel-shimmer) и мёртвая ссылка на удалённый wotold.css
apps/desktop/src/ui/ui.css:11
Одинаковый skeleton-паттерн реализован трижды: .skeleton + @keyframes sk (wk.css:268–269), .ds-skeleton + ds-skeleton-shimmer (ui.css:11–35, именно его использует ui/Skeleton.tsx:29), и wt-skel-shimmer (components.css:24–27, переиспользуется ghost-строками и text-shimmer). Header ui.css при этом ссылается на «styles/wotold.css» — файл удалён в B18.6b. Визуального расхождения почти нет, но канонический .skeleton из uikit фактически мёртв, а какой из трёх — «канон», ни один док не говорит.
FIX: Свести к одному keyframes + одному классу (логичнее ds-skeleton → .skeleton из wk.css), обновить header ui.css.

### [LOW|unverified] Canvas-фоллбек '#ECEAE3' — тёплый Atelier-paper, отсутствующий в палитре Wotold v2
apps/desktop/src/components/DualWaveform.tsx:95
При недоступности computed style разделительная линия waveform падает в #ECEAE3 — это тон старой Atelier-гаммы (тёплый беж), тогда как все v2-бордеры холодно-серые (#E9EAEE/#DFE1E6, tokens.css:51–53). Фоллбеки --text/--accent на соседних строках 91–92 корректно синхронизированы с v2 (#1A1B23/#3C3D49) — этот один пропущен при миграции. Проявится редко (race на первом кадре), но в dark-теме фоллбек ещё и инвертирован неверно — светлая линия.
FIX: Заменить на #E9EAEE (light --border) и вынести все три фоллбека в именованные константы с комментарием-ссылкой на tokens.css.

## Insights
- Источник истины дизайн-системы физически лежит вне репозитория (~/Downloads/Wotold v2/, ~/Downloads/design_handoff_wotold_assistant/) — README канона это честно фиксирует, но воспроизводимость нулевая: любой, кроме автора, не может сверить код с прототипом, а drift уже виден (stale-ссылка на legacy-tokens shim). Прототипы стоило бы вендорить хотя бы в docs/design/ как snapshot.
- CSS-файлы ведутся как decision-log: почти каждое правило снабжено маркером итерации ([B27.5], [V6.9], [S8]) с обоснованием «почему так» — включая уроки вроде коллизии имён .rail vs .progress-rail (components.css:339–342). Это редкая и ценная археология для solo-проекта, но components.css уже 1671 строка и растёт как append-only лог без секционного оглавления.
- Токен-дисциплина по цвету — enforced (хук ловит hex/oklch), а по типографике и spacing — нет: 77 сырых fontSize: NN и 400+ inline style={{}} в pages/ означают, что шкала --t-11..28 и --s1..9 фактически advisory. Дизайн держится на самодисциплине автора, и это уже видно по components.css, где портированные Atelier-классы живут в rem (.display 3.4rem, .title 1.6rem) рядом с px-шкалой uikit.
- Паттерн «канонизации» работает: B21/B23-fix комментарии показывают, что при обнаружении расхождения с прототипом класс не патчится точечно, а переписывается под канон с фиксацией в комментарии (.input--box: «прежние 14px/r-xs были Atelier-легаси»). Это дешёвая замена визуальной регрессии.
- Focus-management развивается реактивно: каждый a11y-фикс (B24.7, светофоры) добавляет focus-visible точечно новым классам, но базовый слой wk.css так и не получил системного правила — классический случай, когда ревью фич не ловит долг фундамента.
- Спикер-палитра --sp1..5 — единственные хроматические цвета системы — грамотно изолирована: используется только для голосов/аватаров и как сырьё для conic-градиента ai-field, с явным запретом на прочее (assistant.md:34–35). Такая формализация «где цвету можно» — зрелый ход для моно-палитры.
- Кастомные traffic-lights (--wc-close/min/max, tokens.css:32–35) сознательно выведены из графит-моно как «функциональные оконные цвета» с комментарием-обоснованием — правильный способ оформлять исключения из собственных правил.


# AREA: contracts-ci — score 6/10
## Area label: Контракты (packages/contracts) + CI/CD (.github/workflows) + dev-харнесс (scripts/hooks, .claude/settings.json)

## Strengths
- S1-изоляция секретов между джобами реально соблюдена на уровне ссылок: release-app.yml (строки 105–106) видит только TAURI_SIGNING_PRIVATE_KEY(+PASSWORD), deploy-proxy.yml (79–80, 136–137) — только CLOUDFLARE_API_TOKEN/ACCOUNT_ID, партнёрские ключи referenced только в ручном sync-proxy-secrets.yml, а claude-review.yml использует отдельный ANTHROPIC_API_KEY, не боевой прокси-ключ (суффиксы _STAGING/_PRODUCTION).
- Нулевое дублирование контрактных типов: rg по interface DiarizedTranscript|RecapJson|TauriUpdaterManifest|CallSummaryV2|AssistantAnswer вне packages/contracts даёт только сниппет в docs/M15_ASSISTANT_PRD.md; 31 файл импортирует @wotold/contracts. Триггер deploy-proxy.yml включает packages/contracts/** — изменение контракта автоматически редеплоит прокси.
- Coverage-гейты честные и реально валят CI: cargo llvm-cov report --fail-under-lines 50 (ci.yml:212–214), vitest thresholds 10/20/40 lines по пакетам (apps/desktop/vitest.config.ts:24–29, services/proxy/vitest.config.ts:15–21, services/mcp/vitest.config.ts:13–18) — совпадают с заявленным в CLAUDE.md «baseline 10-30%, цель 80%», без приписок.
- CI продуманно cost-engineered: dorny/paths-filter с 4 фильтрами и комментарием с расчётом экономии (ci.yml:17–19), concurrency c cancel-in-progress только для PR (ci.yml:8–10), Swatinem/rust-cache с shared-key per-job (ci.yml:164–167, 196–199), pnpm-кэш через setup-node.
- Deploy-пайплайн прокси выше среднего: preflight-тесты → environment-gated staging/production → smoke-тест /health с 5 ретраями → авто-rollback через wrangler rollback при провале smoke (deploy-proxy.yml:85–111, 140–166), с обходом известной поломки wrangler-action на pnpm workspace-протоколе (комментарий 72–74).
- pre-write.mjs — качественный guard: negative lookahead для .env.example, покрытие .dev.vars, *.key/*.pem, SSH id_*, tauri-signing по regex, с внятными сообщениями почему заблокировано и что делать.
- Контракты хорошо документированы по месту: tagged union ModelStatus с явным объяснением почему downloading не state (local-engine.ts), naming-convention отступление M12 snake_case задокументировано в точке несоответствия (local-engine.ts, комментарий Naming convention), AssistantStatusEvent помечен как сознательное исключение из camelCase с запретом «чинить односторонне» (assistant.ts, конец файла).

## Findings

### [MEDIUM|unverified] Автоаллоу Bash(rm:*) — деструктивные команды без подтверждения
.claude/settings.json:35
permissions.allow содержит "Bash(rm:*)" (строка 35) и "Bash(git checkout:*)" (строка 24): агент может выполнить rm -rf по любому пути и git checkout -- . (сброс незакоммиченных правок) без промпта. Это противоречит собственному ECC-правилу hooks.md («Auto-Accept Permissions — use with caution») и заметно шире необходимого: остальной список аккуратно гранулирован (git restore --staged:*, отдельные git-подкоманды).
FIX: Убрать rm:* из allow (оставить точечные вроде rm -rf node_modules при желании) или сузить до путей внутри репо.

### [LOW|unverified] Гейт 800 строк фактически работает только для Write, не для Edit
scripts/hooks/pre-write.mjs:51
Хук считает строки в tool_input.content ?? new_string; для Edit new_string — это фрагмент замены, а не итоговый файл, поэтому файл можно неограниченно наращивать серией Edit'ов, не задев лимит. Комментарий хука обещает блокировку «запись файлов >800 строк» без этой оговорки. Ср. эталонный вариант в ECC web/hooks.md, который тоже читает только content — но там matcher только Write, здесь же matcher Write|Edit создаёт ложное ощущение покрытия.
FIX: Для Edit читать текущий файл с диска и оценивать результирующий размер (old_string→new_string дельта), либо честно сузить matcher гейта до Write.

### [LOW|unverified] Тип lang: 'auto' | string схлопывается в string — литерал 'auto' не даёт ни проверки, ни автокомплита
packages/contracts/src/proxy-api.ts:34
Union литерала с его супертипом ('auto' | string) нормализуется TS в string: контракт не отличает 'auto' от произвольной строки на уровне типов, а IDE не подсказывает 'auto'. Валидация на прокси вынуждена дублировать семантику вручную.
FIX: Использовать идиому `'auto' | (string & {})` для сохранения автокомплита, либо отдельный тип `type SttLang = 'auto' | Bcp47` с брендом.

### [LOW|unverified] CONTRACTS_VERSION — мёртвая константа: нигде не потребляется и не бампалась
packages/contracts/src/index.ts:1
export const CONTRACTS_VERSION = '0.0.1' не референсится ни в приложении, ни в прокси, ни в MCP (rg по репо — ноль потребителей), и осталась 0.0.1 несмотря на добавление summary_v2 (M14), local-engine (M12) и assistant (M15). Реальное версионирование живёт в per-schema литералах (version: 1, schema_version: 2) — это работает, но экспортированная «версия пакета контрактов» создаёт ложный сигнал о существующем механизме совместимости.
FIX: Удалить константу либо начать реально проверять её (например, прокси возвращает contractsVersion в /health и клиент сверяет).

## Insights
- Дисциплина «contracts as single source» реально соблюдается — это редкость: ноль дублей типов вне packages/contracts, 31 файл-потребитель, deploy-триггер прокси включает packages/contracts/**. Но версионирование выбрано per-schema-литеральное (version: 1 в DiarizedTranscript/RecapJson, schema_version: 2 в CallSummaryV2), а Rust-зеркала (summary_v2.rs, assistant/types.rs) синхронизируются руками без cross-language schema-теста — дрейф TS↔Rust ловится только в рантайме.
- В CI применяется паттерн «комментарий-как-постмортем»: ci.yml:146–153 документирует, почему исходный замысел (Rust-линт на дешёвом Linux) был неверен (compile_error! R4 + безусловные ссылки на local_engine), с точным механизмом поломки. Это заметно повышает выживаемость решений при будущих «оптимизациях».
- Качество hook-слоя неравномерно и выдаёт эволюцию: три хука корректно читают stdin-JSON, а post-write.sh — единственный на bash — полагается на несуществующий env var и молча превратился в no-op. Симптом более общей проблемы: у dev-харнесса нет ни одного теста/smoke-прогона на сами хуки, при том что проект в остальном тестами обвешан.
- Оба бага release-app.yml (дубликат args: и подпись после аплоада) объединяет одно: workflow ни разу не исполнялся (git tag пуст). Релизный пайплайн — единственный непрокатанный путь CI и одновременно самый нагруженный секретами (ключ подписи апдейтов). Стоило бы прогнать dry-run на тестовом теге/act до первого настоящего релиза.
- Экономика CI посчитана буквально в долларах в комментариях (ci.yml:17–19: «6 jobs × $0.08/min = ~$1.70 за commit → экономия ~70%») — path-фильтры включают сам ci.yml во все четыре фильтра, что корректно инвалидирует кэш решений при правке workflow. Грамотная деталь, которую часто забывают.
- Комбинация «tags + paths» в deploy-proxy.yml:10–16 выглядит подозрительно, но корректна: GitHub не применяет path-фильтры к пушам тегов, поэтому продакшен-деплой по v*.*.* сработает всегда — однако это неочевидное поведение никак не откомментировано, хотя рядом задокументированы куда менее тонкие вещи.
- Coverage-стратегия — честная «трещотка» вместо карго-культа: заявленные ECC 80% не имитируются, в конфигах стоят реальные достижимые пороги (10/20/40/50) с комментариями о плане подъёма. Это лучше, чем типичный фейковый гейт, но и означает, что «80% минимум» из ECC-правил в этом репо — декларация.


# AREA: test-quality — score 7/10
## Area label: Качество тестов всего репозитория (Rust core, frontend desktop, proxy, MCP)

## Strengths
- Proxy-интеграционные тесты — образцовые behavioral-тесты: services/proxy/src/routes/stt.integration.test.ts гоняет реальный Worker через SELF (@cloudflare/vitest-pool-workers) с in-memory KV/R2, проверяет quota 429 (stt_sec), presigned URL, KV-resume happy path с fetch-mock'ом, который бросает на неожиданный upstream-вызов (строка 288: 'unexpected fetch in test') — то есть ассертит именно поведение, а не вызовы моков.
- Rust db-слой тестируется на реальном SQLite (fresh_db), а не моках: cascade delete покрыт на SQL-уровне во всех критичных таблицах — apps/desktop/src-tauri/src/db/voice_samples.rs:257 (C5/B16 audit), db/calls/lifecycle.rs:903-943 (delete_call_and_samples + NULL source_call), db/assistant.rs:557-591 (FTS cascade triggers), db/telemetry.rs:118, db/decisions.rs:212, db/chunks.rs:707.
- apps/desktop/src-tauri/src/pipeline/audio_merger.rs — тесты уровня 'data-loss critical' сделаны правильно: реальные WAV-файлы, конкурентный merge из потоков (concurrent_merges_of_same_track_all_succeed), ассерты на отсутствие tmp-мусора (assert_no_tmp_leftovers), атомарная перезапись root, числовая (не лексикографическая) сортировка chunk'ов.
- Регрессионные тесты написаны против реальных прошлых багов M13: pipeline/chunk_recovery.rs (promote_legacy_root_to_chunk0, reconstruct_keeps_done_chunk_and_reruns_failed), commands/recording.rs (plan_final_chunk_reset_when_failed, duration_uses_file_size_not_header_field), pipeline/mod.rs:2275 (ensure_all_chunks_done halt-gate) — каждый закрывает конкретный класс бага 'дропнутый финальный chunk / halt-before-merge'.
- Golden-set harness (apps/desktop/src-tauri/src/pipeline/golden_eval.rs) — детерминированный system-level diff для parse→validate→strip→dedup рекапа по 10 JSON-кейсам без LLM-вызовов: редкий и правильный паттерн для LLM-пайплайна.
- Соотношение assert-on-result vs assert-on-mock здоровое: во frontend-тестах 74 toHaveBeenCalled против 1096 expect — тесты в основном проверяют результат, а не имплементацию. RecordingContext.test.tsx полностью прогоняет state machine записи (idle→recording→paused→resume→stop) с учётом накопленной паузы.
- Смоук-тесты честно самодокументированы: CallDetailPage.test.tsx строки 9-12 явно перечисляют что НЕ покрыто (pipeline event listeners, delete/reprocess) вместо имитации покрытия.

## Findings

### [MEDIUM|unverified] Оркестрация recover_chunked_call не покрыта — тестируются только листовые helpers
apps/desktop/src-tauri/src/commands/recording.rs:1204
Core recovery-флоу (recover_chunked_call_inner, строки 1204-1313: gate local-engine, reconstruct → цикл run_chunk с обработкой ошибок → merge trigger) не имеет ни одного теста — rg 'recover' в тест-моде recording.rs пуст. Покрыты только листья: chunk_recovery::reconstruct_chunk_rows (chunk_recovery.rs:177-252) и plan_final_chunk (recording.rs tests). Это crash-repair путь, в котором уже были продовые регрессии (M13: chunk-0 path mismatch, dropped final chunk) — именно glue-код, а не листья, ломался в прошлый раз. Headless-триггер через WOTOLD_RECOVER_CALL_ID (строка 1315+) тоже без тестов.
FIX: Вынести цикл '(re)transcribe chunks → merge' в функцию, принимающую run_chunk как замыкание/trait (как уже сделано для rotate_fn/enqueue_fn в chunk_orchestrator), и покрыть сценарии: все chunks done → сразу merge; частичный fail → статус failed; reconstruct error → ранний выход.

### [MEDIUM|unverified] Frontend coverage-gate lines:10% — беззубый: не поймает откат даже половины тестов
apps/desktop/vitest.config.ts:26
Порог lines/statements/functions = 10 при 599 живых тестах. Комментарий на строке 25 честно называет это baseline с целью 80% ([B7] follow-up), но ratchet не двигался: даже если удалить 80% тестов, gate останется зелёным. Фактический разрыв с правилами харнесса (80% в ~/.claude/rules и docs/ROADMAP) закрыт только в Rust (CI fail-under-lines 50, .github/workflows/ci.yml:214). Наибольшие непокрытые поверхности: hooks/useCallDetail.ts (516 строк — центральный data-hook с event listeners и mutating actions, smoke-тест страницы явно исключает их, CallDetailPage.test.tsx:9-11), hooks/useCallAudio.ts (285 строк, 0 тестов), pages/OnboardingPage.tsx, AccountSection/AppearanceSection/LocalEngineSection/PermissionsSection — 0 тестов.
FIX: Прогнать текущий фактический процент и поднять gate до 'actual минус 2-3пп' (ratchet), повторять при каждом заметном приросте. Приоритет новых тестов: useCallDetail (listeners + delete/reprocess), useCallAudio.

### [MEDIUM|unverified] Инъекционная защита MCP (M8.3/M8.4) не существует в тестах — свойство задекларировано, но нигде не проверяется
services/mcp/src/tools.ts:6
Комментарий в tools.ts:6 делегирует защиту от инъекций клиенту ('В MCP мы возвращаем raw markdown, защита от инъекций — обязанность клиента'), при этом CLAUDE.md и паспорт (M8.3, M8.4, W5) называют services/mcp security-модулем с защитой от инъекций инструкций через транскрипт. В tools.test.ts (18 тестов) нет ни одного кейса с транскриптом, содержащим instruction-injection payload — ни проверки экранирования/оборачивания, ни хотя бы snapshot-теста фиксирующего текущий (сырой) контракт. Если защиту когда-либо добавят или контракт «raw markdown» случайно изменится — регрессию нечем поймать.
FIX: Минимум: тест-фиксация контракта — транскрипт с 'IGNORE PREVIOUS INSTRUCTIONS...' возвращается байт-в-байт без исполнения/модификации, плюс тест что MCP-ответ не содержит системных путей. Если M8.3/M8.4 подразумевает wrapping — сначала свериться с паспортом, это расхождение кода и ТЗ.

### [LOW|unverified] Тест сортировки спит 1100ms реального времени ради секундной гранулярности timestamp
apps/desktop/src-tauri/src/db/calls/lifecycle.rs:850
list_calls_orders_by_started_desc делает tokio::time::sleep(1100ms), чтобы два insert_recording получили разные started_at (rfc3339 с секундной гранулярностью). Это +1.1s к каждому прогону cargo test и зависимость от wall-clock. Аналогичный sleep(150ms) на строке 1106.
FIX: Вставлять строки с явным started_at через прямой UPDATE/INSERT с фиксированными timestamp'ами ('2026-01-01T10:00:00Z' / '10:00:01Z') вместо ожидания реального времени.

## Insights
- Тестовая пирамида инвертирована на стыке integration: листовые функции покрыты образцово (audio_merger, chunk_recovery, merge, db-репозитории), но orchestration-glue живёт на ручных live-запусках (recover_chunked_call_inner, run_inner local path, useCallDetail listeners). Память проекта это подтверждает: все три бага M13 (chunk-0 mismatch, dropped final chunk, halt-before-merge) были именно в glue — и после них тесты написали на листья + gate, но не на сам recovery-флоу.
- У проекта нет абстракции времени, поэтому async-тесты синхронизируются реальными sleep'ами (~25 мест). Показательно, что правильный паттерн в кодовой базе уже есть — providers/transcription/retry.rs принимает инжектируемую sleep-функцию (строка 82) и его тесты детерминированы — но паттерн не обобщили на chunk_orchestrator/resource_queue.
- Coverage-гейты устроены как честный per-layer ratchet с документированными намерениями (Rust 50% enforced в CI с комментарием 'поднимаем consequently', proxy 20/40, mcp 40/50, desktop 10) — это осознанно лучше, чем фиктивные 80%, но ratchet-механизм ручной и, судя по desktop 10%, не двигается. Классический риск ratchet-стратегии: гейт фиксирует прошлое, а не защищает настоящее.
- Proxy использует двухуровневую vitest-схему (node-env unit + отдельный workers-pool проект для *.integration.test.ts, vitest.workers.config.ts), причём интеграционные тесты исключены из coverage-подсчёта (vitest.config.ts:8) — реальное покрытие поведением выше, чем показывают цифры lcov. CI гоняет оба уровня и дублирует прогон в deploy-proxy.yml перед деплоем — хорошая дисциплина.
- Качество тестов коррелирует с тем, был ли в модуле продовый инцидент: audio_merger (потеря аудио), chunk_recovery (M13), voice_samples cascade (B16 audit P0), ip16Prefix (Sec) имеют параноидальные тесты с концаррентностью и malformed-входами; модули без инцидентов (llm route, onboarding, settings-секции) — нулевые. Тестовая стратегия де-факто реактивная, а не PRD-driven TDD, который декларируют правила харнесса.
- Русскоязычные имена-описания в assert-сообщениях и комментарии прямо в тестах ('abort ждущего должен убрать его из очереди') делают тесты самодокументируемыми — падение сразу объясняет нарушенный инвариант; это лучше среднего по индустрии.


# AREA: docs-product — score 7/10
## Area label: Документация и продуктовая целостность (ПАСПОРТ, ROADMAP, CLAUDE.md, README, docs/design)

## Strengths
- ПАСПОРТ_ПРОЕКТА.md — редкий по качеству ТЗ-документ: трёхуровневый scope [MVP]/[SCAFFOLD]/[DEFERRED] (§3), сознательные ограничения R1–R13 с маркерами в коде (§12), acceptance-критерии (§14), правила изоляции секретов S1–S4 (§15) и даже экономический вердикт по Cloudflare free-тиру (§16.2). Документ реально управляет кодом, а не лежит мёртвым.
- ROADMAP.md синхронен с кодом: выборочная проверка 7 отмеченных пунктов подтвердила все — M15.2 (migrations/0019_assistant.sql + src/db/assistant.rs), M15.10 (0020 + db/assistant_embeddings.rs + assistant/embed_cache.rs), M16.6 (0021_assistant_call_meta.sql с CHECK 'call_meta'), B25 (Cargo.toml:106 `default = ["voice-onnx", "assistant-embed"]` + commands/assistant.rs semantic_search), B27.6 (commands/share.rs share_text), B28.2 (recover_chunked_call/.auto-recover-tries в lib.rs и commands/recording.rs), B30.3 (.side-list-foot в 3 страницах). Ни одного фантомного чекбокса.
- MCP-сервер реализован точно по паспорту M8.2: все 7 read-only инструментов на месте (services/mcp/src/tools.ts:84–209), с тестами.
- docs/DEPLOYMENT.md — настоящий runbook: single-time setup Cloudflare, bootstrap per-env, staging/production триггеры. Прокси-часть проекта воспроизводима по докам с нуля.
- Гигиена статуса: история вынесена в ROADMAP_ARCHIVE.md (763 строки), живой ROADMAP читается; release-блокеры честно выделены в секцию A (#42 minisign, #44 CF production, security-scan, manual QA) — проект не притворяется релизнутым.
- PRIVACY.md + таблица приватности в README честно описывают потоки данных по трём режимам (Local/Managed/BYO), включая признание R2-staging и HuggingFace-скачивания.

## Findings

### [MEDIUM|CONFIRMED] README рассказывает пользователю ложный статус продукта
README.md:37
Троблшутинг утверждает «Local-движок ещё не активирован — failed_reason: local_engine_not_yet_wired… wire-up ждёт PRD §14», хотя local engine давно полностью реализован (src-tauri/src/local_engine/{stt,llm,engine,…}.rs; M12–M16 закрыты; единственное упоминание маркера в коде — негативный ассерт pipeline/mod.rs:2793). Строка 5 называет статус «pre-MVP», хотя паспорт §11 фиксирует «этапы 1–12 реализованы». Строка 26 обещает саммари «через 10–30 секунд», что противоречит M13-цели ~3-4 мин на длинном звонке. README — единственный внешний документ, и он врёт в обе стороны.
FIX: Переписать статус, троблшутинг и тайминги под фактическое состояние (local engine — default и работает).

### [MEDIUM|unverified] Паспорт противоречит коду по LLM-провайдеру managed-пути
docs/ПАСПОРТ_ПРОЕКТА.md:137
Паспорт (§3 стр.53, §5 стр.137, M4.1 стр.273) фиксирует: LLM для рекапа — Anthropic API. Фактически прокси по умолчанию использует Groq llama-3.3-70b (services/proxy/src/lib/llm-backends.ts:51-57: «groq если есть GROQ_API_KEY, иначе anthropic»), README:39 это подтверждает. Паспорт объявлен источником истины (W6, S4), но пивот на Groq в него не внесён — верификация «по паспорту» даст ложный FAIL, а квота llm_tokens_used в §10 сформулирована под другой ценник/качество.
FIX: Аддендум в §5/§7 паспорта: managed-LLM = конфигурируемый бэкенд (Groq default / Anthropic), с фиксацией причин.

### [MEDIUM|unverified] Флагманская фича M15/M16 (RAG-ассистент) отсутствует в паспорте
docs/ПАСПОРТ_ПРОЕКТА.md:344
Раздел 7-bis перечисляет пост-MVP майлстоуны только M12–M14; статус-строка (стр.420) — тоже. M15 «Ассистент» и M16 «Recall» — крупнейший реализованный пласт продукта (11 задач M15 + 7 задач M16 + батчи B24–B27, миграции 0019–0021, отдельный PRD) — существуют только в ROADMAP/PRD. При задекларированном правиле «при расхождении PRD и паспорта побеждает паспорт» самая большая фича формально не имеет статуса в источнике истины: у неё нет ни scope-маркера, ни acceptance-критериев на уровне ТЗ.
FIX: Добавить M15/M16 в §7-bis (по образцу M12–M14: абзац + ссылка на PRD) и обновить статус-строку §11.

### [MEDIUM|unverified] Лицензия «TBD», LICENSE-файла нет — при публичной дистрибуции через GitHub Releases
README.md:103
README инструктирует пользователей скачивать .dmg из github.com/zdllucky/wotold/releases, но LICENSE* в корне отсутствует (проверено ls), раздел лицензии — «TBD». Для внешнего мира это репо «all rights reserved»: юридически нельзя ни использовать, ни контрибьютить; для privacy-сегмента ЦА (юристы, безопасники) отсутствие лицензии на инструмент, пишущий переговоры, — прямой блокер доверия.
FIX: Выбрать лицензию (или явный proprietary EULA для бинарей) и закоммитить LICENSE до публичного релиза — добавить в секцию A release-блокеров ROADMAP.

### [LOW|unverified] Внутреннее противоречие CLAUDE.md и ROADMAP по QA-матрице акцентов
CLAUDE.md:118
CLAUDE.md:96 фиксирует «акцент один — графит, picker убран в B18.5», но воркфлоу-секция (:118) всё ещё требует «проверка всех 6 theme×accent комбинаций». ROADMAP.md:169 в release-блокерах тоже требует «6 theme×accent (light/dark × bordeaux/persian/ink)». Реально осталось 2 комбинации (light/dark × ink); агент, следующий инструкции буквально, будет тратить время на несуществующие режимы или сочтёт QA невыполнимым.
FIX: Заменить в обоих файлах на «light + dark (акцент ink)».

### [LOW|unverified] Дизайн-README устарел относительно завершённой миграции B18.6
docs/design/wotold-v2/README.md:38
Документ утверждает, что Atelier-имена «держатся через styles/legacy-tokens.css shim до B18.6», хотя shim уже удалён (в apps/desktop/src/styles/ нет ни legacy-tokens.css, ни wotold.css; CLAUDE.md:72 прямо говорит «shim удалён»). Секция «Статус миграции» отсылает к «docs/ROADMAP.md §B18», которого в живом ROADMAP больше нет — секция уехала в ROADMAP_ARCHIVE.md.
FIX: Обновить статус миграции: shim удалён, ссылка → ROADMAP_ARCHIVE.md.

### [LOW|unverified] README не содержит инструкций сборки/запуска для разработчика
README.md:65
Секция «Требования» перечисляет Node/pnpm/Rust/macOS, но нигде нет команд `pnpm install`, `pnpm tauri dev`, запуска тестов или `wrangler dev` — единственная команда в README это `cargo install tauri-cli`. Онбординг фактически живёт в CLAUDE.md (написанном для агента) и в приватной памяти пользователя (launch-app-worktree.md с gotchas «pnpm install first, no cd»). Человек-разработчик без Claude-контекста собрать проект по докам не сможет.
FIX: Добавить в README секцию Development: install → dev-запуск desktop/proxy/mcp → тесты (cargo test, pnpm -r test).

## Insights
- Документация выстроена слоями как код: паспорт (требования) → PRD на майлстоун → ROADMAP (статус) → ARCHIVE (история), и правило «паспорт побеждает» продублировано в 4 местах (паспорт W6, CLAUDE.md дважды, ROADMAP). Но слой-иерархия начала инвертироваться: реальная спека продукта после пивота (local-first + Groq + ассистент) живёт в PRD/ROADMAP/README, а «источник истины» паспорт остановился на версии 0.2 и постепенно становится историческим документом — противоречия по Groq и отсутствие M15/M16 это симптомы одного процесса.
- ROADMAP-записи — это одновременно engineering log с аудируемыми артефактами: счётчики тестов на батч (804 rust + 562 vitest), вердикты ревьюеров (rust-reviewer: 0 CRIT/HIGH), метрики live-гейтов (M16.7: 18/18 при исходных 2/20). Выборочная проверка показала нулевой дрейф от кода — это исключительная дисциплина. Обратная сторона: записи вроде M15.7 — абзац плотного шифра, читаемый только автором и агентом; для команды >1 человека этот формат не масштабируется.
- MCP-инструмент find_calls_by_contact принимает contact_name вместо contact_id из паспорта M8.2 (services/mcp/src/tools.ts:186) — прагматичное отклонение в пользу эргономики LLM-клиента (Claude не знает внутренних uuid), нигде не задокументированное как deviation.
- Продуктовый анализ. ЦА реальна и узка: privacy-чувствительные профессионалы на macOS с RU/KK code-switching — сегмент, который Otter/Fireflies/Granola обслуживают плохо. Ядро ценности подтверждено кодом: (1) local-first движок «бесплатно навсегда» — настоящий дифференциатор против подписочных конкурентов; (2) диаризация + голосовые отпечатки контактов с подтверждением — то самое «Notion не различает голоса» из §1; (3) ассистент после M16 (18/18 живых вопросов) — сильнейший демо-актив; (4) MCP — дальновидная ставка на экосистему Claude. Спорный балласт: auth-scaffold M10 (SSO трёх провайдеров, «ничего не разблокирует») — осознанный SCAFFOLD, но это самый дорогой из скаффолдов при нулевой пользе юзеру.
- Где продукт сырой (честно): дистрибуция — слабейшее звено. Updater мёртв без minisign-ключей (#42), CF production не затегирован (#44), manual visual QA ни разу не пройден целиком, security-scan local_engine не сделан, лицензии нет, нотаризации нет (R6 принят, но для ЦА «юристы и безопасники» Gatekeeper-пляска — реальный барьер доверия). Плюс секция B ROADMAP признаёт: pipeline::run/reprocess_call — центральный оркестратор — без unit-тестов. Продукт «работает у владельца», но между этим и «можно дать чужому человеку» — вся секция A целиком.
- Чего не хватает до юзабельного продукта за пределами release-блокеров: импорт контактов (M6.4 DEFERRED — адресная книга заполняется руками, это боль первой недели), экспорт/шаринг рекапов за пределами mailto/NSSharingService (нет md/pdf-экспорта пачкой), и хоть какой-то англоязычный onboarding — весь README на русском при английском UI-локали в i18n ×3.

---
---

# ПРИЛОЖЕНИЕ B — Находки, восстановленные из журнала workflow
*(их верификаторы упали по session-limit; все 10 верифицированы вручную точечными проверками кода — подтверждены все)*

=== rust-pipeline: 1 lost ===
[HIGH] substring_fuzzy_score: O(|transcript|·|quote|²) CPU без cap на длину цитаты, синхронно на async-executor
  apps/desktop/src-tauri/src/pipeline/summary_validator.rs:157
  Sliding-window Levenshtein (цикл for start in 0..=max_start на line 157) выполняет для КАЖДОЙ позиции окна полный Levenshtein O(n²). Комментарий на line 182-183 утверждает «m, n ≤ 200 (quote length cap)», но cap нигде в коде не enforced — ev.quote приходит от LLM без усечения (check_evidence:301, evidence_ok:367). Fast-path contains() спасает только verbatim-цитаты; весь смысл валидатора — фабрикованные цитаты, для которых всегда идёт медленный путь. Для 1-часового звонка (~60k нормализованных символов) и 200-символьной цитаты это ~60k окон × 40k операций = ~2.4 млрд операций на цитату; при более длинной цитате — квадратично хуже. Вызывается синхронно из async persist_summary_v2 (recap.rs:330 strip_unverified_evidence) без spawn_blocking — блокирует tokio worker на секунды-десятки секунд на каждый рекап с несколькими неподтверждёнными цитатами.
  FIX: Enforce cap: усечь needle до ~200 chars перед сравнением; шагать окном не по 1 символу (stride n/4 + локальное уточнение) или заменить на banded Myers bit-parallel; обернуть strip_unverified_evidence в spawn_blocking.


=== docs-product: 1 lost ===
[HIGH] Источник истины дизайна лежит вне репозитория (~/Downloads)
  docs/design/wotold-v2/README.md:11
  Канон UI объявлен как «спека = код прототипа» в `~/Downloads/Wotold v2/` (uikit.css + wk-*.jsx), а хендофф ассистента — `~/Downloads/design_handoff_wotold_assistant/` (assistant.md:9). CLAUDE.md:72,76,81 требует сверять каждый экран с этими файлами в рамках обязательного design gate. Любой разработчик (или агент) на другой машине физически не может пройти design gate — референс невоспроизводим, не версионируется и может быть удалён очисткой Downloads.
  FIX: Закоммитить прототип в docs/design/wotold-v2/_reference/ (по прецеденту atelier-v2/_reference) либо явно перенести статус источника истины на wk.css/components.css/tokens.css в репо.


=== design-system: 1 lost ===
[HIGH] Вся типографическая идентичность грузится с Google Fonts CDN — offline app остаётся без брендовых шрифтов
  apps/desktop/src/styles/fonts.css:10
  Hanken Grotesk и IBM Plex Mono подключены через @import url('https://fonts.googleapis.com/...'). Для local-first приложения (принцип «Локальное-первое» в CLAUDE.md) это значит: без сети оба шрифта молча падают в -apple-system/Menlo — исчезает весь пункт «Шрифты» дизайн-канона (README wotold-v2:21). Плюс каждый запуск шлёт запрос в Google (fingerprint-утечка для privacy-позиционируемого продукта записи звонков). Файл сам признаёт это TODO «убрать до публичного релиза» (строки 13–20 с готовым закомментированным OPTION B), но это не покрыто R1–R13 и блокирует релиз по собственному определению.
  FIX: Выполнить OPTION B из этого же файла: положить woff2 в public/fonts/, переключить на self-hosted @font-face.


=== test-quality: 2 lost ===
[HIGH] Синхронизация через реальные sleep'ы в ~15 async-тестах оркестратора — flaky-паттерн
  apps/desktop/src-tauri/src/pipeline/chunk_orchestrator.rs:487
  Тесты chunk_orchestrator (mod tests с строки 364) синхронизируются с фоновой задачей через tokio::time::sleep с реальным временем: 30-50ms на строках 487, 536, 591, 641, 833, 841, 893-949 и 250-300ms на 711, 758, 789, 852, 960. Комментарий на 486 прямо говорит 'Дать orchestrator'у обработать'. Под нагруженным CI-раннером (macOS shared runners) 30-50ms может не хватить — тест упадёт недетерминированно; а суммарно эти sleep'ы добавляют ~3s к каждому прогону. Тот же паттерн в pipeline/resource_queue.rs:422,427,446 (30ms) и audio/call_detect.rs:337.
  FIX: Заменить sleep-опрос на детерминированную синхронизацию: oneshot/Notify из тестового enqueue_fn (тест уже владеет make_enqueue_fn — сигналить из него), либо tokio::time::pause() + advance() где логика тайм-базирована. Проект уже знает правильный паттерн — providers/transcription/retry.rs:82 инжектирует sleep-функцию.

[HIGH] /v1/llm — ноль route-level тестов при полном интеграционном покрытии /v1/stt
  services/proxy/src/routes/llm.ts:13
  rg по 'v1/llm|llmRoutes' в *.test.ts не находит ничего. Тестируются только lib-функции (llm-backends.test.ts — callLlm, rate-limit.test.ts — readUsage/incUsage/quotaCap), но НЕ проверено на уровне роута: enforceQuota('llm_tok') → 429 при превышении, requireDeviceId wiring, incUsage после успешного ответа (строка 47 — если сломается порядок middleware или условие tokensUsed>0, квота llm перестанет списываться и никто не заметит), zod-граница llmRequestSchema с нормализацией null→undefined (строки 32-34). Для /v1/stt аналогичный набор покрыт в stt.integration.test.ts:203 ('enforces stt_sec daily quota'). Proxy — модуль из W5 security-триггеров паспорта (обход квоты).
  FIX: Добавить llm.integration.test.ts по образцу stt.integration.test.ts: 429 при квоте llm_tok на лимите, 401/400 без device-id, инкремент KV после успешного mock-ответа backend'а, 400 на невалидный body.


=== contracts-ci: 5 lost ===
[CRITICAL] Дублирующийся ключ args: в with-блоке tauri-action — релизный workflow сломан
  .github/workflows/release-app.yml:142
  В одном mapping with: заданы два ключа args: — строка 124 (`args: --features voice-onnx`, коммит 7fc89e3 «.dmg всегда с voice-onnx») и строка 142 (`args: --target universal-apple-darwin`, коммит fbe77f7 B16 P0). Парсер GitHub Actions отвергает workflow с дублирующимися ключами mapping («Duplicate key»), т.е. пуш первого же тега v* даст invalid workflow file и релиз не соберётся; даже при last-wins-парсинге --features voice-onnx молча выпадает и прод-DMG уходит без биометрического матчинга вопреки B3.7. Баг латентный: git tag пуст — workflow ни разу не запускался на теге.
  FIX: Слить в один ключ: `args: --features voice-onnx --target universal-apple-darwin` и удалить дубликат.

[HIGH] Ad-hoc codesign выполняется ПОСЛЕ того как tauri-action уже собрал DMG и загрузил его в Release — фикс B16 P0 не действует
  .github/workflows/release-app.yml:148
  Шаг «Ad-hoc codesign DMG/App» (строки 148–156) подписывает локальный .app в target/.../bundle/macos/ после шага tauri-action (строка 101), который в рамках одного действия собирает DMG и публикует ассеты в GitHub Release. Подпись не попадает ни в уже созданный DMG, ни в загруженные артефакты — пользователи получают неподписанный бинарь, и заявленный в комментарии фикс «damaged, move to trash» на macOS 14+ фактически не работает. (R6 — отсутствие нотаризации — принято сознательно, но данный шаг заявлен как работающая митигация и ею не является.)
  FIX: Подписывать через Tauri bundler hook (bundle > macOS > signingIdentity '-') или скриптом между build и upload (tauri-action args --no-bundle + ручная сборка DMG), чтобы подпись оказалась внутри публикуемого DMG.

[HIGH] post-write.sh — мёртвый хук: читает несуществующую CLAUDE_FILE_PATH и всегда выходит no-op
  scripts/hooks/post-write.sh:14
  FILE="${CLAUDE_FILE_PATH:-${1:-}}" — Claude Code не устанавливает переменную CLAUDE_FILE_PATH, а команда в .claude/settings.json (`bash "$CLAUDE_PROJECT_DIR/scripts/hooks/post-write.sh"`) не передаёт аргументов; payload приходит JSON'ом на stdin, что корректно делают остальные три хука (pre-write.mjs, tdd-warn.mjs, design-gate.mjs). Итог: FILE пуст, `[ -z ... ] && exit 0` — cargo fmt/check и tsc --noEmit после правок НИКОГДА не запускаются, хотя CLAUDE.md заявляет этот механизм как активный («PostToolUse … на .rs правках бежит cargo fmt + cargo check»). Отказ полностью тихий.
  FIX: Парсить stdin JSON (например node -e или jq -r .tool_input.file_path) как в соседних хуках; заодно учесть, что `timeout` на macOS есть только с coreutils.

[HIGH] Партнёрские ключи хранятся как repo-wide GitHub Secrets — прямое нарушение S1 «никаких repo-wide секретов для этих значений»
  .github/workflows/sync-proxy-secrets.yml:9
  Шапка файла (строки 9–18) прямо требует класть ANTHROPIC/SONIOX/GLADIA/R2/OAuth-ключи в Repository Secrets, а джоба sync (строка 38) не привязана к GitHub Environment — в отличие от deploy-proxy, где среды staging/production дают protection rules. CLAUDE.md/паспорт S1: «Партнёрские ключи … только в джобе деплоя прокси. Никаких repo-wide секретов для этих значений». Repo-level секрет может заreference'ить любой новый/изменённый workflow без environment-approval, и любой из непиненных сторонних actions внутри этой же джобы (pnpm/action-setup@v4 и т.п.) имеет к ним доступ в рантайме. provision-infra.yml:29–30 фиксирует то же для CF-токена как осознанный выбор, но для партнёрских ключей это против паспорта.
  FIX: Перенести суффиксованные партнёрские секреты в environment-scoped secrets (staging/production) и привязать джобу sync к environment: ${{ inputs.environment }}.

[HIGH] Ни один action не запинен по SHA; mutable-тег tauri-apps/tauri-action@v0 получает TAURI_SIGNING_PRIVATE_KEY
  .github/workflows/release-app.yml:102
  Во всех 7 workflow — 13 уникальных uses:, все по mutable-ссылкам (v0, v2–v6, @stable, @cargo-llvm-cov, @beta). Наиболее острая точка: tauri-apps/tauri-action@v0 (release-app.yml:102) исполняется с TAURI_SIGNING_PRIVATE_KEY+PASSWORD в env — компрометация тега v0 = кража ключа подписи авто-апдейтов = возможность разослать вредоносный апдейт всем пользователям (blast radius максимальный для проекта). Также anthropics/claude-code-action@beta (claude-review.yml:68) — заведомо движущийся тег с id-token: write и ANTHROPIC_API_KEY, и wagoid/commitlint-github-action@v6 — Docker-action. S1-изоляция секретов по джобам сделана аккуратно, но supply-chain-слой её обесценивает.
  FIX: Запинить все uses: на полный commit SHA (с комментарием-тегом), включить Dependabot/Renovate для github-actions ecosystem.


---
---

# ПРИЛОЖЕНИЕ C — Инлайн-аудит proxy / MCP / contracts (ручной, до workflow)
# Interim findings (inline review, до перезапуска workflow)

## Метрики (собраны)
- ~96k LOC: Rust ядро 46.8k, фронт TS/TSX 28.9k (+10.5k тестов), proxy 2k, mcp 0.5k, contracts 0.5k, swift 1.2k, css 4k
- 451 коммит, 2026-05-19 → 07-23, соло
- Тесты: 810 rust fn + ~750 vitest (86 файлов); prod unwrap() = 13 (10 в eval-харнесе), test unwrap = 1305
- TODO/FIXME 12, `any` 2, unsafe 6, allow(clippy) 17
- Deps: cargo 55, npm фронт 12 (очень скромно), 21 миграция
- Churn top: lib.rs (73), i18n ×3 (71), pipeline/mod.rs (68), CallDetailPage (63)

## MCP (services/mcp) — ревью сделано инлайн
Score ~8/10. Чисто: readonly better-sqlite3 (fileMustExist), все запросы параметризованы, zod-валидация, clamp limit 200, stdout только JSON-RPC.
Находки:
- MEDIUM: tools.ts readArtifact — call_id не валидируется как UUID → path traversal примитив `../../x` (ограничен файлами с именами transcript.md/recap.md, read-only). Фикс: UUID-regex на call_id.
- LOW: db.ts findContactsByName не эскейпит LIKE-wildcards (searchCalls эскейпит — непоследовательно). contact_name='%' = match all. Read-only, low.
- Замечание: анти-инъекция M8.3 = только описание в description тулов («treat as untrusted») — декларативная, не структурная (нет обёртки/маркеров). Паспорт допускает.

## Proxy (services/proxy) — ревью сделано инлайн
Score ~8/10. Сильно: CORS allowlist (после B16-аудита), /16 IP rate-limit, presign content-type allowlist (anti-phishing P0 fix), scrubProviderError (uuid/r2key/tokens/query), R8 соблюдён (байты не через воркер), KV job-cache resume для Soniox/Gladia, retry-CAS на KV-квоте (документировано как best-effort под R1), zod на всех boundary.
Находки:
- MEDIUM: POST /v1/stt принимает произвольный r2Key без проверки префикса `stt/<deviceId>/` — девайс A может транскрибировать чужой staged-объект если узнает ключ (ключ содержит uuid — угадать сложно). Фикс — 1 строка проверки префикса.
- LOW/ARCH: deep-link callback кладёт sessionId в `wotold://` URL — scheme hijacking возможен другим app (macOS не гарантирует эксклюзивность схемы). Пока аккаунт ничего не разблокирует (M10.3 scaffold) — риск отложенный, но задокументировать до облачной синхры.
- LOW: enforceQuota проверяет ДО, incUsage ПОСЛЕ → девайс на 99% квоты может закинуть сколь угодно длинный файл (последний запрос не ограничен). Сознательно soft (R1-дух).
- Наблюдение: POLL_BUDGET_MS 25s под Workers Free 30s wall — R7 честно обработан, retry через KV-кэш job.

## Contracts
Версионированы (version: 1 в DiarizedTranscript, CONTRACTS_VERSION), JSDoc-комментарии с ссылками на пункты паспорта. Чисто.

## Auth (proxy) 
state-flow с verifyState server-side (CSRF ок), identity-обновление, provider mismatch check. Хорошо.

## Осталось на workflow re-run (после 15:00 Almaty reset):
rust-pipeline, rust-db, rust-local-engine, rust-assistant, rust-audio(+swift), frontend, design-system, contracts-ci (CI-часть), test-quality, docs-product, market-research.
(mcp-server и proxy из списка areas УБРАТЬ — сделаны инлайн)
