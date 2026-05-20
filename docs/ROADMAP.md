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
- [x] **#37** OIDC backend в прокси (M10.1 SCAFFOLD) — Google real + Apple/MS stubs, KV AUTH namespace, state CSRF, session с TTL
- [x] **[B8]** Backend deployment pipeline — wrangler envs (staging + production), GH Actions split (preflight → staging on main / production on tag), `scripts/cf-bootstrap.sh`, `docs/DEPLOYMENT.md`, `.dev.vars.example` обновлён под OIDC. Бесплатность сохранена (R7). Manual setup → #44.
- [x] **#38** Frontend SSO + session в Keychain — Auth API client, AccountSection UI, manual paste flow (deep-link `wotold://` follow-up)
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
- [x] **#25** M3.2-3.4 Matching foundation — `audio_io::extract_segment` (hound WAV slicing), `matching::{list_consenting_samples, rank_candidates}` (cosine + C2 фильтр), `llm_hint::request_speaker_hints` (Anthropic prompt + JSON parse), `merge_signals::merge` (embedding+llm с embedding bias), `identify::identify_speakers` orchestrator → `db::insert_speaker_suggestions` (call_speakers с confirmed=0). Production pipeline wire через #26 + real OnnxEmbedder.
- [~] **#26** partial M3.5 UI confirmation flow — `db::{list_call_speakers, confirm_call_speaker, unbind_call_speaker}` + view с join'ом display_name по contact_id + 4 unit-теста. Tauri commands `list_call_speakers/confirm_call_speaker/unbind_call_speaker`. UI новая таб «Спикеры» в `CallDetailPage` через `SpeakersSection` (suggestion hint с confidence + источник, контакт-селектор, кнопки Подтвердить/Отвязать; R2 enforced — финальная привязка только через явный confirm). Dev mock с in-memory speakerBindings. **Остаётся (deferred)**: real OnnxEmbedder + wire identify_speakers в pipeline (нет ONNX модели), dynamic sample update (N=5), mic→owner auto-bind в pipeline.

## Recap · Этап 5 / M4

- [x] **#27** M4.1 `AnthropicProvider` baseline (см. «Готово»)
- [x] **#28** M4.2-4.4 Recap pipeline (см. «Готово»)
- [x] **M4.5** regenerate_recap — `pipeline::regenerate_recap` читает transcript.md с диска, читает call meta (lang_detected) и settings (provider_path/llm_model/proxy_base_url), вызывает recap::run заново. Tauri command `regenerate_recap(callId)`. Ошибки LLM пробрасываются в UI (toast) в отличие от pipeline::run где silent-skip. CallDetailPage кнопка «↻ Пересоздать рекап» в табе Рекап (disabled если transcript отсутствует, busy spinner на время).

## UI · Этап 6 / M7

- [x] **#29** M7.1 Record screen — HomePage реализует start/stop с DS-кнопкой, индикатором активной записи (pulse) и tooltip последнего сохранённого звонка. Provider/managed/byo выбираются в Settings и используются pipeline'ом — раздельный UI не требуется (избыточно для M7.1).
- [x] **#30** M7.2 Calls list baseline — без FTS (см. «Готово»); FTS-поиск ждёт #22
- [x] **#31** M7.3 Call detail tabs — Recap/Transcript/Tasks (см. «Готово»). Speaker bindings — в #26.
- [x] **#32** M7.4 Contacts baseline — list + create + delete (см. «Готово»)
- [x] **#46** M7.4 follow-up: edit + multiple identifiers + extensible attributes — `ContactsPage` имеет click-to-edit на имени контакта → ContactForm с initial state, addIdentifier/removeIdentifier с выбором kind из IDENTIFIER_KINDS, addAttribute/removeAttribute для свободных ключ/значение, C2 consent_voice toggle отдельно от attributes. Backend `update_contact` replace-all identifiers внутри транзакции. Owner редактируется (display_name), но `is_owner` не меняется.
- [x] **#45** M7.4 follow-up: voice samples view + manual delete (C3 паспорта) — `db::voice_samples::{list_voice_samples, delete_voice_sample, VoiceSampleView}` (4 tokio-теста, embedding-блоб не возвращается клиенту, только length). Tauri commands `list_voice_samples/delete_voice_sample`. UI `VoiceSamplesSection` показывается внутри ContactForm в режиме редактирования: список с created_at + quality + source_call ссылкой, кнопка ручного delete с warning dialog. Появляется автоматически если у контакта есть семплы, либо при `consent_voice=true` (alwaysShow). Dev mock с in-memory массивом для preview.
- [x] **#33** M7.5 Settings baseline — provider/path/LLM model (см. «Готово»)
- [x] **#47** M7.5 follow-up: BYO keys в keychain — `keyring` crate, `secrets::ByoProvider` enum, Tauri commands (set/delete/list_byo_status — без раскрытия значений), pipeline `mode_for` читает ключ per-provider, Settings BYO UI с password input + status badge
- [x] **#48** M7.5 follow-up: Quota indicator UI из /v1/usage — `apps/desktop/src/api/usage.ts` клиент + `ui/UsageBar` DS-компонент (tone ok/warning/danger по % использования) + `pages/UsageSection` показан только в managed-режиме. Прокси расширен `sttSecondsLimit`/`llmTokensLimit` в `UsageResponse` (берётся из `QUOTA_STT_SECONDS_PER_DAY`/`QUOTA_LLM_TOKENS_PER_DAY` env vars). 3 integration теста для /v1/usage. Сброс счётчиков отображается в локальной таймзоне юзера.
- [x] **#34** M7.6 Onboarding baseline — welcome + owner rename (см. «Готово»)

## MCP · Этап 7 / M8

- [x] **#35** M8.1-8.4 Local MCP server — `services/mcp/` Node TS + `@modelcontextprotocol/sdk` stdio + 7 read-only tools. `better-sqlite3` readonly. Zod input validation. 16 vitest tests.
- [x] **#36** `docs/MCP.md` — установка в Claude Desktop / Cursor / mcp-inspector + env override через `WOTOLD_APP_DATA_DIR` + M8.3 injection warning.

## Auth · Этап 9 / M10 (SCAFFOLD — ничего не разблокирует в MVP)

- [x] **#37** M10.1 OIDC backend в прокси — start/callback/me/signout, KV AUTH (state TTL 5min, session TTL 30d, accounts permanent), GoogleAdapter (реальный) + Apple/Microsoft stubs (X4 manual setup deferred), 44 теста (storage+session+providers+routes integration)
- [x] **#38** M10.2 + M10.4 Frontend SSO flow — auth API client, session token в Keychain (расширение secrets module), AccountSection UI с Sign in/Sign out, manual paste flow для callback. **Auto-перехват через deep-link `wotold://` — отдельный follow-up.**

## Constraints · Этап 10 / раздел 9

- [x] **#39** C1 Recording consent dialog — HomePage показывает Card с предупреждением (статьи РФ/РК о тайне коммуникаций) перед первой записью. consent timestamp в `settings.recording_consent_at` — повторно не показываем.
- [x] **#40** C2 Biometric opt-in per contact — checkbox «Накапливать голосовой профиль» в ContactForm, хранится как `attributes.consent_voice='true'` (без миграции). Matching pipeline (#25/#26) обязан проверять этот флаг перед записью в voice_samples.
- [x] **#41** C5 Cascade delete — `db::delete_call_and_samples` (voice_samples + CASCADE FK на action_items/call_speakers), Tauri `delete_call` команда удаляет также audio dir `calls/<id>/`. UI: красная кнопка «Удалить» в CallDetailPage с native confirm dialog.

> C3 (локальность семплов) и C4 (прокси не логирует контент) — отрицательные инварианты, реализуются как тесты/аудит поверх существующих модулей, не отдельные таски.

## Setup · one-time manual

- [ ] **#42** X1 Generate Tauri minisign + публичный ключ в `tauri.conf.json` + приватный в GitHub-секрет + офлайн-бэкап (M11.1, M11.9)
- [x] **#43** X2 `REPLACE_OWNER/wotold` → `zdllucky/wotold` в `tauri.conf.json` (updater endpoint)
- [ ] **#44** X3 Cloudflare provisioning per env. Делается через `scripts/cf-bootstrap.sh staging|production` + `wrangler secret put --env`. Полная процедура — `docs/DEPLOYMENT.md`. Требует:
  - CF Free аккаунт + API token (Workers/KV/R2 edit) + Account ID
  - GitHub Repo Secrets: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`
  - GitHub Environments `staging` (auto) и `production` (manual approval)
  - Подстановка реальных KV IDs в `services/proxy/wrangler.toml` (TODO_* плейсхолдеры)

---

## Что можно стартовать сразу (без зависимостей)

`#26` · `#42` · `#44`

## Backlog (кандидаты на доработку)

> Свободная лента идей. Пользователь докидывает, я причёсываю формулировку и кладу сюда. Когда забираем — оформляется как полноценная задача (M-ссылка, deps, чек-боксы) и переезжает в TaskList. Приоритет — «что ближе всего к текущей итерации» / «что сильнее всего разблокирует следующий шаг».

- ~~**[B1] Permissions UX в Onboarding + Settings.**~~ Закрыто в #16 — [`f5cb476`](#) + [`4ddaff7`](#) fix.
- ~~**[B2] Graceful stop при закрытии окна.**~~ Закрыто — `lib.rs` setup hook слушает `WindowEvent::CloseRequested`, при активной recording prevent_close + async stop sidecar + `db::fail_recording_with_reason` при сбое + `app.exit(0)`.
- ~~**[B3] STT job-resume при retry.**~~ Закрыто — `transcribeSoniox`/`transcribeGladia` принимают `existingJobId` (+`existingResultUrl` для Gladia) и возвращают `{transcript, jobId, jobCreated}`. STT route кэширует `stt_job:{provider}:{r2Key}` в QUOTA KV TTL 30 мин; на retry resume вместо create — двойной оплаты у партнёра нет.
- ~~**[B4] Proxy URL input в Settings.**~~ Закрыто — `SettingsPage` → секция «Прокси (managed)» с URL input + http/https validation.
- ~~**[B9] Deep-link `wotold://` для OIDC callback.**~~ Закрыто — `tauri-plugin-deep-link` v2.4 + scheme `wotold` в `tauri.conf.json`. Proxy callback читает `state.redirectMode='deeplink'` → 302 на `wotold://auth/callback?session=...`. Tauri setup hook emit'ит `auth:deep-link` event, AccountSection слушает и авто-сохраняет session. Manual paste flow остаётся fallback (redirectMode='json').
- ~~**[B5] Realtime событие «транскрипция готова».**~~ Закрыто — `pipeline::run` принимает `Option<&AppHandle>` и emit'ит `pipeline:finished {call_id, status, failed_reason?}` в финале. CallsPage слушает через `@tauri-apps/api/event` → auto-refresh без manual reload.
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
