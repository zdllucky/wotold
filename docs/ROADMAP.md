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

---

## Audio · Этап 2 / M1

- [x] **#15** M1.2 Swift sidecar — mic + system (см. «Готово»)
- [ ] **#16** M1.3 macOS permissions UX (mic + system tap, обработка отказа) → #15
- [ ] **#17** M1.5 Record screen + indicator + chunked flush (`calls.status` recording → processing) → #15

## STT · Этап 3 / M2 + Этап 8 follow-up

- [ ] **#18** Proxy: подключить Soniox + Gladia в `/v1/stt` (presigned GET + relay + normalize)
- [ ] **#19** Proxy: vitest + miniflare integration tests → #18
- [ ] **#20** M2.2 `SonioxProvider` (managed via proxy + BYO direct из keychain) → #18
- [ ] **#21** M2.2 `GladiaProvider` (managed + BYO) → #18
- [ ] **#22** M2.4-2.5 Pipeline: mic+system merge по таймкодам, owner attribution, `raw_stt.json` → #15, #20
- [ ] **#23** M2.6-2.7 Lang autodetect → `calls.lang_detected` + retries/backoff + UX errors → #22

## Идентификация · Этап 4 / M3

- [ ] **#24** M3.1 Voice embedding sidecar (O3 — выбор библиотеки + интеграция)
- [ ] **#25** M3.2-3.4 Matching: cosine по `voice_samples` + LLM hint + merge в ranked suggestion → #24
- [ ] **#26** M3.5-3.7 UI confirmation (R2 — никакой автопривязки) + dynamic sample update (N=5) + mic→owner auto-bind → #25

## Recap · Этап 5 / M4

- [x] **#27** M4.1 `AnthropicProvider` baseline (см. «Готово»)
- [ ] **#28** M4.2-4.5 Pipeline: structured prompt → `RecapJson` → owner_hint mapping → `recap.md`/`transcript.md` + regen из `raw_stt.json` → #27, #22, #25

## UI · Этап 6 / M7

- [ ] **#29** M7.1 Record screen (start/stop, managed/byo, провайдер) → #17
- [x] **#30** M7.2 Calls list baseline — без FTS (см. «Готово»); FTS-поиск ждёт #22
- [ ] **#31** M7.3 Call detail tabs (recap/transcript/tasks/participants + speaker bindings) → #28, #26
- [x] **#32** M7.4 Contacts baseline — list + create + delete (см. «Готово»)
- [ ] **#46** M7.4 follow-up: edit + multiple identifiers + extensible attributes
- [ ] **#45** M7.4 follow-up: voice samples view + manual delete → #26
- [x] **#33** M7.5 Settings baseline — provider/path/LLM model (см. «Готово»)
- [ ] **#47** M7.5 follow-up: BYO keys в keychain (keyring crate / stronghold)
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
- [ ] **#43** X2 Заменить placeholder `REPLACE_OWNER/wotold` в `tauri.conf.json` → реальный owner/repo (M11.3)
- [ ] **#44** X3 Cloudflare provisioning: `wrangler kv namespace create QUOTA`, `wrangler r2 bucket create wotold-stt-staging`, подставить id в `wrangler.toml`, `wrangler secret put` для ANTHROPIC/SONIOX/GLADIA/R2_* (раздел 16.2)

---

## Что можно стартовать сразу (без зависимостей)

`#16` · `#17` · `#18` · `#24` · `#37` · `#42` · `#43` · `#44` · `#46` · `#47` · `[B1]`

## Backlog (кандидаты на доработку)

> Свободная лента идей. Пользователь докидывает, я причёсываю формулировку и кладу сюда. Когда забираем — оформляется как полноценная задача (M-ссылка, deps, чек-боксы) и переезжает в TaskList. Приоритет — «что ближе всего к текущей итерации» / «что сильнее всего разблокирует следующий шаг».

- ~~**[B1] Permissions UX в Onboarding + Settings.**~~ Закрыто в #16 — [`f5cb476`](#) + [`4ddaff7`](#) fix.
- **[B2] Graceful stop при закрытии окна.** Если юзер закрывает окно с активной записью, сидекар получает SIGHUP — последние ≤5 сек могут не успеть на flushHeader, а calls row остаётся «recording» навсегда. Нужен Tauri on_window_close → invoke stop_recording → finish или fail. Связано с #17.

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
