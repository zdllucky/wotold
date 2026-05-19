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

## Воркфлоу для фича-тасок (PDCA, W3 паспорта)

Для каждой нетривиальной фичи из [`docs/ROADMAP.md`](docs/ROADMAP.md):

1. **Plan.** Изложить план (либо `/plan`, либо просто в чате) — какие файлы трогаем, какие модули задеваются, есть ли пересечения с принятыми ограничениями раздела 12.
2. **Implement.** TDD где разумно: тесты сначала для модулей с чёткой алгоритмической сутью (матчинг, парсинг транскрипта, утилиты). Для UI — визуальная верификация.
3. **Verify.** Локально перед коммитом:
   - Rust: `cargo fmt --check`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`
   - TS: `pnpm -r typecheck`, тесты соответствующего пакета
   - Хуки (PostToolUse) делают первые шаги автоматически — но финальная сверка ручная.
4. **Code review.** Запустить `/code-review` (общий) или язык-специфичный (`/rust-review` для Rust, `code-reviewer` агент для TS) **до** коммита фичи в main. Замечания CRITICAL/HIGH — фиксить.
5. **Mark done.** Снять чек-бокс в `docs/ROADMAP.md` и TaskList харнесса одновременно.

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

## ECC харнесс (W1, W6, W7)

- Используются глобальные правила из `~/.claude/rules/ecc/{common,rust,typescript,web,zh}` (источник: [affaan-m/everything-claude-code](https://github.com/affaan-m/everything-claude-code), копия из приватной инсталляции). При апгрейде ECC сверять что R1–R8 паспорта не «улучшены» обратно.
- Активные хуки и project-allowedTools — в `.claude/settings.json`:
  - **PreToolUse Write/Edit**: `scripts/hooks/pre-write.mjs` — блокирует запись в Tauri-ключи, `.env*`, `.dev.vars`, `*.key`, `*.pem`, SSH-ключи и файлы >800 строк
  - **PostToolUse Write/Edit**: `scripts/hooks/post-write.sh` — на `.rs` правках бежит `cargo fmt` + `cargo check --message-format short` (timeout 60s); на `.ts/.tsx` — `tsc --noEmit` соответствующего workspace-пакета
- Личные настройки разработчика — в `.claude/settings.local.json` (в `.gitignore`).
- При конфликте рекомендаций ECC и паспорта побеждает паспорт. `.claude/` не часть сборки продукта (W6, W7).
