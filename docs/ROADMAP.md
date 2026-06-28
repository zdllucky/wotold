# Roadmap

> Декомпозиция Этапов раздела 11 [ПАСПОРТА](ПАСПОРТ_ПРОЕКТА.md) на единицы реализации. Файл — источник истины по статусу фич, читается и обновляется людьми. Параллельно в харнессе Claude Code лежит TaskList с теми же ID — синхронизируется вручную в этом файле при изменении статуса.
>
> Легенда: `[x]` готово · `[ ]` пендинг · `→ #N` блокируется задачей N.

---

## Статус MVP

**Реализовано:**
- Этапы 1-5, 6-10, 11-12 по [паспорту](ПАСПОРТ_ПРОЕКТА.md): audio capture (mic+system), STT relay (Soniox+Gladia с auto-fallback), pipeline (transcript+raw_stt), recap+action_items (Groq Llama 3.3 70B), CallDetail с интерактивным транскриптом+spoken bubbles+Speakers+regenerate, contacts с edit+samples view, settings (managed+BYO+quota+account), MCP server, OIDC scaffold, auto-update, CI/CD (split staging/prod + smoke+rollback + commitlint + claude-review + changelog).
- Staging backend полностью boevoy: /health, /v1/usage, /v1/stt/staging-url (R2), /v1/llm (Groq), /v1/auth/google/start.
- Все B1-B12 + B14 + B15 backlog requirements закрыты.
- **B17 Atelier v2 redesign** — все 7 страниц мигрированы точь-в-точь по handoff (HomePage, CallsPage, CallDetailPage, SpeakersSection, ContactsPage, OnboardingPage, DesignSystemPage) + Coachmarks; design gate enforcement active.
- **Recording UX (V2)** — реальный live waveform (Swift RMS → Rust event → React canvas), dual-track sync playback, real WAV peaks, sticky AudioScrubber с current-speaker chip + click-to-jump.
- **CallDetail (V3/V4)** — header с LLM-generated title + ParticipantsRow + kebab actions; interactive transcript ↔ audio scrubber sync; inline «? кто это» chip → SpeakerConfirmModal.
- **B3.1–B3.6 voice biometrics scaffold** — cluster_embedding column, WAV chunker, extract_clusters, matching → suggestion, confirm hook → voice_samples (C2-gated). Embedder dispatcher готов, ONNX runtime ждёт B3.7.
- **Native UX (V4.1/V4.2)** — overscroll lock, user-select только на текстовых блоках, ПКМ context menu блок (whitelist на input/markdown/transcript).

**Осталось для production-релиза (manual user actions):**
- **#42 X1 Tauri minisign** — генерация ключа подписи updater'а (one-time CLI).
- **#44 X3 CF production** — те же 7 GH Secrets с суффиксом `_PRODUCTION` + Google OAuth Authorized URI + tag `v0.1.0`.

> Полный пост-MVP беклог (release-блокеры, verification gaps, tech-debt, polish) — в секции [**## Беклог (groomed)**](#беклог-groomed) внизу файла. Этот файл — единственный источник истины по статусу (прежний `CHUNKED_PIPELINE_BACKLOG.md` влит сюда и удалён).

**Biometric matching (B3.x) — статус:** OnnxEmbedder через sherpa-onnx WeSpeaker (B3.7a-c) + on-demand model download реализованы; B3.x cluster-pipeline (`run_cluster_pipeline`) пишет cluster в `call_speakers.cluster_embedding`, матчит против consenting `voice_samples`, suggestion + confirm-hook работают. Осталось (см. §Беклог B): integration-тест против reference-эмбеддинга (B3.7d, нужен bundled WAV) + сверка/wire старого `identify_speakers` orchestrator (#25) с актуальным cluster-path.

---

## Что было сделано в недавних батчах (V2–V4 + B3.x)

> Сводка по PDCA итерациям май 2026. Все детали — в commit log + секция «Готово». Здесь — высокоуровневая навигация для контекста.

### V2 · Recording UX
- Real-time waveform: Swift sidecar → DispatchSourceTimer 100ms RMS → NDJSON stdout → Rust event dispatcher → Tauri `audio:level` event → React `useAudioLevel` hook → canvas waveform с DPR scaling.
- Убраны synthetic-fallback'и (на тишине бары плоские, без surrogate noise).
- DualWaveform stereo split на recording screen.

### V3 · CallDetail полировка
- Sticky bottom AudioScrubber (vs прежний `<audio>` player): owns 2 `Audio()` элемента (mic + system), browser mixer сводит, drift compensation > 50ms.
- Real WAV peaks через `AudioContext.decodeAudioData` + combined element-wise max между mic/system, cache в Map<src, peaks[]>.
- Clip-path progress fill: foreground bars точно align'ятся с background.
- Sticky bottom через flex column + `marginTop: auto` — scrubber всегда у низа scroll viewport.
- InteractiveTranscript ↔ AudioScrubber sync: active row подсветка (`var(--accent-soft)` + speaker color border-left), auto-scroll, click row → seek.
- Speaker chip в scrubber (140px fixed slot — waveform не прыгает) показывает текущего спикера, click → jump to transcript.
- Custom `<Select>` per Atelier (keyboard nav, A11y combobox+listbox+aria-activedescendant).

### V3.9 · CallDetail header под reference §5
- Human meta: «СРЕДА · 20 МАЯ · 16:04 · 1 МИН 12 СЕК» (вместо provider+lang noise).
- LLM-generated title из recap (`recap.json.title`, 3–7 слов headline-style) с fallback «Звонок · 20 мая».
- Kebab menu top-right (Reprocess + Delete).
- ParticipantsRow с `.sp`/`.sp-avatar` chips + русское pluralization «N участник[ов/а]».

### V4.0 · LLM title
- Pipeline `recap::run` теперь генерит `title` поле + persist'ит в `calls.title` через `db::set_call_title`.

### V4.1 · Native UX polish
- **Overscroll lock**: `overscroll-behavior: none` на html/body/#root — нет rubber-band bounce.
- **Selective text-select**: `user-select: none` + `cursor: default` дефолтно, whitelist на `.markdown/.transcript-*/.title/.display/.subtitle/code/pre/kbd/input/textarea/[data-selectable]` (cursor: text + user-select: text).
- **Dynamic ParticipantsRow**: SpeakersSection принимает `onSpeakersChanged` callback, CallDetailPage передаёт refetch — header chip'ы обновляются после confirm/unbind.
- **Sample playback fix**: «▶ сэмпл» в SpeakerCard реально играет фрагмент из mic.wav/system.wav (owner→mic, прочие→system) с currentTime range. Hidden `<audio>` + watch listeners. Кнопка показывает «▶ N сек» / «◼ стоп» / disabled.
- **Inline «? кто это» chip** в InteractiveTranscript для не-confirmed спикеров → открывает `<SpeakerConfirmModal>` с тем же `SpeakerCard` внутри (compoт-reuse). focus-trap + Esc + click-outside.
- `SpeakerCard` вынесен в `components/SpeakerCard.tsx` — переиспользуется и табом «Участники» и SpeakerConfirmModal.

### V4.2 · ПКМ блок
- Глобальный `document` contextmenu listener preventDefault'ит, кроме whitelist'a (inputs / markdown / transcript / [data-selectable]). В DEV оставлен для Inspect.

### B3.1–B3.6 · Voice biometrics scaffold
- **B3.1** Migration 0007 → `call_speakers.cluster_embedding BLOB`.
- **B3.2** `audio::wav_chunker::read_wav_segment` — hound-based PCM int16 → f32 mono slicing + 5 unit-тестов.
- **B3.3** `pipeline::clusters::extract_clusters(merged, mic, sys, embedder)` — per-tag segments ≥0.5s (cap 10s), mean-pool + L2 normalize + 3 unit-теста.
- **B3.4** Matching wire — `set_call_speaker_cluster` + `rank_candidates(min_score=0.5)` → `set_call_speaker_suggestion`. Non-fatal в pipeline.
- **B3.5** Confirm-хук → `voice_samples` — `db::confirm_call_speaker` проверяет `attributes.consent_voice='true'` (C2) + читает cluster + INSERT в той же транзакции.
- **B3.6 scaffold** `embeddings::try_load_onnx_embedder(model_path)` → `Option<Box<dyn Embedder>>` (default None pre-B3.7). `PipelineCtx.app_data_dir` thread-through. Pipeline резолвит `$APP_DATA/models/embedder.onnx` и fallback'ит на StubEmbedder.

### V5 · Polish + i18n + speaker UX
- **V5.1–V5.3** Speakers UX: contact picker search + details (org/role/email/phone); кнопка «соединить как один контакт»; HumanSpeakerLabel («Голос N» вместо «S1»).
- **V5.4** Input focus visual fix (border-color + box-shadow accent-soft); «↻ Пересоздать саммари» только в kebab menu.
- **V5.5** i18n ru/kk/en — `useI18n` context + `detectSystemLocale` (navigator.language) + Settings → Внешний вид → Язык интерфейса. Persisted в `settings.ui_locale`.

### V6 · Async-states UI per design handoff
- **V6.1** Foundation — `types/callState.ts` (CallState, CallProgress, CallError + PIPELINE_STEP_KEYS) + 4 компонента (`CallStateTag`, `ProgressRail`, `PipelineStrip`, `CallErrorRow`) + ~330 строк CSS (`.stat-tag` 6 variants, `.rail`, `.skel`, `.caret`, `.steps`, `.proc-strip`, `.transcript-row--ghost/--streaming`, `.activity-strip`, `.call-error-row`, `.btn--danger`) + 19 vitest unit-тестов; всё уважает `prefers-reduced-motion`.
- **V6.2** Backend progress wiring — migration `0008_pipeline_progress.sql` (`pipeline_step/pct/eta_sec/upload_bytes`) + `db::set_call_progress` + clear-on-ready/failed/reprocess + Tauri событие `call:progress` эмитится на 5 transitions (upload→stt→speakers→merge→recap) + 3 cargo тестa.
- **V6.3** CallsPage — `CallStateTag` per row (live/uploading/processing/queued/error) + `ProgressRail` для processing rows + global `.activity-strip` banner при `activeCount > 0` + `CallErrorRow` для failed + live `call:progress` patcher (без full refetch).
- **V6.4** CallDetailPage processing — `PipelineStrip defaultOpen` с stage-метка + caret + ETA + 5-step body + reassurance строка «Можно закрыть окно — мы сохраним прогресс» + ghost-rows skeleton транскрипта + per-page `call:progress` listener.
- **V6.5** CallDetailPage error — `ErrorScreen` (calm explanation + 3 retry actions включая alt-provider hint + diagnostics `<details>` с code/provider/last_at/quota) + AudioScrubber теперь enabled для failed (аудио всегда доступно).
- **V6.6** Cleanup — i18n keys (`callState.*`, `pipeline.*`, `calls.activityStrip*`, `callDetail.reassureCanClose`, `callDetail.errorTitle/errorAudioSaved/errorRetry/errorRetryProvider/errorOpenSettings/errorDiagnostics*`) на ru/kk/en; cargo fmt + clippy clean; 231/231 vitest + 135/135 cargo passing.
- **V6.7** DS page — §14 секция «Async states (V6)» с live снимками всех новых компонентов (CallStateTag 6 variants + detail-suffix, ProgressRail determinate/indeterminate, PipelineStrip step 3/5, CallErrorRow, activity-strip, transcript-row--ghost). Реагируют на theme×accent.

### V7 · Auto-bind speaker по голосу (opt-in)
- **R2 deviation согласована**: opt-in toggle = «оптовое подтверждение» юзера, не нарушает паспорт.
- **Migration 0009** — `call_speakers.auto_bound_at TEXT` (RFC3339, NULL = ручное; не-NULL = авто-привязано).
- **`db::auto_bind_high_confidence_speakers(pool, call_id, threshold)`** — guardrails: speaker != owner AND confirmed=0 AND contact_id NULL AND suggestion_score ≥ threshold AND contact has `consent_voice='true'` AND contact has ≥ 2 voice_samples. Never overwrites existing binding. 7 cargo tests (high-score happy path + 5 negative scenarios + unbind-clears-auto_bound_at).
- **Pipeline wiring** — `run_auto_bind` after cluster matching, читает settings (default OFF, threshold 90/95/98 default 95), emit'ит `call:auto_bound` event `{ call_id, count, threshold_pct }`.
- **Settings UI** — Транскрипция → checkbox «Авто-привязка по голосу» + threshold Select (показывается только когда enabled). i18n keys ru/kk/en.
- **CallDetailPage AutoBoundBanner** — `.activity-strip` стиль + «↩ Отменить» кнопка, unbind'ит все автопривязки одним кликом. Listener on `call:auto_bound` event для live refresh.
- **API extensions** — `CallSpeakerView.auto_bound_at`, `CallAutoBoundEvent` типы.
- `unbind_call_speaker` теперь очищает `auto_bound_at` тоже — повторное ручное подтверждение даст clean state без auto provenance.

### W · Recording flow redesign (handoff RECORDING-FLOW.md)
- **W1** Configurable hotkeys — `utils/hotkey.ts` (parse/serialize/format/match/capture/isReserved) + `HotkeyCapture` component + Settings UI. layout-independent через `e.code`. RESERVED list блокирует ⌘W/⌘Q/⌘C. 25 hotkey unit tests.
- **W2** Backend pause/resume — migration 0010 (`paused_at TEXT` + `paused_total_ms INTEGER`). `db::pause_call` / `resume_call` + 9 cargo tests (idempotent re-pause, multi-cycle accumulation, finish-with-lingering-pause). `pause_recording` / `resume_recording` Tauri commands. v1 = Rust-level pause (frames пишутся в один WAV, silence-trim в pipeline); Swift sidecar pause = TODO v2.
- **W3** RecordingProvider + UI components — `useRecording()` hook (status idle/recording/paused, elapsedSec с frozen pause, start/pause/resume/stop). `RecEq` (3-span animated equalizer + paused fallback), `RecMiniButton` (pause/play/stop CSS-only glyphs), `RecStrip` (persistent role="status" над app-main). ~200 lines CSS (`.rec-eq`/`.rec-strip`/`.rec-mini-btn` + `prefers-reduced-motion: reduce`). 11 vitest.
- **W5** HomePage refactor — recording state снят (живёт в RecordingProvider), fullscreen overlay удалён (~260 sloc). Hero copy idle/recording/paused. ⌘⇧R toggle start↔stop + ⌘⇧P pause↔resume через configurable settings. Stop → onOpenCall(callId) → CallDetailPage. cmd+W/Q intercept с confirmation через plugin-dialog ask().
- **W6** DS page §15 «Recording controls» showcase: RecEq active/paused, RecMiniButton 3 variants, RecStrip recording+paused snapshots.
- **W4 pending** — RecFloat second Tauri window (мини-виджет при minimize). Требует tauri.conf.json edit + alwaysOnTop transparent decorations:false window + IPC bridge. Отложено как самостоятельная фича: основная backend/UI инфраструктура (RecordingProvider, pause/resume, stop→detail) уже работает в основном окне без виджета.

### S · Auto-detect call popup (R3 deviation opt-in)
- **S1** Settings foundation — `SETTINGS_KEYS.CALL_DETECT_ENABLED` + `CALL_DETECT_COOLDOWN_MIN` + whitelist '3'|'5'|'10'|'15'. SettingsPage блок «Авто-предложение записи» (toggle + conditional cooldown select). i18n ru/kk/en privacy-framed копи. CLAUDE.md R3 row дополнен deviation-нотой. Default OFF (R2→V7 pattern).
- **S2** Swift sidecar probe — `CallActivityProbe.swift` опрашивает `kAudioDevicePropertyDeviceIsRunningSomewhere` на default input device (1.5s tick), matches NSWorkspace frontmost bundle id против whitelist (Zoom/Teams/FaceTime/Discord/Telegram/Skype/Webex + Chrome/Safari/Arc/Firefox/Edge для Meet). State machine idle/detected/suggested; emit `call_suggested` once at transition. Никакая аудио-дорожка чужого процесса не читается — только Core Audio busy флаг. New stdin commands `call_detect_start` / `call_detect_stop`.
- **S3** Rust dispatcher — `audio/call_detect::CallDetectController` хранит долгоживущий sidecar-child, слушает NDJSON, per-bundle in-memory cooldown (3/5/10/15 min из настройки, рестарт обнуляет), эмитит typed Tauri event `recording:suggested`. Эмит подавляется пока активна своя recording session. 6 cargo tests. Bootstrap из настроек в lib.rs setup hook.
- **S4** Native macOS notification — tauri-plugin-notification зарегистрирован + capability `notification:default`. `audio/call_detect::maybe_emit` после Tauri-event дополнительно пушит `Wotold — обнаружен звонок` баннер. Работает даже когда окно свёрнуто.
- **S5** In-app SuggestBanner — `recording/SuggestBanner.tsx` слушает `recording:suggested`, рендерит `.suggest-banner` с accent border-left (не signal: запись ещё не началась), кнопки «Начать запись» / «Скрыть», auto-dismiss через 30s. Снимается автоматически когда recording стартовал любым путём. i18n ru/kk/en. 3 vitest.
- **S6** DS page §16 «Auto-detect call» + ROADMAP S-section + commit'ы.
- **S7** RecFloat drag + position persistence — `data-tauri-drag-region` уже стоял на всех body частях, но не валидировался из-за конфликта с click-restore. Решение: frontend mousedown/mouseup screen-distance threshold (6px) — при движении >threshold click-restore swallow'ится. Rust window `Moved` listener дебаунсит 400ms, persist'ит logical X/Y в settings (`recording.widget.x` / `.y`). `show_recording_widget` теперь читает saved position первым, fallback на top-right primary monitor. `clamp_to_visible_area` helper для off-screen edge case. 2 новых vitest + cargo widget tests passing.

### Hardening
- M8.3 prompt-injection pass-through + LIKE escape regression тесты (MCP).
- voice_samples cascade тесты (C5): delete contact → cascade samples; delete call → SET NULL source_call.

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
- [~] **#26** partial M3.5+3.7 — UI confirmation flow + mic→owner auto-bind. `db::{list_call_speakers, confirm_call_speaker, unbind_call_speaker, auto_bind_owner_speaker}` + view с join'ом display_name по contact_id + 6 unit-тестов. Tauri commands `list_call_speakers/confirm_call_speaker/unbind_call_speaker`. UI новая таб «Спикеры» в `CallDetailPage` через `SpeakersSection` (suggestion hint с confidence + источник, контакт-селектор, кнопки Подтвердить/Отвязать; R2 enforced — финальная привязка только через явный confirm). **M3.7 mic→owner auto-bind**: `pipeline::run` после persist_artifacts автоматически вставляет confirmed=1 row для `speaker_tag="owner"` → owner contact. Не нарушает R2 потому что owner=сам пользователь. Dev mock с in-memory speakerBindings. **Остаётся (deferred)**: OnnxEmbedder уже приехал (B3.7a-c, sherpa-onnx) и B3.x cluster-path работает — остаётся свериться, нужен ли ещё старый `identify_speakers` orchestrator (#25) или он вытеснен cluster-pipeline; dynamic sample update (N=5). См. §Беклог C.
- [x] **B3.1** Migration 0007 `ALTER TABLE call_speakers ADD COLUMN cluster_embedding BLOB NULL` — per-call cluster vector хранится рядом с suggestion_score, нужен B3.5 confirm-хуку чтобы перенести в `voice_samples` без recompute.
- [x] **B3.2** `audio::wav_chunker::read_wav_segment(path, start, end)` — hound-based PCM int16 → f32 mono slicing для embedder input. Multi-channel fold, sample-rate echo, 5 unit-тестов (full file / mid slice / out-of-range / mono fold / SR detection).
- [x] **B3.3** `pipeline::clusters::extract_clusters(merged, mic_path, sys_path, embedder)` — per `speaker_tag` собирает segments ≥ 0.5s (cap 10s), читает WAV chunk через wav_chunker (owner→mic, прочие→system), embedder.extract → mean-pool + L2 normalize → `HashMap<tag, Vec<f32>>`. 3 unit-теста с `CountingEmbedder` mock.
- [x] **B3.4** Matching pipeline wire — `run_cluster_pipeline` persist'ит cluster в `call_speakers.cluster_embedding` через `db::set_call_speaker_cluster`, затем top-1 cosine candidate с `min_score=0.5` через `matching::rank_candidates` → `db::set_call_speaker_suggestion`. Non-fatal: ошибки логятся и пропускаются, recap всё равно генерится.
- [x] **B3.5** Confirm-хук → `voice_samples` — `db::confirm_call_speaker` теперь читает `contacts.attributes.consent_voice` + `call_speakers.cluster_embedding` и в той же транзакции INSERT'ит `voice_samples` (quality=score, source_call=call_id). C2 enforced: без consent образец не записывается. owner-привязка остаётся отдельным `auto_bind_owner_speaker` (M3.7).
- [~] **B3.6 scaffold** Embedder dispatcher — `embeddings::try_load_onnx_embedder(model_path)` возвращает Option<Box<dyn Embedder>> (всегда None в default build, scaffold под `#[cfg(feature = "voice-onnx")]`). `PipelineCtx` теперь содержит `app_data_dir`, `run_cluster_pipeline` резолвит `$APP_DATA/models/embedder.onnx` и fallback'ит на StubEmbedder если модели нет. Honest scaffold: реальный ONNX runtime + Kaldi fbank preprocessing + integration test против reference WeSpeaker output — отдельный пункт B3.7 (без верификации preprocessing генерил бы garbage embeddings в `voice_samples`, ломая cross-call matching).
- [x] **B3.7a/b** OnnxEmbedder via `sherpa-onnx` Rust crate — research показал что fbank-in-graph ONNX моделей для production не существует (WeSpeaker / 3D-Speaker / NeMo все требуют pre-computed mel-fbank features). Вместо ручной реализации Kaldi fbank + ort inference выбран официальный `sherpa-onnx` crate (k2-fsa, static link by default, prebuilt ONNX Runtime libs auto-download) который wrap'ит весь pipeline: Kaldi fbank → ONNX inference → L2 normalize. `OnnxEmbedder::load()` + `extract()` через `SpeakerEmbeddingExtractor`, `EMBEDDING_DIM` валидация модели, defensive L2 normalize над output. Default build не тянет sherpa-onnx — `--features voice-onnx` включает. Тесты на missing-model path. cargo check/clippy/fmt/test зелёные на обоих режимах.
- [x] **B3.7c** Model runtime download + Settings UI. `voice_model.rs` модуль: `check_status(app_data_dir)` (file_sha256 streaming verify), `download(...)` с `reqwest::Response::bytes_stream` + emit'ит `voice-model:progress` каждые 256KB через Tauri events + atomic rename `.partial` → `embedder.onnx` после SHA256 match, `delete(...)` для GDPR opt-out. WeSpeaker URL + SHA256 захардкожены: `e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012`. 4 Tauri commands: `voice_model_status/download/delete/info`. Frontend `VoiceModelSection.tsx` в Settings → новая секция «Распознавание голоса» с status badge (нет / качаем / установлена / повреждена) + progress bar real-time + "Технические детали" details (URL, SHA256, feature_enabled flag). Bundle vs download decision — пользователь сам решает: на первой записи UI его не блокирует, opt-in в Settings. 4 unit-теста (missing/corrupted/delete idempotent/path layout).
  - **B3.7d remaining**: integration test против reference embedding для зашитого WAV (sherpa-onnx fangjun-sr-1.wav) — отдельный шаг (требует bundled test fixture + `--features voice-onnx`).

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

## Backlog кандидаты — закрытая история (B1–B14)

> Лента ранних идей B1–B14, **все закрыты**. Оставлено как исторический след того, что было groomed и сделано. **Активный пост-MVP беклог** — в секции [**## Беклог (groomed)**](#беклог-groomed) внизу файла.

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

- [x] **[B13] Предпочитаемый язык — системная настройка.** Готово: `SETTINGS_KEYS.PREFERRED_LANGUAGE` + `PREFERRED_LANGUAGES` array (auto/ru/en/kk) в `api/settings.ts`. UI в `SettingsPage` → секция «Распознавание» → `<Select<PreferredLanguage>>` («Язык рекапа и задач», hint: «Не влияет на сам STT»). Backend pipeline уже читает `SETTING_PREFERRED_LANGUAGE` в `pipeline::run` + `regenerate_recap` — если не 'auto' → override `lang_detected` в `recap::run`. Acceptance соблюдён: STT auto-detect не трогается, LLM bias только.

- [x] **[B14] Live recording level meter (M7.1 follow-up)** — закрыто V2 batch: Swift sidecar эмитит `{kind:"level", mic, system}` каждые 100ms через NDJSON stdout (DispatchSourceTimer). Rust парсит → emit `audio:level` событие в frontend. `useAudioLevel` hook + `<LiveWaveform>` рендерит real-time canvas с DPR scaling. Synthetic-fallback убран — на тишине бары плоские. Dual-track stereo split (mic + system) на recording screen.

## Atelier v2 Redesign (B17)

> **Контекст**: 2026-05-20 получен полный design handoff — `docs/design/atelier-v2/`. Editorial / transcript-first direction, Bordeaux accent (default), Source Serif 4 + DM Sans + JetBrains Mono. Light + dark × bordeaux/persian/ink (6 комбинаций). PDCA по handoff `README.md` § "Implementation plan". Mandatory design gate активирован — см. CLAUDE.md.

### Foundation (выполнено)

- [x] Скопированы handoff sources в `docs/design/atelier-v2/` + canonical source-of-truth.
- [x] `apps/desktop/src/styles/tokens.css` заменён на Atelier v2 (color-scheme aware light/dark, accent swatches via `data-accent`).
- [x] `apps/desktop/src/styles/wotold.css` (component classes) + `fonts.css` (Google Fonts CDN; self-hosting опция в комментарии).
- [x] `apps/desktop/src/styles/legacy-tokens.css` shim — мост `--color-*` → новые токены, удалить когда вся миграция завершится.
- [x] `apps/desktop/src/theme/useTheme.tsx` + `<ThemeProvider>` (persist через `api/settings`).
- [x] `SETTINGS_KEYS.UI_THEME` + `UI_ACCENT` добавлены.
- [x] `main.tsx` import order: fonts → tokens → legacy-tokens → wotold → global → ui → pages.

### Design gate enforcement (выполнено)

- [x] `.claude/skills/design-gate/SKILL.md` + `.claude/commands/design-gate.md` — mandatory pre-UI checklist.
- [x] ECC skills адаптированы локально: `design-system`, `frontend-design-direction`, `accessibility`, `motion-ui`, `frontend-patterns`.
- [x] ECC agent `a11y-architect` подключён в `.claude/agents/`.
- [x] `scripts/hooks/design-gate.mjs` PostToolUse — warn на сырых hex/oklch/legacy `--color-*` вне whitelist.
- [x] `CLAUDE.md` § "Design Gate" — обязательный шаг в PDCA для любой UI правки.

### Page migrations

- [x] **App shell** (`apps/desktop/src/App.tsx`) — topnav → app-shell + app-rail, ThemeProvider wrap, pipeline indicator перенесён в rail foot.
- [x] **HomePage** (`apps/desktop/src/pages/HomePage.tsx`) — `.rec-btn` round (108px), `.display` heading, `.stat-row`, consent в `.modal-backdrop` + `.index-card`. Хоткей ⌘⇧R + consent gate + updater сохранены 1-в-1.
- [x] **SettingsPage** — добавлена секция "Внешний вид" (`AppearanceSection.tsx`) с theme + accent picker через `useTheme()`. Эмодзи убраны из section titles.
- [x] **CallsPage** — date-grouped serif-list (Сегодня / Вчера / На неделе / месяц) per MIGRATION.md §3 + virtualization (react-window при ≥200 строках).
- [x] **CallDetailPage** — header chrome через `.btn--quiet` back, `.small-caps` meta line (weekday · date · time · duration), `.title` (LLM-generated title) + ParticipantsRow с `.sp`/`.sp-avatar` chips, `.tabs`/`.tab` классы. InteractiveTranscript на `.transcript-row` структуру с active-row highlight (V3.x) + inline «? кто это» chip + SpeakerConfirmModal (V4.1).
- [x] **SpeakersSection** — calling-card flow per MIGRATION.md §5: `.index-card` per speaker, sample bubble с `MiniWave` + real `▶ N сек` playback из mic.wav/system.wav (V4.1), suggestion row с `.conf` bar, footer actions. SpeakerCard вынесен в `components/SpeakerCard.tsx` — переиспользуется в SpeakerConfirmModal.
- [x] **ContactsPage** — two-column list + detail с voice samples table per MIGRATION.md §6. Click-to-edit на имени, ContactForm с identifiers+attributes+consent_voice.
- [x] **OnboardingPage** — 3-step (welcome → permissions → consent+name) centred `.display`+`.input` flow per MIGRATION.md §8. focus-trap + step dots indicator.
- [x] **Coachmarks** — переписаны с новыми токенами, 4-step overlay, keyboard nav + reduced-motion.
- [x] **DesignSystemPage** (dev-only) — переписан с inline-styles по полному tokens.css + wotold.css showcase.

### Cleanup (после migration page-by-page)

- [x] Аудит `apps/desktop/src/ui/{Button,Card,Badge,Empty,Pill,StatusDot,Field,Tabs,Toolbar,UsageBar}.tsx` — превращены в thin wrappers над `.btn`/`.card`/`.dot`/`.input`/etc + token-driven inline styles.
- [x] Удалить `apps/desktop/src/styles/pages.css` (29 KB legacy classes больше не используются — все migrated в inline / wotold.css).
- [x] Удалить `apps/desktop/src/styles/legacy-tokens.css` shim — ноль `--color-*` references в JSX.
- [x] `global.css` сокращён до markdown + selection + macOS traffic-lights padding (всё через новые токены).
- [x] `ui/ui.css` сокращён до Skeleton shimmer (единственное что не покрывается wotold.css).
- [x] Sections migrated: `AccountSection`, `ByoKeysSection`, `PermissionsSection`, `VoiceSamplesSection`, `UsageSection`, `CallAudioPlayer`, `SettingsPage` (SettingsSection + RadioOption helpers).
- [x] `DesignSystemPage` переписан с inline-styles по новому token set.

### A11y / Security / Polish (B17 P3.1 — P3.4)

- [x] Code-review HIGH: HomePage recent-row border ordering + consent modal `role=dialog` / `aria-modal` / `aria-labelledby`.
- [x] Security-review HIGH: AccountSection `openExternal(authorizeUrl)` https-scheme guard (если прокси compromised — javascript:/file:/custom-scheme exploit заблокирован).
- [x] A11y modal focus trap: new `useFocusTrap` hook + applied to consent (HomePage), Coachmarks, OnboardingPage. ESC + Tab cycling + scroll lock + focus restore.
- [x] A11y CRITICAL: bumped `--signal` light #DC2626 → #BF1C1C (был 4.40:1, теперь ≈ 5.6:1 на --bg).
- [x] A11y HIGH: bumped `--muted` #6B6C72 → #5E5F65 (для text-xs/small-caps безопасно).
- [x] A11y HIGH: Tabs aria-controls / aria-labelledby pairs + id'd via useId().
- [x] A11y HIGH: App nav убран orphan role=tab/aria-selected — заменён на aria-current="page" (правильный nav pattern).
- [x] A11y HIGH: Onboarding name label htmlFor + SpeakersSection picker label htmlFor.
- [x] A11y HIGH: dynamic error `<p>` элементы получили `role="alert"` (HomePage, Settings×2, Account, Calls, CallDetail, Contacts×2, Onboarding, VoiceSamples, ByoKeys, Speakers, Permissions).
- [x] A11y MEDIUM/WARN: `prefers-reduced-motion` CSS — `.dot--pulse`, `.rec-btn`, `.conf-fill`; JS — `UsageBar` width transition через `useReducedMotion` hook.
- [x] A11y WARN: ContactsPage name button hit area padding/margin для SC 2.5.8.
- [x] `useFocusTrap` test suite (8 cases) — initial focus, ESC, Tab/Shift+Tab cycling, inactive, scroll-lock.
- [ ] **Manual visual QA** — пройти руками 6 theme×accent комбинаций (light/dark × bordeaux/persian/ink) на всех экранах. Делается перед публичным релизом, не разработка.

## Wotold v2 Redesign (B18)

> **Контекст**: 2026-06-28 получен новый дизайн-прототип **«Wotold v2»** (`~/Downloads/Wotold v2/`: `uikit.css` + `uikit.jsx` + `wk-*.jsx`, входная точка `Wotold v2.html`). Это **смена поколения ДС поверх Atelier v2 (B17)**, не рестайл: Hanken Grotesk + IBM Plex Mono (**без serif**), новый surface/density-токенсет, складной Notion-rail + icon-only minirail, глобальный recording **dock** + floating **widget**, **⌘K command palette**, новые поверхности **Inbox / Views / Explore**, call-detail с right-rail и **Assistant**-табом. Прототип — единственный источник истины: своего handoff-дока у wk-дизайна нет (пакетный `CLAUDE.md` и `design_handoff_wotold_atelier/` описывают СТАРЫЙ Atelier — устарели). Анализ: `scratchpad/v2-analysis.md`. Миграция итерациями ниже, открытые развилки помечены **⚠️** (решаются перед стартом фазы). Паспорт > мок (W6); R1–R13 не «чинить».
>
> **Решения (2026-06-28, владелец)**: акцент = **моно-графит (ink)**, picker убран, QA = light/dark; density = **фикс cozy** (без переключателя); **Home удаляется полностью** (главная не нужна); recording = **новый dock + widget** (суть та же, исполнение из прототипа, логика записи/паузы/стопа сохраняется 1-в-1); все экраны — **ровно по новому дизайну** (Inbox / Views / Explore / filters в scope B18); **Assistant-таб отложен** отдельной доработкой (вернёмся позже).

### B18.0 · Foundation — токены, шрифты, ДС-слой

- [x] Перенесены токены `uikit.css` → `apps/desktop/src/styles/tokens.css`: surface-набор `--bg/--sunken/--panel/--raised/--hover/--active`, текст `--text/-2/-3/-faint`, бордеры `--border/-2/-strong`, `--t-11..28`, `--s1..9`, `--r-xs..pill`, `--fast/base/slow + --ease`, speaker `--sp1..5`, `--rail-w`, shadows. Light + dark.
- [x] Шрифты: Hanken Grotesk + IBM Plex Mono (`fonts.css`), убраны Source Serif 4 + DM Sans + JetBrains Mono. Google CDN (self-host — TODO B18.x в комментарии fonts.css).
- [x] Component-классы `uikit.css` → новый `apps/desktop/src/styles/wk.css` (импорт ПОСЛЕ wotold.css; на коллизиях примитивов побеждает uikit). Полное удаление atelier-классов из `wotold.css` — в B18.6.
- [x] Legacy-shim `apps/desktop/src/styles/legacy-tokens.css`: `--space-*`→`--sN`, `--ink*`→`--text*`, `--line*`→`--border*`, `--signal`→`--danger`, `--font-serif`→`--font` и т.д. Удалить в B18.6.
- [x] **Акцент = моно-графит (ink)**: tokens.css один accent-набор (графит), `useTheme` `DEFAULT_ACCENT='ink'` + `data-accent` no-op. accent-picker из Settings убрать — в B18.5. QA = light/dark.
- [x] **Density = фикс cozy**: `useTheme` ставит `data-density="cozy"` на `<html>`; compact-токены в wk.css не активны.
- [x] Icon-set `uikit-icons.jsx` → `src/ui/Icon.tsx` (62 line-иконки, 1.5px, `currentColor`, без emoji) + экспорт из `ui/index.ts`.
- [x] **design-gate** обновлён: whitelist `wk.css` + `docs/design/wotold-v2/` в `scripts/hooks/design-gate.mjs`, сообщения на uikit-канон; `CLAUDE.md` §Design Gate переписан; `docs/design/wotold-v2/README.md` (atelier-v2 = legacy).

### B18.1 · App shell + IA (складной rail, dock, palette)

- [x] **B18.1a** `App.tsx`: `.app-rail` → **Sidebar (256px) + MiniRail (56px)**, `⌘\` collapse, resize 216–380px + localStorage, авто-collapse <198px (`components/AppSidebar.tsx`).
- [x] **B18.1b** Recording: `RecStrip` → **RecDock** (footer `.composer-dock`, fixed, offset на rail) с audio-reactive RecEq + pip-minimize → floating widget. _RecFloat (отдельное окно) pill-polish отложен — функционален + перекрашен shim'ом._ Логика записи/паузы/стопа/Rust-событий 1-в-1.
- [x] **B18.1c** **Command palette (⌘K)** — `components/CommandPalette.tsx`: действия (record/inbox/contacts/settings) + поиск звонков, ↑↓/Enter/Esc, focus-trap; ⌘K-строка в Sidebar + command-иконка в MiniRail.
- [x] **B18.1a** **Home удалена полностью**: default = Inbox (interim CallsPage). consent-gate (C1/R2) + hotkeys (⌘⇧R/⌘⇧P) + updater подняты в App-level; `HomePage.tsx` + test удалены.
- [x] **B18.1a** Recent-calls list в rail (Sidebar, `listCalls` + refetch на `pipeline:finished`).

### B18.2 · Inbox (замена Home + Calls)

- [ ] `pages/InboxView.tsx` — unified список: **omni-bar** (текст-поиск + facet-токены), **facet-фильтры** (статус/recap/контакт/период), **view switcher** (list/cards/week/month), month-grouping sticky, virtualization ≥200.
- [ ] `CallRow`: status-dot / title (+recap sparkle, status-chip) / participants (AvatarGroup) / duration (mono) / date / ⋯-меню (open/reprocess/export/delete) + engine-chip в строку.
- [ ] Переиспользовать `listCalls` + фильтры + `callState`; данные уже есть (title/status/started_at/parts/processing_via/recap).
- [ ] **Views (saved smart-collections) + Explore** ✅в scope (ровно по прототипу) — персистентность `SavedView` (локальный storage / S2 контракт `{label, filter_state, view_mode, sort}`).
- [ ] Stats (4 hero + sparkline) — разместить как в прототипе (`wk-screens`/`wk-inbox`).

### B18.3 · Call detail (two-column + right rail + tabs)

- [ ] `CallDetailPage` → **doc-column + CallRail** (properties / participants / actions). Tabs **Transcript / Recap / Assistant**.
- [ ] Transcript: `.turn` speaker-turns (avatar + name + time clickable + text), seek-sync с player-scrubber внизу.
- [ ] Recap: summary + **DecisionsBlock + OpenQuestionsBlock + ActionItemsV2** (category-icon commitment/proposal/idea + evidence-quote), rich↔raw toggle. Маппинг на реальные `listCallDecisions`/`listCallOpenQuestions`/`ActionItemV2` (уже в контрактах M14 — переиспользовать).
- [ ] `SpeakerRow` dropdown assign (search contacts / assign / reset); неподтверждённые = «Говорящий» (серый avatar). **Сохранить consent / biometric R2 логику 1-в-1.**
- [ ] **Assistant-таб ОТЛОЖЕН** ⏸️ отдельной доработкой (вернёмся позже): в B18.3 таб либо скрыт, либо «coming soon»-stub. Backend (LLM Q&A над транскриптом) — вне scope B18. Tabs пока Transcript / Recap.

### B18.4 · Contacts

- [ ] `ContactsView` — 2-pane (list + detail), reskin под uikit. Показать `identifiers` (email/phone chips — gap #1: мок их не показывал). Derivations (calls-count / last / confirmed) — агрегат из истории (gap #2, новый query). Voice samples — по факту реализации (gap #7).

### B18.5 · Settings (Row-layout, 9 секций)

- [ ] `SettingsPage` → rail 300px, **Row-primitive** (label слева / контрол справа). **Сохранить ВСЮ логику / SETTINGS_KEYS / i18n 1-в-1.**
- [ ] Appearance: theme → **Segmented** (sun/moon/none); **accent-picker удалён** (моно-графит); language Select. Без density-контрола (фикс cozy).
- [ ] Processing: **OptionCard** engine×2 + preset×3 (quality-dots), models **Table**, hw-probe activity-strip + recompute, cloud-usage **quota-bars** (минуты+токены), BYO-keys в `<details>`.
- [ ] Permissions: 3 dense Row. Recording: Select / HotkeyCapture / **Switch** (checkbox→Switch). Speakers: Panel-card + Switch. Labs: 3 Switch. Maintenance: Row + Wave. Privacy: Row + **HTML Modal** (по прототипу; native `ask()` заменяется).

### B18.6 · DS page + cleanup + QA

- [ ] `DesignSystemPage` переписать под wk-uikit — единый источник всех примитивов / токенов (зеркало `wk-designsystem.jsx`).
- [ ] `src/ui/*` примитивы → thin-wrappers над новыми классами: Btn / IconBtn / Chip / MetricChip / Input / Select / Switch / Segmented / Modal / Panel / Avatar / Dot / Kbd / Wave / Progress / Tabs / Dropdown / NavItem.
- [ ] Удалить legacy-shim, старые atelier-классы, неиспользуемый CSS (`pages.css`/устаревшие).
- [ ] Тесты RTL: обновить под новый DOM (role/label-queries, не классы). Сохранить green core-flow тесты (consent / hotkey / pipeline-sync).
- [ ] **⚠️ Manual visual QA**: light/dark × (1 или 3 акцента) × (cozy/compact) на всех экранах. A11y: focus-trap (palette / modal / widget), keyboard-nav, ARIA, `prefers-reduced-motion`.

### Контракты / backend (S2) — параллельно, по мере scope

- [ ] `SavedView` persistence ✅в scope (Views = B18.2).
- [ ] Assistant Q&A endpoint + prompt ⏸️ отложено (Assistant вне scope B18).
- [ ] Contact derivations query (calls / last / confirmed) для B18.4.
- [ ] Свериться: `ActionItemV2` / `Decisions` / `OpenQuestions` уже в `packages/contracts` (M14) — переиспользовать, не дублировать (S2).

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
- [~] **device-id spoof + IP rate-limit** — частично: **/16 IP rate-limit middleware** (`middleware/ip-rate-limit.ts` + `enforceIp16RateLimit` wired on `/v1/*` в `index.ts`). `cf-connecting-ip` → `ip16Prefix` (v4: первые 2 октета; v6: первый hex-блок). KV-counter `rl:ip16:{prefix}:{minute_bucket}`, лимит 120 req/min/16 default. 429 `rate_limited` при превышении; `/` + `/health` исключены. 8 unit + 3 workers integration теста (включая IPv4 / IPv6 / compressed / malformed edge cases). **Остаётся (B3.7-style scaffold для HMAC)**: HMAC-bind device-id с server-side secret при первом контакте — это контракт-change для клиента (хранить bound-token), вынесено в отдельный пункт.
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
- [x] **Export markdown** для recap/transcript из CallDetailPage. Tauri command `export_call_markdown(call_id, dest_path)` композирует metadata header (title + дата + длительность + провайдер + язык) + recap.md + transcript.md в один `.md` файл. Frontend кнопка «↓ Скачать .md» в kebab меню CallDetailPage → `save()` save-dialog → invoke. Расширение `.md` валидируется backend'ом.
- [x] **CSS responsive breakpoints** — @media (max-width: 760px) topnav-label hide + call-row 2-row + app padding; (max-width: 560px) home-stats 1col + tabs wrap.
- [x] **Recording level meter (B14)** — Swift sidecar RMS → frontend canvas waveform (см. секцию backlog выше).

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

- [x] **Zod schemas в proxy boundary** — `services/proxy/src/lib/schemas.ts` с `llmRequestSchema` / `sttStagingUrlRequestSchema` / `sttRequestSchema` / `authStartRequestSchema` + `parseBody<T>(request, schema)` helper. Заменили hand-rolled `typeof body.X !== 'string'` в /v1/llm, /v1/stt, /v1/stt/staging-url, /v1/auth/:provider/start. Zod issue (с path) идёт в `message` поля envelope'а. Fix как бонус: integration auth tests раньше падали с fake `'dev-1'` deviceId (UUID regex регрессия от 0d79a22) + 500 на callback (отсутствие `iss/aud/exp` claims после ID-token hardening) — обновлены UUID + buildIdToken теперь инжектит дефолтные claims. 27 workers integration + 91 unit = 118 proxy тестов проходят.
- [x] **Hand-rolled Promise.all → Promise.allSettled** в `CallDetailPage` — критична только call meta, остальные artifacts soft-fail с console.warn.
- [x] **`as 400 | 502 | 503` type cast в llm.ts** — заменён explicit whitelist.
- [x] **`.catch(() => {})` silent ignores** в HomePage — заменены на console.warn.
- [x] **Wide `#[allow(dead_code, unused_imports)]`** — surgical allows только на #25 voice-matching scaffold (embeddings/matching/identify/etc), точечные allows на NotImplemented variants. Cargo check: 0 warnings.
- [x] **Cargo.toml `[lints]`** — unsafe_code = forbid, clippy::unwrap_used/expect_used/panic = warn.
- [x] **Split db/calls.rs** — сделан: файл разбит на `db/calls/{lifecycle,speakers,clusters,mod}.rs`.
- [x] **Extract managed_stt_request helper** — `proxy_managed::transcribe_via_proxy` устраняет ~95 строк дубликации в soniox.rs/gladia.rs.
- [x] **audio_io::extract_segments_batch** — single WAV open + slice. Будет использоваться в #25 ONNX wire-up. +2 теста.
- [x] **Soniox text concat без пробелов** — needsSpaceBefore() вставляет пробел между letter-bordered tokens (anti-склейка ru/kk).
- [x] **LIKE wildcards escape в MCP db.ts** — escapeLikePattern() + `ESCAPE '\\'` в SQL.
- [x] **PRAGMA busy_timeout** — `busy_timeout(5s)` в db/mod.rs `init()` connect options.
- [x] **EMBEDDING_DIM в schema** — migration 0005 + backfill из length(embedding)/4. Insert-time validation в Rust — follow-up.
- [x] **partner stderr no leak в proxy logs** — scrubProviderError() в routes/stt.ts: UUID/r2-key/Bearer/sk-/gl_ tokens замаскированы.
- [x] **LLM upstream error generic для клиента** — anthropic/groq возвращают 'LLM upstream error (status)' без upstream body.
- [x] **call_fts virtual table** — dropped migration 0006 (никогда не populated, FTS5 follow-up в #30).

### Logic / Code Quality (P2)

- [x] **CallsPage listen pipeline:finished** — уже scoped в CallsPage useEffect, unlisten на unmount.
- [x] **Wrap JSON.parse(rawSttJson) в runtime validator** — parseRawStt() в InteractiveTranscript filter'ит невалидные segments (без zod dep).
- [x] **`let _ = &call`** — заменён на `let _call =` (underscore prefix подавляет dead-code idiomatic).
- [x] **NaN guard в merge_tracks sort** — segments с NaN start dropped + log::warn.
- [x] **chunk.try_into() → manual array** — `[chunk[0], chunk[1], chunk[2], chunk[3]]` zero-cost vs runtime check.

### Tests (P1)

- [x] **voice_samples cascade test** — `delete_contact_cascades_voice_samples` + `delete_call_sets_source_call_null_keeps_sample` (db/voice_samples.rs). Подтверждают что `ON DELETE CASCADE` на `voice_samples.contact_id` (миграция 0001) действительно срабатывает в SQLite + `ON DELETE SET NULL` на `voice_samples.source_call` (миграция 0003) сохраняет семпл при удалении звонка.
- [x] **delete_call_and_samples** — 3 sqlx-теста в `db/calls.rs::tests`: `delete_call_removes_row_and_voice_samples` (cascade на voice_samples с source_call=id), `delete_call_handles_missing_id_silently` (idempotent), `delete_call_cascades_action_items_and_speakers` (ON DELETE CASCADE на action_items + call_speakers по migration 0001).
- [ ] **pipeline::run/reprocess_call/regenerate_recap** — нет unit тестов. Cover happy + missing audio + recap fail.
- [x] **STT KV-resume happy path** integration test — 2 workers-теста в `stt.integration.test.ts`: (1) resume branch — pre-seeded KV cache `stt_job:soniox:{r2Key}` → mock fetch с invariant `POST /transcriptions` НЕ вызывается, после completion кэш очищен; (2) no-cache branch — POST /transcriptions создаёт job, jobId сохраняется в KV. Dummy R2 + SONIOX_API_KEY creds в wrangler.test.toml (presign без сетевого вызова, partner fetch мокается).
- [x] **OIDC ID-token signature negative tests** — добавлены 12 negative/edge тестов в `providers.test.ts::decodeIdTokenPayload`: invalid JSON в payload, non-base64 chars, empty iss, case-sensitive iss compare, aud=[], exp=0/boundary/non-numeric, missing aud/exp acceptance (Apple semantics), и **known-gap test** документирующий что tampered payload пока принимается (JWKS verification — следующая итерация). Когда JWKS landed → этот тест перевернётся на `.toThrow(/signature/i)`. 32 теста проходят (было 20).
- [x] **MCP prompt-injection content** — pass-through test (M8.3). `services/mcp/src/tools.test.ts` +2 vitest'а: `get_transcript` возвращает «SYSTEM: Ignore all previous instructions» + HTML-comment injection as-is, `search_calls('%')` → 0 совпадений (LIKE escape regression).

---

## M12 · Локальный движок (Local Engine)

> Источник истины: `M12_LOCAL_ENGINE_PRD.md` v0.2 (hardware-probe-driven onboarding; ведётся out-of-tree, в репо отсутствует). Аддендум к паспорту. PDCA по [`CLAUDE.md`](../CLAUDE.md) §«Воркфлоу для фича-тасок».
>
> Назначение: третий путь — `local` engine на sherpa-onnx Whisper + sortformer + llama.cpp. Free навсегда, без сети, без $/user. Cloud-managed становится Pro.

### M12.4 Model Catalog scaffold (PRD §9 step 1)

- [x] **Rust scaffolding** — модуль [apps/desktop/src-tauri/src/local_engine/{mod,models,preset}.rs](apps/desktop/src-tauri/src/local_engine/) (macOS-only `#[cfg(target_os = "macos")]`). `MODEL_CATALOG` на 6 entries (whisper-small/medium/large-v3, gemma3-2b, qwen25-3b/7b). SHA256 = placeholder `TODO_SHA256` (64 нуля) — by design гейт до PRD §14 pre-flight.
- [x] **Tauri commands** — `local_engine_list_catalog`, `_model_status/_download/_delete`, `_get/set_active_preset`. Atomic `.partial → final` после SHA256. Events `model:progress`, `model:done`. Идемпотентность всех операций.
- [x] **TDD tests** — 13 unit-тестов в [local_engine/models.rs](apps/desktop/src-tauri/src/local_engine/models.rs) + [preset.rs](apps/desktop/src-tauri/src/local_engine/preset.rs).
- [x] **Migration prep** — `SETTING_ACTIVE_PRESET = 'local_engine.active_preset'` константа. `local_engine.active` через M12.6.
- [x] **Security M-2/M-3 baseline** — `write_user_only` создаёт prompt-файл с `O_EXCL` + `0o600` (защита от race + чужого чтения в shared /tmp). Whisper output JSON `chmod 0o600` после whisper-cli exit. `ensure_path_under` блокирует `..` сегменты + проверяет prefix перед каждым sidecar spawn. Capability validator теперь явно помечен как «последняя граница» в capabilities/default.json.
- [x] **Security M-4 refresh-script guard** — `WOTOLD_CATALOG_REFRESH_CONFIRMED=1` ENV-gate против случайного запуска + блок ⚠️ SECURITY в шапке скрипта с cross-check workflow.
- [ ] **`/security-scan`** на `local_engine/{models,llm,stt}.rs` + `capabilities/default.json` + `scripts/refresh-model-catalog.sh` — обязателен перед production release (W5). CLAUDE.md security-triggers table обновлён.
- [x] **Pre-flight gate (PRD §14)** — `scripts/refresh-model-catalog.sh` написан + прогнан: реальные SHA256 + размеры получены для 6 моделей (Whisper small/medium/large-v3 у ggerganov + Qwen 2.5 1.5B/3B/7B у bartowski). **Gemma 3 2B заменён на Qwen 2.5 1.5B** для Light preset из-за Google TOS gating (PRD §11 O1 deviation, документировано).

### M12.1 LocalWhisperProvider (PRD §9 step 2)

- [x] **Whisper provider real** — [local_engine/stt.rs](apps/desktop/src-tauri/src/local_engine/stt.rs) спавнит `wotold-whisper` sidecar (whisper.cpp `whisper-cli`). sherpa-onnx Whisper отклонён — несовместим с ggerganov .bin форматом каталога. Sidecar получает `-m <model.bin> -f <audio.wav> --output-json-full -of <stem> -l <lang>`, парсит `<stem>.json`. Per-track speaker tagging (mic → `speaker:owner`, system → `speaker:0`). 12 tests (lang normalize, JSON parse, mic vs system tagging, sort, NaN guard).
- [x] **Sidecar binary register** — `wotold-whisper` добавлен в `tauri.conf.json::externalBin` + `capabilities/default.json::shell:allow-execute` с args validator'ами. Placeholder бинарь + build инструкции в [binaries/README.md](apps/desktop/src-tauri/binaries/README.md).
- [ ] **Acceptance integration test** — bundled WAV (RU+2 спикера) → snapshot DiarizedTranscript. Требует реального `whisper-cli` бинаря в `binaries/`.

### M12.2 LocalDiarizer (PRD §9 step 3)

- [x] **Diarizer trait** — [local_engine/diarization.rs](apps/desktop/src-tauri/src/local_engine/diarization.rs) с `SortformerDiarizer` stub. `MAX_LOCAL_SPEAKERS=4`, `SPEAKER_UNKNOWN` для excess. `apply_speaker_cap` pure-фн с 4 unit-тестами.
- [x] **Merge timestamps** — [local_engine/merge.rs](apps/desktop/src-tauri/src/local_engine/merge.rs) whisperX-style overlap, owner-bind, NaN guard, sort. 7 unit-тестов.
- [x] **Owner bind** — `force_owner_track` фиксирует mic-track в `SPEAKER_OWNER` (M3.7).
- [x] **Sortformer real wire-up (M12-D5)** — pyannote-segmentation-3-0 (~6 MB) добавлен в MODEL_CATALOG как `ModelKind::Diarization`. WeSpeaker (voice_model.rs B3.7c reuse) — embedding. `SortformerDiarizer::diarize_real` за `#[cfg(feature = "voice-onnx")]`: `Wave::read` → `OfflineSpeakerDiarization::process` через `tokio::task::spawn_blocking` → cap=4 → `SpeakerSegment`. `pipeline::diarize_system_track` non-fatal helper: при отсутствии моделей / voice-onnx off → degraded fall back (system track остаётся `speaker:0`). Auto-download pyannote добавлен в OnboardingEngineStep + Settings preset switch. Без real Inference в тестах: `diarize_real_fails_on_missing_segmentation_model`.

### M12.3 LocalLlamaProvider (PRD §9 step 4)

- [x] **O2 решено: sidecar** (`wotold-llama` через tauri-plugin-shell). PRD §11 O2 default подтверждён.
- [x] **Real LlmProvider** — [local_engine/llm.rs](apps/desktop/src-tauri/src/local_engine/llm.rs) `generate()` спавнит `llama-cli` sidecar с prompt-файлом, парсит первый сбалансированный JSON-объект из stdout, валидирует `title/summary`. 5min timeout с kill-on-drop. 15 tests (build_prompt, extract_json brace counting, escape handling, validate shape).
- [x] **Sidecar binary register** — `wotold-llama` в externalBin + capability whitelist со строгими args validators. Placeholder + build instructions в [binaries/README.md](apps/desktop/src-tauri/binaries/README.md).
- [x] **Prompt** — `LOCAL_LLM_SYSTEM_PROMPT` (PRD §M12.3.3 «only JSON» + few-shot ru пример) + 2 regression-теста.

### M12.6 Pipeline integration (PRD §9 step 5)

- [x] **Migration 0011** — [migrations/0011_local_engine_active.sql](apps/desktop/src-tauri/migrations/0011_local_engine_active.sql) backfill из `provider_path` (managed→cloud_managed, byo→cloud_byo, иначе local).
- [x] **EngineKind enum** — [local_engine/engine.rs](apps/desktop/src-tauri/src/local_engine/engine.rs) с `load_or_default` + `save` + legacy mapping. 5 unit-тестов.
- [x] **Selector Tauri commands** — `local_engine_get/set_active_engine`.
- [x] **`pipeline::run` Phase 3 — Local route real** — `run_local_inner` ([pipeline/mod.rs](apps/desktop/src-tauri/src/pipeline/mod.rs)): resolve preset → проверка моделей → STT (mic+system параллельно через whisper-cli sidecar) → merge artifacts → recognize speakers (existing B3.x) → recap через llama-cli sidecar → persist via shared `recap::persist_recap_from_json` helper → `touch_usage` для UI last_used. Cloud route не тронут. Контракт ошибок: `local_engine_model_missing`, `local_engine_stt_failed`, `local_engine_llm_failed`, `local_whisper_timeout`, `local_llm_timeout`, `local_engine_no_app_handle`, `local_engine_preset_not_set` (PRD §M12.6.5 UI fallback markers).
- [x] **recap::persist_recap_from_json extracted** — общий helper для cloud (`recap::run`) и local (`run_local_inner`); один post-processing pipeline (action_items + title + recap.md).
- [x] **Tests** — `pipeline_run_requires_app_handle_for_local_engine` валидирует precondition.
- [ ] **Cancellation flow** — SIGTERM на sidecar при call delete during processing. `tauri_plugin_shell::Child::kill()` интеграция приходит со spawn-handle tracking (B16 P0 расширение).

### M12.7 Hardware probe (PRD §9 step 6)

- [x] **`probe_hardware()`** — [local_engine/hw_probe.rs](apps/desktop/src-tauri/src/local_engine/hw_probe.rs) через `sysctl` (machdep.cpu.brand_string, hw.memsize, hw.optional.arm64). HwReport кеш в `local_engine.hw_report`.
- [x] **`recommend_preset()`** — pure-фн с 5 правилами PRD §M12.7.2. 7 unit-тестов.
- [x] **Wire-format match** — `HwArch::X8664` сериализуется как `"x86_64"` (regression test).
- [x] **Tauri command** — `local_engine_hw_probe(force?)` с кешем в settings.

### M12.5 Settings UI «Движок распознавания» (PRD §9 step 7)

- [x] **Design Gate alignment** — выдан перед .tsx (Surface / Reference / Tokens / Classes / A11y).
- [x] **Engine picker** — 3 radio-карточки (Local · Cloud · BYO) с `●●○ / ●●●` quality badges + i18n. Atelier v2 tokens only.
- [x] **Preset picker (когда Local)** — Light/Balanced/Quality с `.dot--{success|accent|muted}` статусом, GB-размер.
- [x] **Storage management modal (M12.4.4-bis)** — таблица catalog с name · size · last_used_at · active badge · × delete. Confirm-modal жёстче при удалении активной модели (PRD §M12.5.4). Migration 0012 `local_engine_model_usage` для last_used_at tracking.
- [x] **Probe summary block (M12.5.2.5)** — `.subtle` строка «CPU · RAM · Metal — рекомендуем preset» + `.btn--quiet` «Переоценить» (форс probe).
- [x] **Hardware probe banner** — `.activity-strip` с Apply/Dismiss.
- [x] **Quality confirm** — `ask()` на RAM < 16 GB (PRD §M12.5.4).
- [x] **i18n ru/kk/en** — `localEngine.*` namespace, no jargon.
- [ ] **6 theme×accent manual QA** — visual verification (PRD §M12.5.6 acceptance).

### M12 onboarding + i18n + docs (PRD §9 steps 8-10)

- [x] **Onboarding step 1** — `feature4: 'Локально на устройстве, бесплатно, без сети'` (ru/kk/en).
- [x] **i18n ru/kk/en** — `localEngine.*` + `settings.{sectionEngine,engineTitle,engineLede}`.
- [x] **`docs/PRIVACY.md`** — v0.2: local-first TL;DR + per-engine таблица + section «При Local-движке».
- [x] **README user-facing** — local-first pitch в начале + раздел «Чем Wotold отличается» + per-engine privacy таблица.
- [x] **M12.7.3 Onboarding step «Engine setup»** — новый 4-й шаг для macOS-юзеров между Owner и Permissions+Consent. Probe-карта + 3 кнопки (download / choose another / use cloud) + download progress + cancel handling. Non-macOS пропускает (R9). [OnboardingEngineStep.tsx](apps/desktop/src/pages/OnboardingEngineStep.tsx) + расширение [OnboardingPage.tsx](apps/desktop/src/pages/OnboardingPage.tsx) до 4 шагов на macOS.
- [x] **M12.7.5 Existing-users announcement banner** — `.activity-strip` в [HomePage.tsx](apps/desktop/src/pages/HomePage.tsx) для users с ≥1 ready call. Open → SettingsPage; Dismiss → persist `local_engine_announcement_seen=1`. i18n ru/kk/en.

### M12 чек-лист «можно стартовать» (PRD §14)

- [ ] sherpa-onnx version с Whisper + sortformer проверен (changelog crate).
- [ ] O2 решён: crate vs sidecar для llama (предпочтительно sidecar).
- [x] HuggingFace URL'ы + SHA256 для 6 моделей — `scripts/refresh-model-catalog.sh` + вставлено в `MODEL_CATALOG`. Whisper small/medium/large-v3 (ggerganov) + Qwen 2.5 1.5B/3B/7B (bartowski). Gemma deviation — Qwen 1.5B.
- [ ] CI build matrix готова к feature flag `local-engine` (macOS arm64+x86_64 only).
- [ ] PRD review'ен заказчиком; O1–O5 closed или accepted.

---

## M13 · Chunked Pipelined Transcription

> Источник истины: [`M13_CHUNKING_PRD.md`](M13_CHUNKING_PRD.md). Аддендум к паспорту (R11 переформулирована — chunked post-processing acceptable).
>
> Назначение: уменьшить воспринимаемое stop→ready время с **20-35 мин** до **~3-4 мин** на 2-часовом звонке через 10-минутные chunks с pipelining (chunk N обрабатывается параллельно с записью chunk N+1).

**Tradeoff:** ~80% UX-выигрыша от true realtime за ~30% усилий (~2 спринта vs 3-4). Качество транскрипта 99% от baseline (silence-aware cut + whisper `--prompt` context priming + global speaker re-clustering через WeSpeaker embeddings).

### Phase 1 — Silent cut + sequential pipeline (no UX surfacing)

- [x] **M13.1.1** `audio/silence_detector.rs` — RMS-buffer + поиск тишины в окне [T+9:00, T+11:00], fallback к local RMS min
- [x] **M13.1.2** Sidecar `rotate` команда — atomic flush + reopen WAV без drop'а сэмплов (AudioRecorder + ProcessTapRecorder)
- [x] **M13.1.3** `pipeline/chunk_runner.rs` — per-chunk STT (mic+system dual-track в M13.1.5d), `LocalWhisperRequest::with_prompt` для context priming; per-chunk embeddings + diarization добавлены в Phase 2 (M13.2.1)
- [x] **M13.1.4** DB schema — `call_chunks` table (migrations 0013 + 0014 для system_transcript_json + embeddings_json column)
- [x] **M13.1.5** Feature flag `chunked_pipeline=false` по умолчанию + orchestrator wired в start_recording (M13.1.5c/d) + pause-aware (own M13.2.1 sub-milestone, не путать с PRD M13.2.1 ниже)
- [ ] **M13.1.6** Smoke verify: dual-run на 30-мин фикстуре, diff transcripts ≥99% — **deferred to end** (требует real WAV)

### Phase 2 — Parallel pipelining + global speaker re-clustering

- [x] **M13.2.1** `pipeline/speaker_reclustering.rs` — agglomerative single-link cosine clustering, threshold 0.75 (tunable). Owner / unknown / empty-embedding passthrough. 11 unit-tests. Per-chunk embeddings extract'аются в `chunk_runner` через `extract_clusters` (reuse B3.3), persist в `call_chunks.embeddings_json`. Assembly применяет global remap к segments обеих дорожек.
- [x] **M13.2.2** Chunk N обрабатывается параллельно с записью N+1 — `tokio::spawn` per rotation event; drain pending JoinHandles на stop с `tokio::time::timeout(300s)` per task. **Trade-off:** prev_prompt всегда None в parallel mode (best-effort, ~1% quality drop на стыках, whisper всё равно reset'ит context).
- [x] **M13.2.3** `transcript:chunk_done` Tauri event — `events.rs` const + `ChunkDoneEvent { call_id, chunk_idx, status, segment_count }` + `EventBus::transcript_chunk_done`. Backend-only emit; frontend listener — Phase 3.
- [ ] **M13.2.4** Verification на multi-speaker фикстуре — **deferred to end** (требует bundled multi-speaker WAV)

### Phase 3 — UX surfacing + flag-on default

- [x] **M13.3.1** `components/call-state/ChunkProgressStrip.tsx` — N-chunk progress strip (mirror PipelineStrip `.proc-strip` pattern). Tauri `list_call_chunks` command + `transcript:chunk_done` event listener в `useCallDetail` для delta-патчей без полного refetch'а. 5 vitest cases.
- [x] **M13.3.2** Intermediate states — `ChunkProgressStrip` показывает «N / M» в summary + per-segment bullets (done/processing/failed/pending) в expand body. Reassurance строка «Можно закрыть окно — мы сохраним прогресс» остаётся из V6.4 ProcessingPanel. Macro % rounded `done/total*100`.
- [x] **M13.3.3** i18n ключи `chunkProgress.{label,ofN,statusDone,statusFailed,statusProcessing,statusPending}` на ru/en/kk.
- [x] **M13.3.4** Feature flag `chunked_pipeline=true` по умолчанию — `prepare_chunked_setup` теперь reads `Some("0") | Some("false")` → OFF, иначе ON (no migration, escape hatch через explicit DB write).

### M13 acceptance gates

- **Performance:** stop→ready ≤ 5 мин на Balanced 2ч (vs 20-35 сейчас); Quality preset 2ч не упирается в `LOCAL_WHISPER_TIMEOUT`
- **Quality:** transcript ≥99% bit-equivalent с full-file baseline на reference фикстуре
- **Robustness:** crash-safety (per-chunk recovery); silence-less window → fallback к local RMS min
- **UX:** 6 theme×accent проверены для ChunkProgressStrip

**Effort:** ~2 спринта (10-15 рабочих дней).

### M13 follow-ups

- [x] **Mic-track diarization (multi-voice on microphone)** — toggle в Settings → Speakers (default ON, hint о ~10-20% slowdown). Sortformer проходит и по mic для случаев когда на микрофон попадают несколько голосов (live-meeting в одной комнате). Owner-голос определяется через voice biometric match против voice_samples владельца (`owner_identify::identify_owner_speaker`); fallback на primary-speaker by duration heuristic если samples ещё не накоплены. Cross-track owner reflection через Phase 2 reclustering. M3.7 invariant сохраняется (owner всегда `OWNER_TAG`). 7 unit-тестов в `pipeline::owner_identify`. Не затрагивает cloud paths.
- [x] **Pipeline step label re-link** — i18n step1-5 синхронизированы с backend `Stage::step()` enum + переведены в present continuous. CallStateTag в PipelineStrip теперь dynamic через `labelOverride={progress.stageLabel}`.

---

## M14 · Summary v2 (type-driven, evidence-grounded)

> Источник истины: [`docs/M14_SUMMARY_V2_PRD.md`](M14_SUMMARY_V2_PRD.md) (out-of-tree копия — см. `Downloads/бфт.md`).
> 18 задач T-01..T-18 (12 P0, 4 P1, 2 P2). Foundation slice landed.
> Cloud LLM: оставляем Groq (Llama 3.3 70B); миграция на xAI Grok-4.1-Fast отложена.

### Foundation (T-01 + T-03) ✓ done
- [x] **Migration 0015** — ALTER calls (call_type, summary_schema_version, summary_engine, summary_pipeline_mode, summary_*_tokens, summary_type_specific_block) + ALTER action_items (owner_confidence, due_confidence, category, evidence_*) + NEW tables decisions / open_questions с FK на calls + 5 indices. Non-destructive.
- [x] **Rust types** ([pipeline/summary_v2.rs](../apps/desktop/src-tauri/src/pipeline/summary_v2.rs)) — `CallType` (9 variants), `ActionItemCategory`, `EvidenceAnchor`, `ActionItemV2`, `Decision`, `OpenQuestion`, `ParticipantV2`, `CallSummaryV2` + serde camelCase aliases для cloud responses. 7 unit-тестов (roundtrip + aliases + defaults).
- [x] **TS contracts** ([packages/contracts/src/summary_v2.ts](../packages/contracts/src/summary_v2.ts)) — mirror Rust types + `CALL_TYPES`, `ACTION_ITEM_CATEGORIES` constants.
- [x] **Validator** ([pipeline/summary_validator.rs](../apps/desktop/src-tauri/src/pipeline/summary_validator.rs)) — `substring_fuzzy_score` (sliding-window Levenshtein, normalize lowercase+collapse-ws), `verify_evidence_quotes` (≥ 0.9 threshold), `strip_unverified_evidence` (drop-on-fail), `validate_schema` (confidence ranges + key_points len 3..7), `dedup_items` (Jaccard token overlap ≥ 0.7). 15 unit-тестов.

### Pipeline (T-02..T-10) — partial
- [x] **T-02 cloud schema-v2 prompts + persist + extended recap.md** — recap.rs heavy rewrite: новый PRD §5.1 cloud_universal system prompt (8 call types, ABSOLUTE RULES, TYPE GUIDE с MoM sections, evidence quote rules); v2 parse с graceful v1 fallback через `promote_legacy_to_v2`; validator pass (substring fuzzy ≥ 0.9, dedup, schema warn); persist через новые DB модули (`db::decisions`, `db::open_questions`, `db::set_summary_metadata`); action_items extended с v2 fields (owner_confidence, due_confidence, category, evidence_*); recap.md теперь содержит ## Решения / ## Открытые вопросы / ## Задачи с category emoji prefix + evidence blockquotes (ru/en/kk localized via `summary.language`). Cloud LLM остаётся Groq Llama 3.3 70B / Anthropic Sonnet 4. UI остаётся legacy markdown render (T-11 deferred). 12 new unit-tests в recap.rs.
- [x] **T-04 local classifier (Phase A)** — `pipeline/classifier.rs`: lightweight LLM-pass (~256 tokens) перед main v2 generation. Output `{ call_type, confidence, language }`, parses через defensive `CallType::from_str` (unknown → `Other`). Берёт первые 6000 chars transcript (head). Best-effort: на любую ошибку orchestrator делает fallback на single-pass без hint. 8 unit-тестов (prompt structure, head extraction с UTF-8 boundary safety, response parsing).
- [x] **T-05 chunker (Phase B)** — `pipeline/chunker.rs`: разбиение transcript.md по speaker-turn boundaries (`**name** [mm:ss]:`) с overlap. Per-preset config из PRD §3.3: Light 12.8K chars chunks / 1.28K overlap / trigger >24K, Balanced 19.2K/1.92K/>38.4K, Quality 38.4K/3.84K/>76.8K. Greedy pack speaker-turns, tail overlap всегда обрезается ровно по последней speaker-header (никогда не посередине реплики). Edge cases: broken format без headers → char-boundary fallback; short transcripts → 1 chunk без overlap. 8 unit-тестов.
- [x] **T-06 map-reduce (Phase B)** — `pipeline/map_reduce.rs`: per-chunk map LLM call → JSON `{ chunk_idx, facts, decisions_candidates, action_candidates, open_questions_candidates, topic_tags, participants_mentioned }`, потом reduce call с consolidated map outputs JSON-array + known_call_type hint + known speakers → финальный `CallSummaryV2`. Resilience: failed map call (`Err` или garbage JSON) пропускается с log warn — reduce работает с остальными. Если все map fail → `AppError` ("all map calls failed"). 6 unit-тестов с trait-based `MockProvider`. Local orchestrator (`local_orchestrator.rs`) теперь dispatch'ит short/long через `chunker::needs_chunking` (5 orchestrator tests total: 3 Phase A + 2 Phase B). Phase C/D/E (T-07 expert prompts / T-08 action-item post-pass / T-09 GBNF) deferred.
- [x] **T-07 8 expert prompts (Phase C)** — `pipeline/expert_prompts.rs`: focused per-call-type prompts (9 variants — 8 specialized + `Other` fallback). Каждый expert prompt содержит shared ABSOLUTE RULES + SCHEMA blocks плюс SPECIALIZED GUIDE с MoM headers + type_specific_block schema только для одного call_type (без 8 других). Privacy-sensitive rule встроена в `one_on_one` (paraphrase evidence, no verbatim personal feedback). Dispatcher: `local_orchestrator::run_v2_pipeline` short path + `map_reduce::run_map_reduce` reduce step используют expert когда `known_call_type=Some(t)`, universal fallback на None (classifier failure → no regression). Cloud path продолжает использовать universal. 12 expert tests + 1 orchestrator dispatch + 2 map_reduce dispatch.
- [x] **T-08 action-item post-pass (Phase D)** — `pipeline/action_item_post_pass.rs`: третий LLM-call после main/reduce для refinement action_items. Re-validate category (commitment / proposal / idea), owner_confidence (0.9+ только при explicit accept), dedup identical items, drop non-verbatim evidence (LLM решает, не binary validator). Best-effort: на failure / garbage output → keep original (no regression). Skip когда action_items пустой массив. Integration в `local_orchestrator::run_v2_pipeline` after main/reduce — работает на обоих paths (single-pass + map-reduce). Cloud path skip (Groq/Anthropic качество достаточное; Phase D-bis backport — backlog). 8 post-pass tests + 2 orchestrator integration tests.
- [x] **T-09 GBNF grammar fallback (Phase E)** — `pipeline/gbnf.rs` retry wrapper для всех local LLM calls. Первая попытка БЕЗ grammar (естественный output, быстрее); на `LlmError::Provider` → retry с `--grammar-file <universal_json.gbnf>` который констрейнит output до valid JSON object (standard llama.cpp json.gbnf — outer shape). На second failure → propagate original error (no infinite loop). `LlmRequest.grammar: Option<String>` пробрасывается через `LocalLlamaProvider` (пишет в temp file mirror prompt-file pattern + `--grammar-file` arg). Capability whitelist расширен `--grammar-file` validator (same regex как `-f`). Cloud (`AnthropicProvider`) ignores field. Applied in classifier / local_orchestrator main / map_reduce (map + reduce) / action_item_post_pass. Non-Provider errors (Auth/Quota/Network/NotImplemented) НЕ ретраятся. 5 new gbnf tests + updated 4 existing tests reflect retry behavior. **Phase E завершает M14 local pipeline T-04..T-10.**
- [x] **T-10 orchestrator skeleton (Phase A)** — `pipeline/local_orchestrator.rs`: chain classifier → main v2 generation с known_call_type hint. `build_v2_system_prompt` extended optional `known_call_type: Option<CallType>` parameter (cloud callers pass None). Replaces inline `LocalLlamaProvider::generate` call в `run_local_inner`. Telemetry (T-14) теперь captures local engine runs — `flag_state=Some(s.summary_v2_enabled)` (был None). Tests с trait-based `MockProvider` (3 cases — success/classifier-fail/main-fail). LOCAL_LLM_SYSTEM_PROMPT legacy v1 constant остаётся для debugging baseline. Phase B/C/D/E (T-05..T-09) deferred.

### UI + Quality (T-11..T-15) — deferred
- [x] **T-11 UI v2** — 5 новых React компонентов в Atelier v2 design: `CallTypeBadge` (header chip с типом звонка), `DecisionsBlock` + `OpenQuestionsBlock` (структурированные takeaways над markdown в Рекап табе), `EvidenceTooltip` (hover/click sticky popover с quote из транскрипта + jump-to-moment), `PrivacyDisclaimer` (banner для 1:1 встреч). Extended `TasksPanel`: category emoji prefix (✅/💡/📝) + confidence badge при inferred owner + evidence tooltip. 2 новые Tauri commands (`list_call_decisions`, `list_call_open_questions`) + extended Call/ActionItem TS types + useCallDetail hook fetches. 24 vitest tests. Atelier v2 CSS additions (.v2-block / .decision-row / .evidence-popover / .privacy-disclaimer / .confidence-low). Legacy markdown render остаётся для schema_version=1 (контекстный narrative + backward compat).
- [x] **T-12 golden set + CI regression harness** — 10 reference cases в `pipeline/golden_summaries/` (cloud v2 sales_discovery/standup/one_on_one + legacy v1 minimal/with_actions + evidence stripping + dedup action_items/decisions + multilingual ru/en + empty arrays edge case). `pipeline/golden_eval.rs` test-only harness прогоняет каждый case через full processing pipeline (`parse_summary_v2_or_promote_legacy` → `strip_unverified_evidence` если transcript_md given → `dedup_items` → serialize → deep-diff). `parse_summary_v2_or_promote_legacy` + `promote_legacy_to_v2` → `pub(crate)` для test access (matched validator helpers visibility). f32-exact confidence values (0.5, 0.75, 0.875, 0.9375) чтобы избежать precision drift в diff. CI catches regressions deterministic'но (no LLM calls, no network). +10 tests. Total: 498 cargo + 320 vitest.
- [x] **T-13 LLM-as-judge G-Eval scoring infrastructure** — `pipeline/g_eval.rs`: cloud Sonnet/Anthropic judge оценивает summary v2 по 4 dimensions (G-Eval, Liu et al. NLP-2024): coherence, faithfulness, relevance, conciseness — каждая 1-5 integer + 2-4 sentence justification. Phase A foundation: `build_judge_prompt` (4D rubric, `lang_detected` для justification language), `extract_transcript_head` (12K chars, UTF-8 safe), `parse_eval_response` (clamp 0→1 / 6→5 defensive), `evaluate_summary(provider, transcript, summary, lang)` через `AnthropicProvider::Managed`. `EvalScores::average()` возвращает mean f32 для dashboards. 9 unit tests с trait-based MockProvider (no real LLM в CI). Backlog (M14.5): DB persistence (`summary_eval_scores` table), Tauri command + UI display, auto-eval Labs flag, multi-sample averaging, cost guards.
- [x] **T-14 summary v2 feature flag + local telemetry** — `SUMMARY_V2_ENABLED` setting (default ON) в `PipelineSettings.summary_v2_enabled`. Branch в `recap.rs::run()` между `build_v2_system_prompt` и новой `build_legacy_system_prompt` (минимальный v1 markdown-only schema). Migration 0016 + `db::telemetry::record_summary_generation` персистит local log (call_id, engine, schema_version, flag_state, generation_ms) для будущей analytics UI (M14.5) — без сетевой отправки (R8). Settings → новая «Лаборатория» секция с opt-out toggle. 9 новых tests (3 telemetry + 2 settings + 4 recap + 3 vitest LabsSection). Backlog: telemetry dashboard UI, per-call override, rollback existing v2.
- [x] **T-15 legacy v1 → v2 upgrade button** — `LegacyRecapBanner` (.activity-strip pattern) на CallDetailPage перед табами. Виден когда `summary_schema_version ∈ {1, NULL}` AND recap.md существует AND status ≠ processing. Click → переиспользует `regenerateRecap(call_id)` Tauri-команд (LLM-only, без re-STT). Backend через T-02 path (recap::run) пишет CallSummaryV2 → DB. После завершения `onRegenerateRecap` вызывает `refetchAll()` (вместо узкого setRecap+setTasks) — Call/decisions/open_questions/action_items одним батчем обновляются → banner condition false → банер исчезает, CallTypeBadge / DecisionsBlock / OpenQuestionsBlock появляются. 4 vitest tests + i18n ru/en/kk (`callDetail.legacyRecap{Title,Hint,Button,Upgrading}`).

### Backlog (T-16..T-18) — P2
- [x] **T-16 speculative decoding plumbing (P2)** — llama.cpp `--model-draft <path>` plumbing через `LocalLlamaProvider::with_draft_model(Option<PathBuf>)` builder. 0.5B draft model (Qwen 2.5 0.5B Instruct ~380MB) выдвигает draft-токены параллельно с 7B target → 2-3× speedup generation на Quality preset. Activation: (1) `summary_speculative_decoding` setting = "1" (Labs toggle, default OFF) + (2) preset == `LocalEnginePreset::Quality` + (3) draft model file exists (graceful no-op + warn log если absent). Mirror T-09 grammar-file pattern: tempfile-less (model = постоянный file), conditional arg в `generate()`. Files: `local_engine/models.rs` (QWEN25_0_5B catalog entry, SHA256 placeholder — refresh-model-catalog.sh backlog M14.6), `pipeline/settings.rs` (PipelineSettings field + load), `local_engine/llm.rs` (builder + arg), `capabilities/default.json` (--model-draft validator), `pipeline/mod.rs::run_local_inner` (orchestrator gate), `src/api/settings.ts` + `src/pages/LabsSection.tsx` (second toggle), i18n ru/en/kk (`settings.speculativeDecoding{Label,Hint}`). Backlog (M14.6): real SHA256 verification, benchmark в CI, telemetry counter, version-skew auto-disable. +5 tests (2 settings + 3 provider) + 4 vitest LabsSection. Total: 518 cargo + 321 vitest. **M14 milestone полностью завершён** (все P0/P1/P2 tasks done).
- [x] **T-17 title regen trigger** — новая кнопка «↻ Пересоздать название» в `HeaderActions` kebab menu CallDetailPage. Lightweight LLM-call (~150 max_tokens, focused prompt только на title) через cloud `AnthropicProvider::Managed` (mirror `regenerate_recap` pattern). `pipeline/title_regen.rs` модуль: `build_title_prompt`, `extract_transcript_head` (UTF-8 safe), `parse_title_response` (fallback "Без названия" на garbage/empty), `regenerate_title()` (loads PipelineSettings → managed mode → LLM → `db::set_call_title` → return new title). NEW Tauri command `regenerate_title` зарегистрирован в `invoke_handler!`. Frontend: `regenerateTitle(callId)` TS API → CallDetailPage `onRegenerateTitle` handler с `regeneratingTitle` busy flag + shared disabled state в kebab → `refetchAll` после success. i18n ru/en/kk (`callDetail.regenerateTitle{Title,ing,Failed,NoTranscript}`). Cloud-only path; local engine support — backlog M14.6. 7 backend tests.
- [x] **T-18 hierarchical 3-level pipeline (P2)** — `pipeline/map_reduce.rs` extended: `build_mid_reduce_prompt` + `run_single_mid_reduce` + новая entry point `run_pipeline` dispatcher. Когда `chunks.len() > HIERARCHICAL_THRESHOLD` (8) → 3-level: map (per chunk) → mid-reduce (per group of MID_REDUCE_GROUP_SIZE=4) → final reduce. Mid-aggregates имеют same shape как map outputs (facts / candidates / topic_tags / participants_mentioned, без CallSummaryV2 final fields). Final reduce принимает array из mid-aggregates вместо raw map outputs — solving ctx-overflow для Light preset на calls >25K tokens (~1.5h+). Best-effort: failed map/mid-reduce groups skipped; all-fail → AppError. Constants tuned для Light; backlog adaptive thresholds. local_orchestrator switched к `run_pipeline` (auto-dispatches flat/hierarchical). +6 tests. Total: 513 cargo + 320 vitest. **M14 P2 завершён** (только T-16 speculative decoding остался — backlog nice-to-have).

### Post-M14 bug-fix batch (6 user-reported)

- [x] **Bug-fix batch (post-M14 ship)** — 6 багов из user-reported QA сессии.
  - **#1 Recap regen 429** — `AnthropicProvider::generate_managed` 3-attempt exponential backoff (1s → 3s → 9s ± jitter) на transient errors. `is_retryable_message()` ловит "429"/"upstream error"/"Bad Gateway"/"rate limit"/"502/503/504". Cloudflare proxy wraps Anthropic 429 как `ok:false code:provider_error message:"LLM upstream error (429)"` — это retryable. Hard-cap `code:"quota_exceeded"` остаётся permanent (no retry). Frontend `api/errors.ts` разделил quota (hard-cap) от transient ("Сервис временно занят"). +3 backend tests + 2 vitest patterns.
  - **#2 Mic diarization silent skip** — `diarize_track` без pyannote-segmentation модели тихо no-op'ил. Лог bumped `info` → `warn` (видно в release-logs). Frontend Speakers section: mic-diarization toggle gated на `localEngineModelStatus('pyannote-segmentation') == 'present'`, иначе disabled + inline кнопка "↓ Установить модуль разделения голосов" → `localEngineModelDownload`.
  - **#3 HomePage hotkey hint hardcode** — `⌘ ⇧ R` был хардкод-строкой; lifted `toggleHotkey/pauseHotkey` в `useState<ParsedHotkey>`, JSX рендерит `{formatHotkey(toggleHotkey)}` + i18n `home.hotkeyTitle` теперь принимает `{chord}` placeholder.
  - **#4 VoiceModelSection обезличена** — "WeSpeaker ResNet34 LM · VoxCeleb" → "Модуль распознавания голоса" / "Voice recognition module" / "Дауысты тану модулі". Tech details expander (URL/SHA256/feature flag) удалён. Размер модели остался в кнопке download.
  - **#5 Realtime reactivity** — `useCallDetail` `call:progress` listener теперь триггерит debounced `refetchAll()` на каждый stage transition (`step !== prevStep`), 600ms debounce + 1.5s rate-limit. Артефакты (transcript / raw_stt / recap / tasks) подтягиваются live без exit-enter.
  - **#6 Recap regen после bind speaker** — `RecapRegenSuggestionStrip` (mirror `LegacyRecapBanner.activity-strip` pattern) появляется после успешного `confirmCallSpeaker` или `onSpeakersChanged`. Текст "Имена участников изменились — пересоздать саммари?" + кнопка/dismiss. Memory-only flag, перезапускается на следующий bind в этом же звонке.

  Files: `providers/llm/anthropic.rs` (retry loop), `pipeline/mod.rs` (warn-level logs), `api/errors.ts` + `i18n/{ru,en,kk}.ts`, `pages/HomePage.tsx`, `pages/VoiceModelSection.tsx`, `hooks/useCallDetail.ts`, `pages/CallDetailPage.tsx`, `components/call-detail/RecapRegenSuggestionStrip.tsx` (new). Tests: 521 cargo + 323 vitest (was 518/321).

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
