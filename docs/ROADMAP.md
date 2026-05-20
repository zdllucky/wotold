# Roadmap

> Декомпозиция Этапов раздела 11 [ПАСПОРТА](ПАСПОРТ_ПРОЕКТА.md) на единицы реализации. Файл — источник истины по статусу фич, читается и обновляется людьми. Параллельно в харнессе Claude Code лежит TaskList с теми же ID — синхронизируется вручную в этом файле при изменении статуса.
>
> Легенда: `[x]` готово · `[ ]` пендинг · `→ #N` блокируется задачей N.

---

## Готово

- [x] **Bootstrap** монорепо-скелет — [`322f5d6`](#)
- [x] **Этап 1** Tauri 2 каркас + SQLite + traits + device-id + миграции раздела 6.2 — [`8e40edc`](#)
- [x] **Этап 8** прокси Hono/CF Workers (relay + квота + presigned R2; partner wiring под #18) — [`1bb87b5`](#)
- [x] **Этап 11** авто-обновление + аварийный downgrade-режим + M11.9 doc — [`6a2aa79`](#)
- [x] **Этап 12** CI/CD скелет (ci.yml, release-app.yml, deploy-proxy.yml) + version sync M11.5 — [`a361a37`](#)
- [x] **#32** Contacts directory baseline (list + create + delete + nav) — [`38b310f`](#)
- [x] dialog-plugin для нативного confirm удаления — [`fa0a68a`](#)
- [x] **#34** Onboarding (welcome + owner rename + persistent flag) — [`7091672`](#)
- [x] **#33** Settings page baseline (provider/path/LLM model) — [`a30cd9c`](#)
- [x] **#27** AnthropicProvider (managed + BYO + 6 httpmock-тестов) — [`b942149`](#)
- [x] **#15** Swift audio sidecar — mic (AVAudioEngine) + system (ScreenCaptureKit) → mic.wav + system.wav — [`2c60ec1`](#) + [`5ab308d`](#)
- [x] **#30** Calls list (партишн без FTS) — [`4bbf78f`](#)
- [x] **#16** Permissions UX в Settings (закрывает [B1] тоже) — [`f5cb476`](#) + [`4ddaff7`](#)
- [x] **#17** Chunked WAV flush для crash safety (M1.5) — [`bd9a9a6`](#)
- [x] **#18** Proxy: Soniox + Gladia partner relay в /v1/stt — [`8ef5fac`](#)
- [x] **#46** Edit contact + identifiers + extensible attributes — [`8d61b64`](#)
- [x] **#20** SonioxProvider (managed + BYO direct, 4 тестов) — [`194aa8b`](#)
- [x] **#21** GladiaProvider (managed + BYO direct, 3 теста) — [`d9c1163`](#)
- [x] **#22** Pipeline: mic+system merge + raw_stt.json + transcript.md (M2.4-2.5) — [`4b0970b`](#)
- [x] startup sweep застрявших recording/processing + status tooltips — [`ddee420`](#)
- [x] **#28** Recap pipeline (M4.2-4.4) — LLM auto-chain → recap.md + action_items — [`3e1246c`](#)
- [x] **#19** Proxy vitest + миниframe integration tests (STT routes + partner unit tests, 42 теста)
- [x] **#23** STT robustness: retry/backoff (Network only), auto-fallback Soniox→Gladia, UX-readable `failed_reason`, banner на CallDetail
- [x] **#43** `tauri.conf.json` updater endpoint → `zdllucky/wotold`
- [x] **#47** BYO API keys в Keychain (keyring crate, secrets module, Tauri commands, pipeline wire, Settings UI)
- [x] **#24** Voice embedding foundation (M3.1) — Embedder trait + cosine + BLOB serde, lib decision = ort + ONNX WeSpeaker
- [x] **#31** Call detail tabs (Рекап/Расшифровка/Задачи, без speaker bindings) — [`195ad91`](#)
- [x] **[B6]** Design system + dev-only DS showcase — tokens.css, ui/*, refactor пагов на DS
- [x] **[B7]** Test infra — vitest (desktop+proxy), cargo-llvm-cov, CI tests+coverage, 21 Rust + 31 TS test, TDD hook + ECC enforcement в CLAUDE.md

---

## Audio · Этап 2 / M1

- [x] **#15** M1.2 Swift sidecar — mic + system (см. «Готово»)
- [x] **#16** M1.3 macOS permissions UX (см. «Готово»)
- [x] **#17** M1.5 chunked flush (см. «Готово»). Record screen UX живёт в HomePage.

## STT · Этап 3 / M2 + Этап 8 follow-up

- [x] **#18** Proxy: Soniox + Gladia partner relay (см. «Готово»)
- [x] **#19** Proxy: vitest + miniflare integration tests — STT routes (device-id, quota, R2 head, bad inputs) + partner unit tests (Soniox+Gladia happy/error paths, normalize)
- [x] **#20** M2.2 `SonioxProvider` (см. «Готово»)
- [x] **#21** M2.2 `GladiaProvider` (см. «Готово»)
- [x] **#22** M2.4-2.5 Pipeline (см. «Готово»)
- [x] **#23** M2.6-2.7 Lang autodetect + retries/backoff + auto-fallback Soniox→Gladia + UX-readable `calls.failed_reason` (migration 0002, retry module 11 тестов, banner на CallDetail, tooltip в Calls list)

## Идентификация · Этап 4 / M3

- [x] **#24** M3.1 Voice embedding foundation (O3 — выбран `ort` + ONNX WeSpeaker/ECAPA-TDNN, 256-dim). Модуль `embeddings`: Embedder trait, cosine_similarity, BLOB serde. Реальный OnnxEmbedder + per-segment audio decode + sidecar split → #25
- [ ] **#25** M3.2-3.4 Matching: cosine по `voice_samples` + LLM hint + merge в ranked suggestion → #24
- [ ] **#26** M3.5-3.7 UI confirmation (R2 — никакой автопривязки) + dynamic sample update (N=5) + mic→owner auto-bind → #25

## Recap · Этап 5 / M4

- [x] **#27** M4.1 `AnthropicProvider` baseline (см. «Готово»)
- [x] **#28** M4.2-4.4 Recap pipeline (см. «Готово»). M4.5 regenerate_recap Tauri-команда — отдельная мини-задача

## UI · Этап 6 / M7

- [ ] **#29** M7.1 Record screen (start/stop, managed/byo, провайдер) → #17
- [x] **#30** M7.2 Calls list baseline — без FTS (см. «Готово»); FTS-поиск ждёт #22
- [x] **#31** M7.3 Call detail tabs — Recap/Transcript/Tasks (см. «Готово»). Speaker bindings — в #26.
- [x] **#32** M7.4 Contacts baseline — list + create + delete (см. «Готово»)
- [ ] **#46** M7.4 follow-up: edit + multiple identifiers + extensible attributes
- [ ] **#45** M7.4 follow-up: voice samples view + manual delete → #26
- [x] **#33** M7.5 Settings baseline — provider/path/LLM model (см. «Готово»)
- [x] **#47** M7.5 follow-up: BYO keys в keychain — `keyring` crate, `secrets::ByoProvider` enum, Tauri commands (set/delete/list_byo_status — без раскрытия значений), pipeline `mode_for` читает ключ per-provider, Settings BYO UI с password input + status badge
- [ ] **#48** M7.5 follow-up: Quota indicator из /v1/usage → #44
- [x] **#34** M7.6 Onboarding baseline — welcome + owner rename (см. «Готово»)

## MCP · Этап 7 / M8

- [ ] **#35** M8.1-8.4 Local MCP server + 7 read-only tools (search_calls, get_call, get_recap, get_transcript, list_participants, find_calls_by_contact, calls_in_range) → #28
- [ ] **#36** Connector setup docs для подключения в Claude → #35

## Auth · Этап 9 / M10 (SCAFFOLD — ничего не разблокирует в MVP)

- [ ] **#37** M10.1 OIDC backend в прокси (Apple/Google/Microsoft)
- [ ] **#38** M10.2 + M10.4 Frontend SSO flow + device-id linking + sign-out → #37

## Constraints · Этап 10 / раздел 9

- [ ] **#39** C1 Recording consent dialog → #15
- [ ] **#40** C2 Biometric opt-in per contact (флаг «накапливать голосовой профиль») → #32
- [ ] **#41** C5 Cascade delete (audio + samples от удалённого звонка) → #30

> C3 (локальность семплов) и C4 (прокси не логирует контент) — отрицательные инварианты, реализуются как тесты/аудит поверх существующих модулей, не отдельные таски.

## Setup · one-time manual

- [ ] **#42** X1 Generate Tauri minisign + публичный ключ в `tauri.conf.json` + приватный в GitHub-секрет + офлайн-бэкап (M11.1, M11.9)
- [x] **#43** X2 `REPLACE_OWNER/wotold` → `zdllucky/wotold` в `tauri.conf.json` (updater endpoint)
- [ ] **#44** X3 Cloudflare provisioning: `wrangler kv namespace create QUOTA`, `wrangler r2 bucket create wotold-stt-staging`, подставить id в `wrangler.toml`, `wrangler secret put` для ANTHROPIC/SONIOX/GLADIA/R2_* (раздел 16.2)

---

## Что можно стартовать сразу (без зависимостей)

`#25` · `#37` · `#42` · `#44`

## Backlog (кандидаты на доработку)

> Свободная лента идей. Пользователь докидывает, я причёсываю формулировку и кладу сюда. Когда забираем — оформляется как полноценная задача (M-ссылка, deps, чек-боксы) и переезжает в TaskList. Приоритет — «что ближе всего к текущей итерации» / «что сильнее всего разблокирует следующий шаг».

- ~~**[B1] Permissions UX в Onboarding + Settings.**~~ Закрыто в #16 — [`f5cb476`](#) + [`4ddaff7`](#) fix.
- **[B2] Graceful stop при закрытии окна.** Если юзер закрывает окно с активной записью, сидекар получает SIGHUP — последние ≤5 сек могут не успеть на flushHeader, а calls row остаётся «recording» навсегда. Нужен Tauri on_window_close → invoke stop_recording → finish или fail. Связано с #17.
- **[B3] STT job-resume при retry.** Когда воркер таймаутит на 25-секундном polling-бюджете (длинная запись), клиент теряет partner job_id и при повторе создаёт новый job → двойная оплата у Soniox/Gladia. Решение: кэшировать `r2Key → partner_provider:job_id` в Workers KV с TTL ≈30 мин; на retry — резюмировать polling по существующему id. Связано с #18.
- **[B4] Proxy URL input в Settings.** Pipeline managed-режима требует непустой `proxy_base_url`, но в UI его ввести нельзя — только через прямую правку settings table. Добавить field в SettingsPage после X3 (когда задеплоенный URL прокси будет известен).
- **[B5] Realtime событие «транскрипция готова».** Сейчас клиент узнаёт о статусе ready/failed только через ручной refresh Calls list. Поднять Tauri event `pipeline:finished {call_id, status}` из `pipeline::run` финала, во фронте слушать через `listen()` и обновлять список без перезагрузки.
- ~~**[B6] Design system + dev-only Components showcase.**~~ Закрыто — `apps/desktop/src/styles/tokens.css` + `ui/*` (Button/Badge/Pill/StatusDot/Field/Tabs/Card/Empty/Toolbar) + рефакторинг всех экранов + `pages/DesignSystemPage.tsx` (гейт `import.meta.env.DEV`, таб «🛠 DS» в навбаре только в dev).

  **Правило проекта**: новые экраны/фичи **обязаны** использовать DS-компоненты + токены. Если чего-то не хватает — сначала PR в DS (новый компонент или токен), потом фича. Inline `oklch(...)` и magic gaps отлавливаем при ревью.

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
