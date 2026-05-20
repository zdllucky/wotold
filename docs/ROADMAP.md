# Roadmap

> Декомпозиция Этапов раздела 11 [ПАСПОРТА](ПАСПОРТ_ПРОЕКТА.md) на единицы реализации. Файл — источник истины по статусу фич, читается и обновляется людьми. Параллельно в харнессе Claude Code лежит TaskList с теми же ID — синхронизируется вручную в этом файле при изменении статуса.
>
> Легенда: `[x]` готово · `[ ]` пендинг · `→ #N` блокируется задачей N.

---

## Статус MVP

**Реализовано:**
- Этапы 1-5, 6-10, 11-12 по [паспорту](ПАСПОРТ_ПРОЕКТА.md): audio capture (mic+system), STT relay (Soniox+Gladia с auto-fallback), pipeline (transcript+raw_stt), recap+action_items (Groq Llama 3.3 70B), CallDetail с интерактивным транскриптом+spoken bubbles+Speakers+regenerate, contacts с edit+samples view, settings (managed+BYO+quota+account), MCP server, OIDC scaffold, auto-update, CI/CD (split staging/prod + smoke+rollback + commitlint + claude-review + changelog).
- Staging backend полностью boevoy: /health, /v1/usage, /v1/stt/staging-url (R2), /v1/llm (Groq), /v1/auth/google/start.
- Все B1-B12 + B15 backlog requirements закрыты.

**Осталось для production-релиза (manual user actions):**
- **#42 X1 Tauri minisign** — генерация ключа подписи updater'а (one-time CLI).
- **#44 X3 CF production** — те же 7 GH Secrets с суффиксом `_PRODUCTION` + Google OAuth Authorized URI + tag `v0.1.0`.

**Deferred до получения ONNX model bytes:**
- Real OnnxEmbedder + wire identify_speakers в pipeline (часть #26). Speakers UI работает через manual confirm, mic→owner auto-bind работает; biometric matching отключён.

**Активный backlog (пост-MVP улучшения):**
- B13 preferred_language setting, B14 live recording level meter.

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
- [~] **#26** partial M3.5+3.7 — UI confirmation flow + mic→owner auto-bind. `db::{list_call_speakers, confirm_call_speaker, unbind_call_speaker, auto_bind_owner_speaker}` + view с join'ом display_name по contact_id + 6 unit-тестов. Tauri commands `list_call_speakers/confirm_call_speaker/unbind_call_speaker`. UI новая таб «Спикеры» в `CallDetailPage` через `SpeakersSection` (suggestion hint с confidence + источник, контакт-селектор, кнопки Подтвердить/Отвязать; R2 enforced — финальная привязка только через явный confirm). **M3.7 mic→owner auto-bind**: `pipeline::run` после persist_artifacts автоматически вставляет confirmed=1 row для `speaker_tag="owner"` → owner contact. Не нарушает R2 потому что owner=сам пользователь. Dev mock с in-memory speakerBindings. **Остаётся (deferred)**: real OnnxEmbedder + wire identify_speakers в pipeline (нет ONNX модели), dynamic sample update (N=5).

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
- [~] **#44** X3 Cloudflare provisioning per env. Staging закрыт полностью (R2 enabled by user, KV created via provision-infra workflow, secrets залиты через sync-proxy-secrets workflow, deploy зелёный, smoke /health 200, /v1/llm и /v1/stt/staging-url работают вживую). **Остаётся для production**: GH Secrets с суффиксом `_PRODUCTION` (можно те же ключи что staging), Google OAuth Authorized redirect URI для production callback, и tag `v0.1.0` для триггера production deploy. Полная процедура — `docs/DEPLOYMENT.md`. Требует:
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

### Активные задачи (формализованные требования)

- [x] **[B10] Интерактивный транскрипт на CallDetail (M7.3 follow-up).** Сейчас транскрипт рендерится как сырой markdown через ReactMarkdown — стена текста, спикеры теряются. Требование:
  - Парсить `raw_stt.json` (`merged` массив `TranscriptSegment[]`) вместо `transcript.md`.
  - Рендер в виде чат-бабблов: бейдж спикера + текст + тайм-метка `mm:ss`.
  - Группировка подряд идущих сегментов одного спикера (как сейчас в `render_transcript_md` для md, но в DOM).
  - Цвет бейджа стабилен на `speakerTag` (hash → palette из tokens.css).
  - Owner-бабблы выровнены вправо (правая колонка), остальные — слева.
  - Click на бейдж спикера → открывает Speakers tab + скроллит к этому спикеру (deep-link внутри страницы).
  - `read_call_artifact(kind='raw_stt')` Tauri-команда возвращает JSON segments. Если файл отсутствует (старые звонки) — fallback на текущий markdown.
  - Acceptance: на ready-звонке таб «Расшифровка» показывает баблы; на 5+ спикерах цвета не пересекаются; mobile-узкая ширина окна не ломает layout.

- [x] **[B11] Авто-добавление всех спикеров в Speakers секцию + кнопка «Добавить как контакт» (M7.4 follow-up #46, M3.5 follow-up #26).** Сейчас `SpeakersSection` показывает только rows из `call_speakers` table — а они туда попадают только если `identify_speakers` отработал (#25 pipeline-wire deferred ⇒ обычно пусто). Требование:
  - В `pipeline::run` после `persist_artifacts`: для каждого distinct `speaker_tag` из merged-транскрипта (кроме `owner` — у него auto-bind, см. M3.7) **создать call_speakers row** с `contact_id=NULL`, `confirmed=0`, `suggestion_*` NULL. Это делает спикера видимым в UI сразу, без identify_speakers.
  - В `SpeakersSection`: рядом с селектором контакта добавить кнопку **«+ Добавить как контакт»**. При клике — inline форма (display_name + опц. `consent_voice` checkbox) → `create_contact` + `confirm_call_speaker(speaker, new_contact_id)` атомарной парой.
  - Список ВСЕХ спикеров отображается даже если они анонимные («S1», «S2» без привязки) с подсказкой «Не привязан».
  - UX-копия: «Кто это? Выбери контакт или добавь нового».
  - Acceptance: после успешного звонка с 3 спикерами в табе «Спикеры» сразу 3 row (включая owner confirmed); кнопка «+ Добавить» создаёт контакт и тут же привязывает.

- [x] **[B12] LLM resilience: retry on 5xx + UX message.** Groq может вернуть 502/503 при rate-limit (30 RPM free) или временной перегрузке. Сейчас одна ошибка → `recap silent-skip` в pipeline, регенерация — explicit Err во фронте. Требование:
  - В `services/proxy/src/lib/llm-backends.ts`: на upstream `≥500` сделать одну паузу 1.5s и retry — это покрывает transient Groq glitches.
  - В UI ошибки рекапа показывать с кнопкой «Повторить» (current «↻ Пересоздать рекап» уже почти оно — добавить hint «бесплатный Groq иногда лимитит, подожди 5 сек и попробуй ещё»).
  - Acceptance: 502 на первом запросе с переход на retry за 1.5s даёт 200; счётчик usage тикает только за фактически использованные токены.

- [ ] **[B13] Предпочитаемый язык — системная настройка.** Сейчас Soniox/Gladia auto-detect (с биасом ru/en/kk — [Lang-tuning]), а LLM пишет рекап/действия на языке детектированного транскрипта (`Output language: {lang_detected}` в `recap.rs::build_system_prompt`). Если детект ошибся или транскрипт мульти-язычный — рекап получается в случайном языке. Требование:
  - Новый setting `preferred_language: BCP47 | 'auto'` (default `'auto'`). UI — `SelectField` в Settings → LLM section с пресетами: auto / Русский / English / Қазақша / Other (BCP47 input).
  - Пробрасывать в `recap::run` как `lang_hint_override`. Если `preferred_language != 'auto'` — `build_system_prompt` использует его (override над `lang_detected`).
  - **НЕ форсить** в Soniox/Gladia — auto-detect остаётся (см. [Lang-tuning]) чтобы корректно распознавать речь на любом языке. Только LLM-output bias.
  - Action items: тоже на preferred_language (поле `lang_hint` в `replace_action_items` или просто в system_prompt).
  - Acceptance: при `preferred_language='ru'` рекап звонка с английским транскриптом — на русском; на `'auto'` поведение не меняется.

- [ ] **[B14] Live recording level meter (M7.1 follow-up).** Во время записи в `HomePage` пока только pulse-dot. Не видно, есть ли вход с микрофона / системного аудио. Требование:
  - Расширить Swift sidecar протокол: эмит'ить `{mic_rms: f32, system_rms: f32}` каждые 100ms через stdout (NDJSON line `{"kind":"level","mic":0.12,"system":0.34}`).
  - Tauri parsing: новый event `audio:level` с двумя float'ами (нормализованы 0..1).
  - DS-компонент `<LevelMeter mic={...} system={...} />` — две вертикальные/горизонтальные «лесенки» с 8-12 LED'ами, заполняются по RMS. Цвет: зелёный <-12dB, жёлтый -12..-3dB, красный >-3dB.
  - Anti-clip indicator: если значение > 0.95 хотя бы 100ms — мигает красный «CLIP» badge.
  - При записи на HomePage: показывать meter вместо/рядом с pulse-dot. При остановке — fade out.
  - Acceptance: говорим в микрофон — mic-meter растёт. Включаем YouTube/Zoom — system-meter растёт. Меняем громкость — meter реагирует в реальном времени.

## Production Readiness (B16)

> **Контекст**: после прохождения MVP-фич — 4 параллельных аудита (UX/CX, Visual/Design, Logic/Code Quality, Build/Deploy/Security) нашли ~260 пунктов разной серьёзности для перехода PoC → consumer-ready. Здесь — сводка с приоритетами. Items закрываются батчами; статус фиксируется галочкой ☑. **P0** = блокер для shipping / data loss / security. **P1** = serious UX / maintenance burden. **P2** = polish.

### Security & Build (10 P0)

- [x] **CSP strict** на webview (`tauri.conf.json` security.csp) — был null, теперь allowlist для self+proxy+R2+Google OAuth. Закрывает XSS escalation через markdown rendering.
- [x] **bundle.macOS.minimumSystemVersion '14.0'** + category productivity + Info.plist с NSMicrophoneUsageDescription + NSScreenCaptureUsageDescription. Без screen-cap string ScreenCaptureKit silently denies на macOS 14+.
- [x] **bundle.targets ['app','dmg','updater']** — больше не строим Windows/Linux artifacts случайно.
- [x] **R2 presign contentType allowlist** (`services/proxy/src/routes/stt.ts`) — 12 audio mime типов, reject text/html для phishing-hosting.
- [ ] **Tauri minisign pubkey** в `tauri.conf.json:52` placeholder. До первого публичного релиза — сгенерировать через `pnpm tauri signer generate`, public в config, private+password в GH Secret. Без этого updater не работает.
- [x] **Ad-hoc codesign в release-app.yml** — `codesign --force --deep --sign -` шаг добавлен после tauri-action. macOS 14+ Gatekeeper больше не ставит DMG в quarantine «damaged».
- [x] **Universal binary вместо двух DMG** — matrix macos-13+macos-14 заменена на macos-14 + `--target universal-apple-darwin`.
- [x] **Quota race fix** — best-effort CAS-loop через KV (3-attempt re-read+retry, см. rate-limit.ts). Full atomic CAS требует Durable Object — follow-up.
- [x] **Pipeline JoinHandle leak** — AppState.pipeline_tasks HashMap<call_id, JoinHandle>. Window close handler ждёт каждый task с tokio::timeout(8s) перед exit(0).
- [x] **SQLite integrity_check + backup** — startup integrity check, при corrupt rename *.corrupt-{ts}, fresh DB. (Nightly VACUUM INTO — отдельный backlog.)

### Security & Build (P1)

- [x] **shell:allow-open** в capabilities — сужено: `accounts.google.com/o/oauth2/**`, `appleid.apple.com/auth/**`, `login.microsoftonline.com/**/oauth2/**`, `{proxy}/v1/auth/**`.
- [x] **OIDC ID token claims validation** — `decodeIdTokenPayload` теперь проверяет exp/iss/aud (GoogleAdapter передаёт expected). JWKS signature — follow-up.
- [x] **consumeState CAS race** — best-effort через consumedAt tombstone + re-read verify. Full atomic CAS = DO follow-up.
- [x] **CORS /v1/*** — origin allowlist (tauri://localhost, http://tauri.localhost, http://localhost:5173, http://127.0.0.1:5173). /, /health открыты для smoke. Bearer-only auth, не cookie.
- [ ] **device-id spoof + IP rate-limit** — UUID regex недостаточно. HMAC-bind device-id с server-side secret при первом контакте + cf-connecting-ip rate-limit /16.
- [x] **panic hook** — backtrace в `~/Library/Logs/app.wotold.desktop/panic.log` + prev_hook chain.
- [x] **single-instance plugin** — `tauri-plugin-single-instance` v2 с feature deep-link, callback поднимает существующее окно.
- [x] **log rotation** — `max_file_size(5MB).rotation(KeepOne)` в tauri_plugin_log.
- [x] **Apple/Linux build guard** — compile_error! в audio/mod.rs для cfg(target_os="linux").
- [x] **README user-facing** — добавлена секция «Для пользователя» с 5 шагов установки + что Wotold не делает + если что-то не работает + privacy summary.
- [x] **Privacy Policy + ToS** — `docs/PRIVACY.md` создан (v0.1, GDPR Art. 13). Ссылка из Onboarding step 1 — follow-up.
- [x] **Delete-all-data button** — Settings → 🗑 Конфиденциальность → красная кнопка с confirm. Стирает calls/, app.db, device.json, BYO ключи и session. Требует ручного restart.

### UX / CX (10 P0)

- [x] **Internal-jargon leak фикс** (R2/M3.6/M10/B11/X4/embedding/voice_samples/BYO/Managed/SSO/provider_path) — 15+ мест в SettingsPage/AccountSection/SpeakersSection/VoiceSamplesSection/ContactsPage/CallDetailPage переписаны на человеческий русский.
- [x] **Post-Stop Open CTA на HomePage** — было `✓ Звонок сохранён: id8…`; стало success-card с большой кнопкой «Открыть» → навигация в CallDetailPage. Закрывает разорванный CJM «запись → стоп → видеть результат».
- [x] **Skeleton loaders** — DS Skeleton + CallRowSkeleton, заменяет голый `<p>Загрузка…</p>` на shimmer-rows на CallsPage. Применить также на CallDetailPage / SettingsPage / ContactsPage.
- [x] **Tab labels human-readable**: «Рекап» → «Саммари», «Спикеры» → «Участники», «Action items» → «Задачи»
- [x] **Onboarding step Permissions** — добавлен step 2 с embed PermissionsSection до consent/имени.
- [x] **Onboarding step Consent** — consent перенесён в step 3 онбординга (плюс остался one-time fallback в HomePage).
- [x] **HomePage hero** — stats-row (всего / неделя / последний clickable) + recent-list 3 для one-click open. Device-id убран из UI.
- [x] **Audio player на CallDetailPage** — `<audio preload="metadata">` + track switch mic/system, через tauri assetProtocol.
- [x] **Error mapper** — `src/api/errors.ts` (humanError + 25 regex). Заменён setError(String(e)) во всех страницах.
- [x] **CallsPage group-by-date** — sticky headers «Сегодня / Вчера / На неделе / месяц». groupByBucket в CallsPage.
- [x] **CallsPage virtualization** — react-window v2 List при filtered.length >= 200. <200 — grouping by date.

### UX / CX (P1)

- [ ] **Settings auto-name из NSFullUserName** в onboarding (default «Я» + edit). Требует Swift bridge — отложен.
- [x] **Hotkey ⌘⇧R для start/stop** записи. Window-level keydown, обе раскладки, ignore при input focus.
- [x] **Pre-check permissions** перед start_recording — Rust check перед sidecar start, clear error.
- [x] **CallDetailPage auto-name** для звонка без title — «{contact name} · 20 мая» если есть confirmed speaker.
- [x] **Failed banner с CTA** — «Попробовать ещё раз» / «Пересоздать саммари» внутри call-failed-banner на CallDetailPage.
- [x] **Pipeline progress в topnav** — pipeline:started/finished events + counter в App, subtle pill 'обрабатываем N…' с spinner.
- [x] **BYO ключи validation** — Settings → BYO secrets section warn если все ключи пустые (red border-left) или часть (yellow). Юзер видит до попытки записи.
- [x] **Контакты search** — фильтр по name/org/role/identifiers/notes когда >5 контактов. Identifier kind icons + attributes UI follow-up.
- [ ] **Export markdown** для recap/transcript из CallDetailPage.
- [x] **CSS responsive breakpoints** — @media (max-width: 760px) topnav-label hide + call-row 2-row + app padding; (max-width: 560px) home-stats 1col + tabs wrap.
- [ ] **Recording level meter (B14 backlog)** — Swift sidecar emits RMS → DS LevelMeter.

### UX / CX (P2)

- [x] **Coachmarks на первом запуске** — Coachmarks.tsx, 4-step overlay (ONBOARDING_DONE=1 + COACHMARKS_SEEN!=1), keyboard nav + reduced-motion.
- [x] **macOS app menu** — Tauri 2 MenuBuilder с Wotold/Edit/View/Window submenus. Native Cut/Copy/Paste теперь работают в webview.
- [x] **Window min-size 760x560** — поднят с 640x480 в tauri.conf.json.
- [x] **macOS toast при сохранении settings** — pill «✓ Сохранено» 1.5s, fade-in/out, reduced-motion respect.
- [x] **Toolbar subtitle + sticky** — props добавлены, CallsPage использует с правильным склонением ru ('12 звонков').

### Visual / Design (P0)

- [x] **Top nav rework** — segmented topnav-tab с emoji-icon + underline-active indicator. SVG-icon set (lucide-react) — P1 follow-up.
- [x] **Sidebar или icons в nav** — закрыто через lucide-react SVG icons в segmented topnav.
- [x] **Title bar overlay + traffic lights padding** — titleBarStyle Overlay, hiddenTitle true, trafficLightPosition 18×18. topnav padding-left 88px + app-region: drag (no-drag на interactive).
- [x] **HomePage hero block** — stats cards + recent 3 list.
- [x] **Record-button visual weight** — accent→danger gradient + inset highlight + 6px outer glow ring на hover.
- [x] **Onboarding hero**: step-dots indicator реализованы (B16 batch P0). Icon + screenshot preview — follow-up.
- [x] **App identity в UI** — Brand label «Wotold» слева в topnav. SVG-logo — follow-up.

### Visual / Design (P1)

- [x] **SVG icon set** — lucide-react добавлен. Topnav nav-tabs мигрированы (Home/Phone/Users/Settings). Остальные места (status-cell ⏺⚙✓✗, кнопки) — follow-up, currently emoji-based but readable.
- [x] **Status-cell processing spinner** — уже был, animation ds-spin 1.2s linear на data-status='processing'.
- [x] **CallRow depth** — micro-elevation translateY(-1px) + shadow-1 на hover. Avatar/chevron — follow-up.
- [x] **Failed banner как Alert component** — call-failed-banner с danger border + icon в circle + retry button inside (CallDetailPage).
- [x] **Settings sections с иконками** — 🔐/🎙/🤖/⚙/🌐/🔑/👤/📊/🗑 в settings-section-title (SettingsPage).
- [x] **Empty states с дефолт-иконками** — Empty.tsx fallback на ✨ если caller не передал свой icon.
- [x] **Transcript bubble max-width** — `min(75%, 36rem)` вместо просто `75%`.
- [x] **Permissions section dashed border → solid**.
- [x] **Tabs active state visual** — `background: var(--color-surface-sunken)` + `font-weight: 600` для active trigger.

### Logic / Code Quality (P0)

- [x] **Pipeline JoinHandle storage** — реализовано через AppState.pipeline_tasks + graceful await на window close.
- [x] **Recap fail persistence** — migration 0004 + recap_failed_reason поле, pipeline catches recap error и пишет в БД, UI banner с retry.
- [x] **OIDC ID token signature** — exp/iss/aud claims validation в decodeIdTokenPayload + GoogleAdapter wired.
- [x] **consumeState CAS** — best-effort через consumedAt tombstone + re-read.
- [x] **Quota race CAS** — 3-attempt retry loop в incUsage.
- [x] **Soniox poll timeout** — явный throw 'soniox poll timeout (job ...)' вместо fall-through.
- [x] **deviceId UUID validation в /v1/auth/start** — UUID regex, 400 bad_request если не UUID.
- [x] **ReactMarkdown rehypeRaw audit** — rehypeRaw / dangerouslySetInnerHTML не используется, CSP closes остальное.
- [x] **FK ON DELETE для call_speakers.contact_id, action_items.owner_contact_id, voice_samples.source_call** — migration 0003 с SET NULL.

### Logic / Code Quality (P1)

- [ ] **Zod schemas в proxy boundary** — заменить hand-rolled `typeof body.X !== 'string'` validation на `z.object({...}).parse()`. Routes: stt/auth/llm/usage.
- [x] **Hand-rolled Promise.all → Promise.allSettled** в `CallDetailPage` — критична только call meta, остальные artifacts soft-fail с console.warn.
- [x] **`as 400 | 502 | 503` type cast в llm.ts** — заменён explicit whitelist.
- [x] **`.catch(() => {})` silent ignores** в HomePage — заменены на console.warn.
- [x] **Wide `#[allow(dead_code, unused_imports)]`** — surgical allows только на #25 voice-matching scaffold (embeddings/matching/identify/etc), точечные allows на NotImplemented variants. Cargo check: 0 warnings.
- [x] **Cargo.toml `[lints]`** — unsafe_code = forbid, clippy::unwrap_used/expect_used/panic = warn.
- [ ] **Split db/calls.rs** (791 строка) — отложен; single-domain, high cohesion. Acknowledged tech debt.
- [x] **Extract managed_stt_request helper** — `proxy_managed::transcribe_via_proxy` устраняет ~95 строк дубликации в soniox.rs/gladia.rs.
- [x] **audio_io::extract_segments_batch** — single WAV open + slice. Будет использоваться в #25 ONNX wire-up. +2 теста.
- [x] **Soniox text concat без пробелов** — needsSpaceBefore() вставляет пробел между letter-bordered tokens (anti-склейка ru/kk).
- [x] **LIKE wildcards escape в MCP db.ts** — escapeLikePattern() + `ESCAPE '\\'` в SQL.
- [x] **PRAGMA busy_timeout** — `busy_timeout(5s)` в db/mod.rs `init()` connect options.
- [ ] **EMBEDDING_DIM в schema** — добавить `voice_samples.embedding_dim INTEGER` column, reject mismatched.
- [ ] **partner stderr no leak в proxy logs** — Cloudflare observability ловит `console.error`. Соскрести device-id/r2Key из 200-char message.
- [ ] **LLM upstream error generic для клиента** — сейчас `groq 400: detail...` уходит юзеру. Логировать в console.error full, возвращать `'upstream error'` обобщённо.
- [ ] **call_fts virtual table** — создана в 0001_initial.sql, никогда не populated. Либо implement #30, либо drop.

### Logic / Code Quality (P2)

- [ ] **CallsPage listen pipeline:finished** — только когда CallsPage mounted, не глобально.
- [ ] **Wrap JSON.parse(rawSttJson) в zod** в InteractiveTranscript.
- [ ] **`let _ = &call` comment** в pipeline/mod.rs — discarding result чище.
- [ ] **NaN guard в merge_tracks sort** — фильтровать NaN start times.
- [ ] **chunk.try_into() → manual array** в embeddings.rs.

### Tests (P1)

- [ ] **voice_samples cascade test** — verify `ON DELETE CASCADE` для samples.
- [ ] **delete_call_and_samples** — test для action_items + call_speakers cleanup.
- [ ] **pipeline::run/reprocess_call/regenerate_recap** — нет unit тестов. Cover happy + missing audio + recap fail.
- [ ] **STT KV-resume happy path** integration test.
- [ ] **OIDC ID-token signature negative tests** после P0 fix.
- [ ] **MCP prompt-injection content** — pass-through test (M8.3).

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
