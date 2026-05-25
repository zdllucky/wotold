# Chunked Pipeline + Local Engine — Backlog

Single source-of-truth для deferred items вокруг chunked recording pipeline (M13.x) и
local-engine (M12.x, M14.x) tech-debt. ROADMAP.md остаётся product-level — здесь
живут только M13/M14 pipeline-specific следующие шаги.

## Закрыто (для контекста)

| Slice | Что | Коммит |
|---|---|---|
| P0.1 | Audio merger: chunks → root WAV | `28c686f` |
| P0.2 | Failed chunk retry button + `retry_chunk` Tauri command | `7b0d1ac` |
| P0.3 | Startup orphan-chunk sweep | `4169a0f` |
| P1.1 | Reprocess surface failed chunks (log::warn) | `ebdd406` |
| P1.2 | Labs «Force N speakers» + reclustering diag logs | `05a1de8` |
| P1.3 | Per-preset LLM timeout + recap:progress elapsed event | `ce8fd92` |
| P2.2 | humanError patterns для local_engine error tokens | (этот slice) |

См. `archive` секцию в `~/.claude/plans/sleepy-juggling-cocke.md` для предыдущих
циклов фиксов (recap regen 429, mic diarization 1-speaker collapse, M14 bug-batch).

## Deferred — Diarization (P1.2 follow-ups)

- **Threshold tuning 0.4 → 0.35.** Нужен golden-set из 2-3 mic recordings с
  known speaker counts. CI smoke не запускает sherpa-onnx weights (heavy), но
  локальный verify script подойдёт. Текущий 0.4 — компромисс из commit `0ece0b3`.
- **VAD config exposure** через sherpa-onnx `OfflineVoiceActivityDetector`. FFI
  research needed — не подтверждено что Rust binding поддерживает dynamic
  config параметры VAD.
- **Embeddings audit** для коротких сегментов (< 2 sec) — cosine similarity
  может быть нестабильна на shorter than threshold acoustic windows.
  WeSpeaker ResNet34 LM trained на VoxCeleb где средняя длина ~5s.
- **Per-cluster centroid distances** в `speaker_reclustering::agglomerative_cluster`
  log::debug — на каждый merge видеть `cos_dist`. P1.2 добавил input/output
  counts; this — detail polish, не критично.
- **Sortformer replacement** — switch на ECAPA-TDNN или Wespeaker v2. Heavy
  research, отдельный milestone. Текущий sherpa-onnx WeSpeaker — baseline.

## Deferred — LLM progress / UX (P1.3 follow-ups)

- **Progress percentage estimate.** Требует parse llama-cli streaming output —
  ловить `print_timings` / `n_eval / n_predict` маркеры реальном времени.
  Currently UI показывает только elapsed_sec (~15s ticks).
- **Cancel button** во время recap regen. Backend нужна структура
  `CancelToken` + propagation через `local_orchestrator::run_v2_pipeline` +
  `SidecarGuard::kill()`. Sidecar уже умеет abort'аться через drop.
- **Expected duration hint** «~5min из 10min». UX усложнение —
  preset-dependent estimate. Можно сгенерить из telemetry (median per-preset).
- **Periodic emit во время STT** (не только LLM). Helper
  `with_recap_progress_emitter` generic над future — легко переиспользовать
  на `LocalWhisperProvider::transcribe`, но событие нужно отдельное (e.g.
  `stt:progress`).

## Deferred — Audio player (P2.1)

- **Conditional badge** «Аудио недоступно до завершения обработки» когда merged
  WAV ещё processing. Сейчас плеер показывает первый chunk (10 мин) для 30+ мин
  записей до окончания merge.
- **Hint про длину** «Файл объединяется… (X из Y чанков готово)» — derived
  state из `useCallDetail`'s `chunks` array (`done` count / total).

## Deferred — Telemetry (P2.3)

- **`db/telemetry.rs` schema extension** для `chunk_failed` events:
  `(call_id, chunk_idx, reason, retried_count, created_at)`. Сейчас
  только summary v2 telemetry persisted.
- **DevSection aggregate dashboard** — «X% chunks failed last 7 days»,
  per-preset breakdown. Только для dev, не для production users.

## Deferred — Reprocess incremental

P1.1 добавил pre-flight warn для failed chunks при reprocess. Но full
reprocess всё равно делает re-STT всех чанков. Возможный smart path:

- **Reuse done chunks при reprocess** — chunk_assembly уже фильтрует
  `status='done'`, но reprocess сбрасывает все chunks к pending перед запуском.
  Если оставить done как done и только rerun failed → значительная экономия
  времени для частично-успешных записей.

## Deferred — Live duration tracking (reported 2026-05-25)

User зарепортил screenshots: запись активно идёт 31+ мин, но UI показывает
stale длительность в двух местах:

- **HomePage list:** «1:56» — = `duration_sec` из DB, который NULL / stale
  во время recording.
- **CallDetailPage player:** «21:55» — = реальная длина merged root WAV
  (только закрытые chunks, без in-progress активного).
- **Реальная запись:** 31+ мин. Никакая UI surface не отражает фактическую
  длительность.

**Root cause (confirmed):**

- [`db/calls/lifecycle.rs::finish_recording`](../apps/desktop/src-tauri/src/db/calls/lifecycle.rs)
  — единственный writer `duration_sec`, fires только на `stop_recording`.
- `audio:rotated` event несёт `duration_sec` в payload, но в DB не пишется.
- [`pipeline/audio_merger.rs`](../apps/desktop/src-tauri/src/pipeline/audio_merger.rs)
  запускается post-pipeline; root `mic.wav` не отражает активный
  незакрытый chunk.
- HomePage `listCalls()` initial fetch + нет re-fetch на `call:progress`.
- CallDetailPage `useCallAudio` fallback = stale DB `duration_sec`; реальная
  длина приходит из WAV `onDurationchange` (но без активного chunk).

**Возможные подходы (исследовать):**

1. **DB rotation update.** На каждый `audio:rotated` event в
   `commands/recording.rs` rotate handler → `update_call_duration(call_id,
   accumulated_sec)`. HomePage / CallDetailPage refresh через existing
   `call:progress` либо новый `recording:duration` event. Минимально-инвазивный.

2. **Sidecar live duration ping.** Sidecar шлёт `audio:duration_tick`
   каждые ~5s с текущим accumulated time. Backend пишет в DB + emit event.
   Reactive UI без race на rotation boundaries.

3. **UI-only fix (без DB writes).** Frontend tracks recording start
   timestamp + `Date.now() - start` для активной записи. `RecordingProvider`
   уже знает `call_id` recording session. На HomePage list для записей
   `status='recording'` показывать live counter; для остальных — DB
   `duration_sec`. Минимум backend изменений; не покрывает crash recovery.

**Acceptance.** UI везде показывает корректную длительность во время
активной recording: HomePage list, CallDetailPage player fallback, любые
другие surface'ы. Stop recording → переход на финальный `duration_sec`
seamless.

**Связано с P2.1** (audio player UI badge). Эти 2 backlog item'а вместе
закрывают «audio во время recording» UX gap.

## Deferred — SpeakerConfirmModal sample playback broken (reported 2026-05-25)

User зарепортил: на Speakers tab → «Кто этот голос?» card → «▶ Прослушать»
кнопка визуально переходит в playing state (waveform animates, кнопка
становится «■ стоп»), **но звук не воспроизводится**. Confirmed на 2
голосах из 2.

**Возможные источники (требует диагностики):**

- `SpeakerConfirmModal` либо аналогичный inline-bubble component на
  Participants tab — какой source для audio? Если использует ту же
  `getCallAudioPath(callId, 'mic')` что P3 для voice samples, должно
  работать. Если кастомный slice по start/end через что-то типа
  `get_sample_bytes` — это не существует (нет схемы поддерживающей
  audio slice).
- Возможно UI animates waveform progress независимо от actual audio
  element state — нет sync с `audio.played` events.
- Audio element src может быть устаревший / wrong call ID — проверить
  что `call_speaker.source_call_id` правильно резолвится в путь.
- `convertFileSrc` permission / capability issue на этом surface
  (CallDetailPage Participants tab) — возможно требуется отдельный
  asset scope.

**Связь с P3** (voice sample play button в Contact detail). P3 работает
через `getCallAudioPath` — должно быть копируемый паттерн. Если ломается
на ParticipantsTab/SpeakerConfirmModal, разница в:
- инициализация audio element (different ref / lifecycle);
- play context (single shared audio vs per-card audio);
- src resolution (call-level vs sample-level).

**Acceptance.** Click ▶ на «Кто этот голос?» card воспроизводит реальный
audio (mic.wav из source call). Waveform progress sync'ится с
`audio.currentTime`. Кнопка ■ стоп ставит pause.

**Files to investigate:**
- `apps/desktop/src/components/SpeakerConfirmModal.tsx`
- `apps/desktop/src/components/call-detail/ParticipantsRow.tsx` (если
  inline-bubble на этом tab)
- Любой компонент с «Кто этот голос?» текстом — `t('callDetail.whoIsThis')`
  или similar i18n key.

## Deferred — Recap failed_reason engine label mismatch (reported 2026-05-25)

User зарепортил: «НЕ УДАЛОСЬ СОЗДАТЬ САММАРИ · **ОБЛАКО (WOTOLD PROXY)**»
badge показывает cloud engine, но в **message body**: «Локальная модель
не успела ответить за 10 минут. Попробуй preset «Light»...» — текст
явно про local engine.

**Гипотеза.**

`Call.summary_engine` (badge label) и `Call.recap_failed_reason` (message
body) выставляются в разных code-path'ах + могут разъезжаться:

- `summary_engine` пишется через `recap::persist_recap_from_json` (только
  на success / partial-success). На failure path remains stale — старое
  значение из предыдущего успешного recap.
- `recap_failed_reason` пишется на каждый failure attempt (`local_engine_*`
  tokens) включая после flip с Local → Cloud (или vice versa).

**Сценарий воспроизведения (likely):**

1. User запустил recap на Local engine → upal с `local_llm_timeout`.
   → `summary_engine='local-qwen-Xb'`, `recap_failed_reason='local_llm_timeout...'`.
2. User переключился на Cloud engine в Settings.
3. User нажал «Пересоздать саммари» → cloud path запустился но тоже упал
   (например 502).
4. `recap_failed_reason` обновился на cloud error, НО он переписался
   локальным error token-ом из previous attempt? Либо: cloud handler
   вообще не запустился по race condition и UI ещё видит stale local
   reason, но `summary_engine` уже обновился до cloud preset.

**Files to investigate:**
- `apps/desktop/src-tauri/src/pipeline/mod.rs::regenerate_recap` — dispatch
  по engine; что пишется в `recap_failed_reason` + `summary_engine` per branch.
- `apps/desktop/src-tauri/src/pipeline/recap.rs::persist_recap_from_json` —
  обновляет ли `summary_engine` на failure path?
- `apps/desktop/src/components/call-detail/LegacyRecapBanner.tsx` либо
  banner-component — какие fields он рендерит (engine label vs reason)
  и не путает ли источники.

**Acceptance.** Badge engine label = engine, который реально упал. Message
body human-error matches engine. Если оба source-of-truth fields разъехались
по race — single transactional UPDATE с двумя полями вместе.

**Связано с** «Archive» секции «Step 3 — engine indicator» (commit
`db2afc7` либо similar) который добавил engine eyebrow в banner. Этот
фикс работает корректно для свежих attempts; mismatch появляется при
engine-switching между attempts.

## Deferred — Архитектурный

- **`db/calls.rs` split** — файл 791 строк (см. ROADMAP активный backlog).
  Разделить на `lifecycle.rs` (status FSM), `metadata.rs` (recap fields),
  `query.rs` (list / filter). Не блокер, но усложняет навигацию.
- **Cross-platform R9/R4.** Linux/Windows local-engine + audio capture —
  trait + `unimplemented!()` сейчас. MVP только macOS. Big chunk работы.
- **R10 model bundling.** Сейчас on-demand download. Если CI/CD scale'ится,
  bundled installer для full preset суммарно ~50MB → отдельный download
  flow без runtime fetch.
- **R12-bis storage UI** — explicit storage management при смене preset.
  Сейчас старые модели остаются на диске пока user не удалит вручную.

## Уверенно НЕ делаем

Эти items rejected by design (см. `archive` секции в plan + ПАСПОРТ §12).

- **R3 deviation auto-detect call started** — call recording всегда manual
  trigger. Опт-ин в Labs (Core Audio frontmost-app whitelist) — open.
- **R11 live realtime captions.** Local STT offline-only. Chunked 10-min
  post-processing допустим как UX optimization (P0.x slices), но НЕ live
  realtime.
- **Auto-fallback Cloud → Local** при cloud LLM fail. Risky — explicit
  user consent required для switching engines.
- **Distributed chunk processing** (multi-process). Overkill для desktop.

## Links

- [`docs/M13_CHUNKING_PRD.md`](M13_CHUNKING_PRD.md) — chunked pipeline spec
- [`docs/M14_SUMMARY_V2_PRD.md`](M14_SUMMARY_V2_PRD.md) — summary v2 spec
- [`docs/ROADMAP.md`](ROADMAP.md) — product-level decomposition
- [`docs/ПАСПОРТ_ПРОЕКТА.md`](ПАСПОРТ_ПРОЕКТА.md) — TZ + R1-R13 принятые ограничения
