# Wotold — навигация для агента

Десктоп-утилита записи звонков с транскрипцией и диаризацией. macOS-first, локальное хранение, MCP для Claude.

## Источник истины

[`docs/ПАСПОРТ_ПРОЕКТА.md`](docs/ПАСПОРТ_ПРОЕКТА.md) — ТЗ. При расхождении паспорт побеждает (S4, W6).

**ВАЖНО — прочесть раздел 12 паспорта перед изменениями.** Сознательно принятые ограничения R1–R8 не «чинить»:

| Маркер | Что принято |
|---|---|
| R1 | Free-тир абьюзится переустановкой (сброс device-id) — митигации на потом |
| R2 | LLM-догадка спикеров — только booster, никакого автоприсвоения |
| R3 | Авто-детект «идёт звонок» не делаем, только ручная кнопка |
| R4 | Windows-захват = `unimplemented!()` за trait `AudioCapture` |
| R5 | Биллинг/тиры = заглушки |
| R6 | macOS-сборка без Apple-нотаризации в MVP, Gatekeeper «Open anyway» норма |
| R7 | Free Cloudflare: перестаёт отвечать при превышении лимитов, без списаний |
| R8 | Аудио НЕ через память прокси-воркера — только R2 + presigned-URL |

## Структура монорепо

```
apps/desktop/         Tauri 2 (фронт TS + Rust ядро + Swift sidecar Core Audio)
  src/                Фронтенд (TypeScript)
  src-tauri/          Rust ядро, capabilities, tauri.conf.json
  sidecars/macos-audio/  Swift sidecar (Core Audio process tap)

services/proxy/       Hono на Cloudflare Workers (relay + квота по device-id)
services/mcp/         Локальный MCP-сервер (read-only)

packages/contracts/   ОБЩИЕ типы — DiarizedTranscript, Recap JSON, API прокси, latest.json

.github/workflows/    CI/CD (path-filtered, S1 изоляция секретов между джобами)
docs/                 Паспорт и сопутствующие документы
.claude/              ECC dev-харнесс (W7 — не часть продукта, исключён из артефактов)
```

## Этапы реализации

Раздел 11 паспорта. Порядок строгий: 1 → 2 → 3 → 4 → 5, далее 6/7/8 параллельно после 5, авто-обновление (11) и CI (12) ведутся параллельно с 6–10, но обязательны до релиза.

**Декомпозиция фич и текущий статус** → [`docs/ROADMAP.md`](docs/ROADMAP.md). Это источник истины по тому что сделано/в работе/осталось. Параллельно ведётся TaskList в харнессе Claude Code — обновляется руками одновременно с ROADMAP при смене статуса.

## Контракты (S2)

Любое изменение формата `DiarizedTranscript`, рекап-JSON, API прокси или `latest.json` правится в `packages/contracts` и потребляется и приложением, и прокси. Дублирование типов запрещено.

## Секреты (S1)

- **Tauri minisign приватный ключ** — только в джобе сборки приложения (`TAURI_SIGNING_PRIVATE_KEY` + пароль)
- **Партнёрские ключи** (Soniox/Gladia/Anthropic) — только в джобе деплоя прокси
- Никаких repo-wide секретов для этих значений
- BYO-ключи пользователей живут в системном keychain, не в БД, не в логах

## Принципы

- **Локальное-первое**: запись, просмотр, поиск, MCP, контакты — без сети
- **Идентификация только подсказка**: никакой автопривязки контакта без подтверждения пользователя (M3, R2)
- **Прокси не видит контент**: только метрики по device-id, аудио через R2, не через память воркера (M9.6, R8)
- **MCP read-only**: контент звонков — недоверенные данные, защита от инъекций инструкций (M8.3, M8.4)

## Design Gate (Atelier v2, [B17] — ОБЯЗАТЕЛЬНО до любой UI работы)

Перед **любой** правкой `.tsx`/`.css`/`*.module.css`, или инлайн-стилей, **до** Plan/Implement:

1. Прочесть [`docs/design/atelier-v2/README.md`](docs/design/atelier-v2/README.md) и соответствующую секцию [`docs/design/atelier-v2/MIGRATION.md`](docs/design/atelier-v2/MIGRATION.md).
2. Запустить `/design-gate <surface>` или прочесть [`.claude/skills/design-gate/SKILL.md`](.claude/skills/design-gate/SKILL.md).
3. В чате выдать alignment-блок:
   ```text
   [design-gate] Surface: <page/component>
   Reference: docs/design/atelier-v2/<file>:<section>
   Tokens used: <list>
   Classes used: <list>
   New tokens needed: <none | list>
   Logic preserved: <yes — list>
   A11y: <focus, target, ARIA>
   ```
4. Только после этого — Plan / Implement.

**Правила (см. design-gate skill для полного списка):**

- Все цвета/spacing/radius/shadow → `var(--*)` из [`apps/desktop/src/styles/tokens.css`](apps/desktop/src/styles/tokens.css). Запрещены сырые hex/oklch в `.tsx`/любом `.css` кроме handoff sources.
- Компонентные классы из [`apps/desktop/src/styles/wotold.css`](apps/desktop/src/styles/wotold.css): `.btn`, `.card`, `.tabs`, `.transcript-row`, `.field`, `.input`, `.sp`, `.rec-btn`, `.stat`, `.nav-item`, `.app-rail`, `.app-shell`, `.app-main`, `.tab`, `.modal-backdrop`, `.index-card`, `.dot`, `.conf`, `.empty`, `.divider`, `.wave-lane`.
- `var(--signal)` (красный) — **только** запись и destructive actions. Все остальные акценты — `var(--accent)` (bordeaux / persian / ink, ортогонально к light/dark).
- Шрифты: Source Serif 4 (display/title/subtitle/transcript), DM Sans (UI/labels), JetBrains Mono (timestamps/IDs).
- Любая новая страница / модал / форма должна работать корректно во всех 6 комбинациях theme×accent.
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
   - UI: live запуск (`pnpm tauri dev`) + проверка всех 6 theme×accent комбинаций для затронутых экранов.
   - Хуки (PostToolUse) делают первые шаги автоматически — но финальная сверка ручная.
5. **Code review.** Запустить `/code-review` (общий) или язык-специфичный (`/rust-review` для Rust, `code-reviewer` агент для TS) **до** коммита фичи в main. Замечания CRITICAL/HIGH — фиксить. UI-PR должен содержать design-gate alignment block.
6. **Mark done.** Снять чек-бокс в `docs/ROADMAP.md` и TaskList харнесса одновременно.

### Тестирование ([B7] enforcement)

| Слой | Тулинг | Где живёт |
|---|---|---|
| Rust core | `cargo test` + cargo-llvm-cov | `#[cfg(test)] mod tests` внутри файлов; helper `crate::db::test_support::fresh_db` для SQLite-репозиториев |
| Frontend (apps/desktop) | vitest + jsdom + React Testing Library | `*.test.ts` / `*.test.tsx` рядом с модулем; setup `src/test/setup.ts` |
| Proxy (services/proxy) | vitest (node env) | `*.test.ts` рядом с handler/middleware |
| Coverage gate (CI) | `cargo llvm-cov` + vitest `--coverage` v8 | Артефакт `lcov.info` + html, baseline 10-30% lines, цель 80% |

ECC-агенты для теста:
- `tdd-guide` — для алгоритмических модулей (PRD-driven test-first)
- `code-reviewer` / `rust-reviewer` / `typescript-reviewer` — обязательны до commit (см. п.4 выше)
- `pr-test-analyzer` — оценка покрытия PR
- `silent-failure-hunter` — поиск swallowed errors

При нехватке покрытия — сначала тесты, потом фича. Понижение coverage threshold в `vitest.config.ts` или `Cargo.toml` — только по явному согласованию.

## Security-review триггеры (W5 паспорта — обязательно)

Эти модули обрабатывают чувствительные данные. Перед merge / mark-done **обязательно** прогнать `/security-scan` (AgentShield) или `/security-review`:

| Модуль | Угрозы |
|---|---|
| `services/proxy/**` | Инъекция ключей владельца, обход квоты, утечка контента в логи (M9.6), CORS/CSRF, R2 presign abuse |
| BYO-ключи (keychain, `M7.5`) | Утечка в БД, логи, телеметрию; небезопасное хранение |
| `services/mcp/**` | Контент звонков = недоверенные данные; защита от инъекций инструкций через транскрипт (M8.3, M8.4); никаких сетевых вызовов |
| Auth flow (`M10`) | OIDC callback, CSRF на /v1/auth/callback, токен-handling |
| Audio sidecar permissions (`M1.3`) | Запись без согласия (C1), повышение привилегий в Swift-процессе |
| Cascade delete (`C5`) | Утечка остаточных семплов, неполная очистка `voice_samples.source_call` |

## Терминология взаимодействия

- **«Демо» / «показать»** = полноценный запуск целевой среды (`pnpm tauri dev` для desktop, `wrangler dev` для proxy). НЕ vite-only browser preview, НЕ dev-mock в Safari. Если environment не поднимается — диагностируем причину и чиним, не падаем на упрощённую версию без явного согласования.
- **«Промежуточный итог»** = живой запуск + summary + git log, не только текст.

## ECC харнесс (W1, W6, W7)

- Используются глобальные правила из `~/.claude/rules/ecc/{common,rust,typescript,web,zh}` (источник: [affaan-m/everything-claude-code](https://github.com/affaan-m/everything-claude-code), копия из приватной инсталляции). При апгрейде ECC сверять что R1–R8 паспорта не «улучшены» обратно.
- Активные хуки и project-allowedTools — в `.claude/settings.json`:
  - **PreToolUse Write/Edit**: `scripts/hooks/pre-write.mjs` — блокирует запись в Tauri-ключи, `.env*`, `.dev.vars`, `*.key`, `*.pem`, SSH-ключи и файлы >800 строк
  - **PostToolUse Write/Edit**: `scripts/hooks/post-write.sh` — на `.rs` правках бежит `cargo fmt` + `cargo check --message-format short` (timeout 60s); на `.ts/.tsx` — `tsc --noEmit` соответствующего workspace-пакета
  - **PostToolUse Write/Edit**: `scripts/hooks/design-gate.mjs` ([B17]) — warns на сырых hex/oklch/legacy `--color-*` вне whitelisted handoff sources
- Личные настройки разработчика — в `.claude/settings.local.json` (в `.gitignore`).
- При конфликте рекомендаций ECC и паспорта побеждает паспорт. `.claude/` не часть сборки продукта (W6, W7).
