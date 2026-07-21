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
- [ ] **Assistant-таб** ⏸ отложен отдельной доработкой (LLM Q&A над транскриптом + endpoint).
- [ ] **Contacts B18.4 доводка** — identifiers chips (email/phone), derivations (calls-count / last / confirmed — новый query).
- [ ] **Manual visual QA** (light/dark, все экраны) — human follow-up, агент не скриншотит native app.
- [ ] **B18 a11y follow-up** — recording-state live-region (SC 4.1.3); отдельный aria-label тема-toggle; toast dismiss контекст (SC 4.1.2); capabilities least-privilege split для recording-widget; токен `--on-danger`; контраст `--wc-*` в dark; зачистка dead i18n `home.*`/`calls.*`.
- [ ] **Сверка контрактов (S2)** — `ActionItemV2`/`Decisions`/`OpenQuestions` уже в `packages/contracts` (M14) — переиспользовать, не дублировать.

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
