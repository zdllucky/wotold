# Wotold — навигация для агента

Десктоп-утилита записи звонков с транскрипцией и диаризацией. macOS-first, локальное хранение, MCP для Claude.

## Источник истины

[`docs/ПАСПОРТ_ПРОЕКТА.md`](docs/ПАСПОРТ_ПРОЕКТА.md) — ТЗ. При расхождении паспорт побеждает (S4, W6).

**ВАЖНО — прочесть раздел 12 паспорта перед изменениями.** Сознательно принятые ограничения не «чинить» (R1/R5/R7/R8 — superseded: облако удалено, local-only 0.3):

| Маркер | Что принято |
|---|---|
| ~~R1~~ | ~~Free-тир абьюзится переустановкой~~ — **superseded (облако удалено, local-only 0.3)** |
| R2 | LLM-догадка спикеров — только booster, никакого автоприсвоения |
| R3 | Авто-детект «идёт звонок» не делаем, только ручная кнопка ([S1] **deviation opt-in**: настройка default OFF — Core Audio + frontmost-app whitelist. Никакая аудио-дорожка чужого app не читается. Аналог R2→V7 deviation.) |
| R4 | Windows-захват = `unimplemented!()` за trait `AudioCapture` |
| ~~R5~~ | ~~Биллинг/тиры = заглушки~~ — **superseded (облако удалено, local-only 0.3)** |
| R6 | macOS-сборка без Apple-нотаризации в MVP, Gatekeeper «Open anyway» норма |
| ~~R7~~ | ~~Free Cloudflare: перестаёт отвечать при превышении лимитов~~ — **superseded (облако удалено, local-only 0.3)** |
| ~~R8~~ | ~~Аудио НЕ через память прокси-воркера~~ — **superseded (облако удалено, local-only 0.3)** |
| R9 | Local-движок (M12) — только macOS в MVP, Linux/Windows trait+`unimplemented!()` |
| R10 | Local-движок: модели не бандлятся в installer (~50MB), download on-demand |
| R11 | Local STT — offline-only (sherpa-onnx Whisper). Live realtime captions НЕ делаем. Chunked 10-мин post-processing (M13, planned) допустим: тот же offline pipeline, разрез только для UX (pipelining + crash-safety). |
| R12 | Local-LLM саммари по качеству ниже cloud — UI явно показывает «●●○» |
| R12-bis | Авто-удаление моделей при смене preset НЕ делаем — explicit storage UI (M12.4.4-bis) |
| R13 | Слишком слабое железо НЕ блокирует Local — probe рекомендует Light с warning |
| R14 | Базовые модули обязательны, per-module тумблеров нет: не хватает любого — обработка стоит, звонки паркуются и поднимаются сами после докачки (B31) |

## Структура монорепо

```
apps/desktop/         Tauri 2 (фронт TS + Rust ядро + Swift sidecar Core Audio)
  src/                Фронтенд (TypeScript)
  src-tauri/          Rust ядро, capabilities, tauri.conf.json
  sidecars/macos-audio/  Swift sidecar (Core Audio process tap)

apps/site/            Публичный сайт (Astro 5 + Starlight) → GitHub Pages

services/mcp/         Локальный MCP-сервер (read-only)

packages/contracts/   ОБЩИЕ типы — DiarizedTranscript, Recap JSON, latest.json

.github/workflows/    CI/CD (path-filtered, S1 изоляция секретов между джобами)
docs/                 Паспорт и сопутствующие документы
.claude/              ECC dev-харнесс (W7 — не часть продукта, исключён из артефактов)
```

### Документация: три уровня

Репозиторий публичный, и у документации три разных адресата. Путать их не нужно.

| Уровень | Где | Для кого |
|---|---|---|
| **Пользователь** | `apps/site/src/content/docs/` | Установка, фичи, приватность, consent, MCP-гайд, FAQ, легал. Три локали (ru по умолчанию, en/kk с префиксом; непереведённое Starlight фолбэчит на ru) |
| **Контрибьютор** | `docs/`, `CONTRIBUTING.md`, `SECURITY.md` | Паспорт, ROADMAP, TECH_DEBT, PRD, дизайн-канон, RELEASING. Внутренние — по-русски; community-файлы в корне — по-английски |
| **Провенанс** | `docs/audits/`, `.claude/` | История находок и dev-харнесс. Публичны, на сайт не выносятся |

`docs/PRIVACY.md` и `docs/MCP.md` — **стабы**: канонический текст живёт на сайте. Правки контента идут туда, не в `docs/`.

### Сайт (`apps/site`)

- **Токены и примитивы не копируются** — `site.css` импортирует `apps/desktop/src/styles/{tokens,wk}.css` напрямую, шрифты синхронизируются `scripts/sync-fonts.mjs` на prebuild (`apps/site/public/fonts/` в `.gitignore`). `components.css` намеренно не подключается.
- **Импорты канона лежат в `@layer wotold.canon`.** Starlight держит свои стили в `@layer starlight.*`, а неслоистые правила выигрывают у слоистых независимо от специфичности — без слоя базовое `a { color: inherit }` из `wk.css` перебивало стили Starlight.
- **Ноль сторонних хостов** — enforced `scripts/check-site-assets.mjs` в CI и в `pages.yml`, а не ревью.
- `base` и `site` берутся из env (`SITE_BASE`, `SITE_URL`), дефолт — `/wotold` на `zdllucky.github.io`.
- Деплой — `.github/workflows/pages.yml`, только с `main`. На PR сайт лишь проверяется джобой `site` в `ci.yml`.

## Этапы реализации

Раздел 11 паспорта. Порядок строгий: 1 → 2 → 3 → 4 → 5, далее 6/7/8 параллельно после 5, авто-обновление (11) и CI (12) ведутся параллельно с 6–10, но обязательны до релиза.

**Декомпозиция фич и текущий статус** → [`docs/ROADMAP.md`](docs/ROADMAP.md). Это источник истины по тому что сделано/в работе/осталось. Параллельно ведётся TaskList в харнессе Claude Code — обновляется руками одновременно с ROADMAP при смене статуса.

**Технический долг** → [`docs/TECH_DEBT.md`](docs/TECH_DEBT.md) — сводка закрытого долга по итогам аудита 2026-07 (TD-01…TD-50, волны W1–W9; закрыты все 50). Карта TD-NN держит живыми ссылки из комментариев кода, раздел «Действующие решения» — принятые компромиссы. Развёрнутые постановки — в истории git, провенанс находок — [`docs/audits/`](docs/audits/).

## Контракты (S2)

Любое изменение формата `DiarizedTranscript`, рекап-JSON или `latest.json` правится в `packages/contracts` и потребляется приложением и MCP-сервером. Дублирование типов запрещено.

## Секреты (S1)

- **Tauri minisign приватный ключ** — только в джобе сборки приложения (`TAURI_SIGNING_PRIVATE_KEY` + пароль)
- Никаких repo-wide секретов для этого значения
- **[local-only 0.3]** партнёрские ключи (Soniox/Gladia/Anthropic) и джоба деплоя прокси удалены вместе с облачным сегментом
- Секреты будущих внешних интеграций (planned, keychain-seam `secrets.rs`) — только в системном keychain пользователя, не в БД, не в логах, не в CI

## Принципы

- **Локальное-первое (и единственное)**: запись, транскрипция, диаризация, саммари, поиск, ассистент, MCP, контакты — всё на устройстве, без сети (единственный сетевой поток — разовое скачивание моделей с HuggingFace)
- **Идентификация только подсказка**: никакой автопривязки контакта без подтверждения пользователя (M3, R2)
- **MCP read-only**: контент звонков — недоверенные данные, защита от инъекций инструкций (M8.3, M8.4)

## Инженерные правила (аудит 2026-07)

Выведены из **системных** находок аудита — тех, что повторились в разных слоях. Нарушение блокирует `/code-review`. Точечные находки живут задачами в [`docs/TECH_DEBT.md`](docs/TECH_DEBT.md), правила — здесь.

1. **Клей тестируется первым.** Любая оркестрация (`pipeline::run`, recovery-флоу, композиция хуков) получает happy-path + минимум один fail-path тест до mark-done. Покрытие листьев за покрытие клея не считается: все три прод-бага M13 были именно в клее, при параноидально покрытых листьях.
2. **Twin parity.** Фикс бага в одном из парных модулей обязан включать проверку близнеца на ту же дыру: `AudioRecorder` ↔ `ProcessTapRecorder`, chunk-FSM ↔ call-lifecycle, `searchCalls` ↔ `findContactsByName`. «Одинаковый контракт, разная зрелость» — главный источник будущих багов в этом репо.
3. **Деградация видима.** Путь «warn-and-continue», влияющий на результат звонка, обязан выставлять персистентный degraded-флаг, доступный UI (инфраструктура готова: `packages/contracts/src/degraded.ts`, миграция 0023, чип в шапке звонка). «Только в лог» запрещено: юзер не должен гадать, один там голос или система-трек ушла в speaker:0.
4. **i18n тотален.** Все user-visible строки — через `t()` и три локали, включая нижние слои (`api/errors.ts`, `utils/callMeta.ts`). Образец — `utils/modelLabel.ts` (принимает `TFn`). Русский литерал в UI-пути = замечание ревью.
5. **CPU >10мс не на async-executor.** Левенштейн, кластеризация, ONNX-инференс, WAV-чтение — только внутри `spawn_blocking`. На tokio-worker крутятся Tauri-команды UI; образцы правильного кода — `SortformerDiarizer.diarize_real`, `audio_merger`.
6. **Тесты без реального времени.** `sleep()` для синхронизации с фоновой задачей запрещён — инжектируемое время, `Notify`/oneshot или `tokio::time::pause()`. Образцы — `pipeline/resource_queue.rs`, `audio/call_detect.rs` (`Notify`); TD-32/TD-48 распространили это на оркестратор и очередь ресурсов.
7. **Границы доверия валидируются.** Любой id из webview или MCP валидируется (UUID) до участия в файловых путях; путь дополнительно проходит `ensure_path_under` (defense-in-depth). Новая Tauri/MCP-команда с параметром, попадающим в путь или SQL-паттерн, без этого не проходит ревью.
8. **800 строк меряются по итоговому файлу.** Не по диффу и не по фрагменту Edit'а (гейт `scripts/hooks/pre-write.mjs` считает итоговый размер и блокирует только **рост** за лимит: правку файла, который уже длиннее, он пропускает). Новый модуль планируется под лимит заранее — «порежем потом» не работает, см. `pipeline/mod.rs`. **Исключение — словари переводов** (`src/i18n/{ru,en,kk}.ts`, TD-49): правило про модули, а локаль — плоская таблица строк с одной когезией по определению. Резать её по неймспейсам значит платить тремя правками в трёх файлах за каждую новую строку и ловить расхождения между локалями. Гейт их пропускает намеренно.

## Design Gate (Wotold v2, [B18] — ОБЯЗАТЕЛЬНО до любой UI работы)

> **Wotold v2 (uikit) — действующий дизайн** (миграция завершена в B18.6; shim и файлы прошлого поколения удалены). Канон — [`docs/design/wotold-v2/`](docs/design/wotold-v2/README.md) + код: `wk.css` (примитивы) / `components.css` (app-классы) / `tokens.css`. Источник истины = прототип [`docs/design/wotold-v2/_reference/`](docs/design/wotold-v2/_reference/) (`uikit.css` + `wk-*.jsx`, открывается `index.html`). Для поверхностей **Ассистента** (M15/B24) — addendum [`docs/design/wotold-v2/assistant.md`](docs/design/wotold-v2/assistant.md) + хендофф [`docs/design/wotold-v2/_reference-assistant/`](docs/design/wotold-v2/_reference-assistant/).

Перед **любой** правкой `.tsx`/`.css`/`*.module.css`, или инлайн-стилей, **до** Plan/Implement:

1. Прочесть [`docs/design/wotold-v2/README.md`](docs/design/wotold-v2/README.md) (канон) и сверить экран с прототипом [`docs/design/wotold-v2/_reference/`](docs/design/wotold-v2/_reference/) (`wk-*.jsx` / `uikit.css`).
2. Запустить `/design-gate <surface>` или прочесть [`.claude/skills/design-gate/SKILL.md`](.claude/skills/design-gate/SKILL.md).
3. В чате выдать alignment-блок:
   ```text
   [design-gate] Surface: <page/component>
   Reference: docs/design/wotold-v2/_reference/<wk-file>|uikit.css
   Tokens used: <list>
   Classes used: <list>
   New tokens needed: <none | list>
   Logic preserved: <yes — list>
   A11y: <focus, target, ARIA>
   ```
4. Только после этого — Plan / Implement.

**Правила (см. design-gate skill для полного списка):**

- Все цвета/spacing/radius/shadow → `var(--*)` из [`apps/desktop/src/styles/tokens.css`](apps/desktop/src/styles/tokens.css). Запрещены сырые hex/oklch в `.tsx`/любом `.css` кроме handoff sources.
- Компонентные классы из [`apps/desktop/src/styles/wk.css`](apps/desktop/src/styles/wk.css) (uikit, канон): `.btn`(+`--primary/--default/--ghost/--soft/--danger`), `.iconbtn`, `.chip`, `.avatar`, `.dot`, `.kbd`, `.input`, `.field`, `.seg`, `.switch`, `.tabs`/`.tab`, `.navitem`, `.rail`/`.minirail`, `.menu`, `.overlay`/`.modal`, `.palette`, `.tbl`/`.trow`, `.turn`, `.doc`, `.rrail`, `.composer-dock`, `.rec-widget`, `.wave`, `.optioncard`, `.setting-row`. App-специфичные компонентные классы (transcript/pipeline/stat-tag/rec-float/banners/modal-frame и пр.) — в [`apps/desktop/src/styles/components.css`](apps/desktop/src/styles/components.css) (token-clean). React-обёртки над классами — `src/ui/*` (Switch/Segmented/IconBtn/Dot/Wave/Kbd/NavItem/Panel/Avatar/Chip/SettingRow/OptionCard/Modal/Menu + Button/Select/Tabs/Field/Badge/…). Иконки — `<Icon name=… />` из [`src/ui/Icon.tsx`](apps/desktop/src/ui/Icon.tsx) (line 1.5px, без emoji).
- `var(--danger)` (красный) — **только** запись и destructive actions. Все остальные акценты — `var(--accent)` (моно-графит `ink`). Токены прошлого поколения (`--signal`/`--ink`/`--line`/…) удалены — используй uikit-набор напрямую.
- Шрифты: **Onest** (UI / текст / транскрипт), **IBM Plex Mono** (timestamps / IDs / код). Serif выпилен. Hanken Grotesk снят в TD-47 — у него нет базовой кириллицы, и весь русский интерфейс рисовался системным шрифтом.
- Любая новая страница / модал / форма должна работать корректно в **light + dark** (акцент один — графит, picker убран в B18.5). Density фикс `cozy`.
- Логика сохраняется 1-в-1 (хоткеи, consent gates, useEffect, API). Меняется только JSX + className.

**Enforcement:**

- PostToolUse hook [`scripts/hooks/design-gate.mjs`](scripts/hooks/design-gate.mjs) warns при сырых hex/oklch/legacy `--color-*` в новых правках вне whitelisted handoff sources.
- ECC-skills для проектирования UI (`design-system`, `frontend-design-direction`, `accessibility`, `motion-ui`, `frontend-patterns`) скопированы в `.claude/skills/`.
- ECC-agent `a11y-architect` (`.claude/agents/a11y-architect.md`) — обязателен для модалов/форм/навигации/диалогов.
- `/code-review` обязан проверить наличие alignment-блока и token discipline в diff.

При расхождении handoff и паспорта — побеждает паспорт (W6, разделы 12). Но дизайн-токены и компонент-классы — авторитетны.

## Воркфлоу для фича-тасок (PDCA, W3 паспорта)

Для каждой нетривиальной фичи из [`docs/ROADMAP.md`](docs/ROADMAP.md):

1. **Design gate (UI only).** Если фича трогает UI — сначала прогнать design gate (см. секцию выше).
2. **Plan.** Изложить план (либо `/plan`, либо просто в чате) — какие файлы трогаем, какие модули задеваются, есть ли пересечения с принятыми ограничениями раздела 12.
3. **Implement (TDD).** Для модулей с чёткой алгоритмической сутью (матчинг, парсинг, утилиты, db repository, middleware) тесты пишутся **до** реализации. Для UI — визуальная верификация + smoke RTL. Hook `scripts/hooks/tdd-warn.mjs` (PostToolUse) предупреждает если правишь source без соседнего теста.
4. **Verify.** Локально перед коммитом:
   - Rust: `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`
   - TS: `pnpm -r typecheck`, `pnpm --filter <pkg> test`
   - UI: live запуск (`pnpm tauri dev`) + проверка **light и dark** для затронутых экранов ([TD-39] раньше здесь стояло «6 theme×accent» — пикер акцентов убран в B18.5, акцент один (графит `ink`), реальных комбинаций две).
   - Хуки (PostToolUse) делают первые шаги автоматически — но финальная сверка ручная.
5. **Code review.** Запустить `/code-review` (общий) или язык-специфичный (`/rust-review` для Rust, `code-reviewer` агент для TS) **до** коммита фичи в main. Замечания CRITICAL/HIGH — фиксить. UI-PR должен содержать design-gate alignment block.
6. **Mark done.** Снять чек-бокс в `docs/ROADMAP.md` и TaskList харнесса одновременно.

### Тестирование ([B7] enforcement)

| Слой | Тулинг | Где живёт |
|---|---|---|
| Rust core | `cargo test` + cargo-llvm-cov | `#[cfg(test)] mod tests` внутри файлов; helper `crate::db::test_support::fresh_db` для SQLite-репозиториев |
| Frontend (apps/desktop) | vitest + jsdom + React Testing Library | `*.test.ts` / `*.test.tsx` рядом с модулем; setup `src/test/setup.ts` |
| MCP (services/mcp) | vitest (node env) | `*.test.ts` рядом с handler |
| Coverage gate (CI) | `cargo llvm-cov` + vitest `--coverage` v8 | Артефакт `lcov.info` + html. Пороги-ratchet: фронт `lines/statements 69, functions 58, branches 82` (`vitest.config.ts`), Rust `--fail-under-lines 50` (`.github/workflows/ci.yml`) |

ECC-агенты для теста:
- `tdd-guide` — для алгоритмических модулей (PRD-driven test-first)
- `code-reviewer` / `rust-reviewer` / `typescript-reviewer` — обязательны до commit (см. п.4 выше)
- `pr-test-analyzer` — оценка покрытия PR
- `silent-failure-hunter` — поиск swallowed errors

При нехватке покрытия — сначала тесты, потом фича. Понижение порогов в `vitest.config.ts` или `.github/workflows/ci.yml` — только по явному согласованию.

## Security-review триггеры (W5 паспорта — обязательно)

Эти модули обрабатывают чувствительные данные. Перед merge / mark-done **обязательно** прогнать `/security-scan` (AgentShield) или `/security-review`:

| Модуль | Угрозы |
|---|---|
| `services/mcp/**` | Контент звонков = недоверенные данные; защита от инъекций инструкций через транскрипт (M8.3, M8.4); никаких сетевых вызовов |
| Keychain-seam (`secrets.rs`, planned внешние интеграции) | Утечка ключей в БД, логи, телеметрию; небезопасное хранение (значения — только в системном keychain) |
| Audio sidecar permissions (`M1.3`) | Запись без согласия (C1), повышение привилегий в Swift-процессе |
| Cascade delete (`C5`) | Утечка остаточных семплов, неполная очистка `voice_samples.source_call` |
| `local_engine/models.rs` (M12.4) | SHA256-only защита от подмены HF releases; tamper resistance, partial-download race |
| `local_engine/llm.rs` + `local_engine/stt.rs` (M12.3/M12.1) | Sidecar args injection (capability validators), path traversal через `..` (Rust `ensure_path_under` defense-in-depth), temp file leak (transcript в /tmp без 0o600 perms), zombie процессы на timeout (kill vs drop) |
| `capabilities/default.json` (M12) | Sidecar whitelist correctness, args validator regex anchoring |
| `scripts/refresh-model-catalog.sh` (M12) | Bootstrap trust на HF CDN — cross-check workflow + ENV-guard |

## Терминология взаимодействия

- **«Демо» / «показать»** = полноценный запуск целевой среды (`pnpm tauri dev` для desktop). НЕ vite-only browser preview, НЕ dev-mock в Safari. Если environment не поднимается — диагностируем причину и чиним, не падаем на упрощённую версию без явного согласования.
- **«Промежуточный итог»** = живой запуск + summary + git log, не только текст.

## ECC харнесс (W1, W6, W7)

- Используются глобальные правила из `~/.claude/rules/ecc/{common,rust,typescript,web,zh}` (источник: [affaan-m/everything-claude-code](https://github.com/affaan-m/everything-claude-code), копия из приватной инсталляции). При апгрейде ECC сверять что актуальные ограничения паспорта (R2/R3/R4/R6, R9–R13; R1/R5/R7/R8 — superseded облаком) не «улучшены» обратно.
- Активные хуки и project-allowedTools — в `.claude/settings.json`. Все матчатся на `Write|Edit|MultiEdit` и получают payload JSON'ом на stdin:
  - **PreToolUse**: `scripts/hooks/pre-write.mjs` — блокирует (exit 2) запись в Tauri-ключи, `.env*`, `.dev.vars`, `*.key`, `*.pem`, SSH-ключи; и файлы >800 строк, считая **итоговый** размер файла (для Edit/MultiEdit — текущий файл ± дельта, а не размер фрагмента)
  - **PostToolUse**: `scripts/hooks/post-write.mjs` — на `.rs` бежит `cargo fmt` + `cargo check --message-format short`; на `.ts/.tsx` — `typecheck` соответствующего пакета (`packages/contracts` проверяется через потребителя `@wotold/desktop`, своего tsc у него нет). Таймаут 60s через Node — не через `timeout`, которого на macOS нет
  - **PostToolUse**: `scripts/hooks/tdd-warn.mjs` — warns если правишь source без соседнего теста (не блокирует)
  - **PostToolUse**: `scripts/hooks/design-gate.mjs` ([B18.6]) — warns на сырых hex/oklch/legacy `--color-*` вне whitelisted sources. Whitelist считается по пути **относительно корня репо**: абсолютный матч ломался в git-worktree (`<repo>/.claude/worktrees/…` попадал под правило `.claude/` и гейт пропускал всё)
- Личные настройки разработчика — в `.claude/settings.local.json` (в `.gitignore`).
- При конфликте рекомендаций ECC и паспорта побеждает паспорт. `.claude/` не часть сборки продукта (W6, W7).
