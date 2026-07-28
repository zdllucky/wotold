# M13 — Chunked Pipelined Transcription (PRD)

> **Status:** feature request, approved. Implementation deferred — отдельный sprint по PDCA-flow из [CLAUDE.md](../CLAUDE.md). Аддендум к [паспорту](ПАСПОРТ_ПРОЕКТА.md) §11/§12.
>
> **Source:** утверждённое roadmap-предложение из inline chat-discussion 2026-05-22 (см. также [ROADMAP §M13](ROADMAP.md#m13--chunked-pipelined-transcription)).

## 1. Problem statement

Сейчас при `stop` записи пользователь видит «обработка…» в течение:
- **18-30 мин** на Balanced preset (Whisper Medium + Qwen 3B), 2-часовой звонок
- **50-90 мин** на Quality preset (Whisper Large v3 + Qwen 7B), 2-часовой звонок

Полный аудио-файл одноразово уходит в `whisper-cli` → `pyannote` → `llama-cli` recap. Это создаёт 4 проблемы:

1. **Психологически воспринимается как зависание** — нет промежуточных артефактов, прогресс-бара или подсказок что движется.
2. **Упирается в timeout'ы:** [`LOCAL_WHISPER_TIMEOUT = 20 мин`](../apps/desktop/src-tauri/src/local_engine/stt.rs#L49) ломает Quality preset на 2-часовых звонках → SIGKILL → `local_whisper_timeout` error.
3. **RAM peak:** Whisper Large держит mel-spectrogram всего файла в памяти. 2ч ≈ 3.5GB резерва на 16GB Mac — свопинг, замедление другого app'а.
4. **Crash-unsafe:** force-quit на 1ч 45мин записи теряет все 105 минут работы — partial transcript отсутствует.

## 2. Solution overview

Запись делится на 10-минутные chunks с **silence-aware cut** (резать на естественной паузе ±1 мин от target). После закрытия chunk'а — асинхронный pipeline (STT + diarization + per-segment embedding extraction) запускается на нём **параллельно с записью следующего chunk'а**.

После `stop` и завершения всех chunk pipelines — **global speaker re-clustering** + concat + LLM recap (одноразово на полном transcript'е).

### Tradeoff matrix

|  | Full-file (текущее) | **Chunks (M13)** | True realtime (M14?) |
|---|---|---|---|
| Лаг до первого видимого результата | 20-30 мин | **~12 мин** (после 1-го chunk'а) | 1-4 сек |
| Качество транскрипта | 100% (baseline) | **99%** (с silence-cut + prompt) | 80-90% (нет правого контекста) |
| Швы между сегментами | N/A | **Почти невидимы** (silence-cut + prompt) | Артефакт каждые 3 сек |
| RAM peak | 3.5GB на 2ч | **300MB на chunk** | 200MB |
| Stop→ready (2ч Balanced) | 20-35 мин | **~3-4 мин** | 5-10 сек |
| Quality preset на 2ч | Timeout-блок | **OK** | OK |
| Crash safety | Все или ничего | **Per-chunk сохраняется** | Per-second |
| Effort | 0 | **~2 спринта** | 3-4 спринта |
| UX-модель | Простая (one-shot artifact) | **Простая** + intermediate states | Сложная (live captions, incremental recap) |

**M13 даёт ~80% UX-выигрыша от realtime за ~30% усилий.** True realtime (M14) — отдельная фича, если будет реальный запрос.

## 3. Seam solutions

Главная сложность chunking'а — швы между сегментами. Четыре решения, по приоритету:

### 3.1 Silence-aware cut (must)

Резать **не** ровно в 10:00.000 (порвёт слово на границе). **Target = 10 мин, search window = 9:00 – 11:00**. Искать тихий сегмент с `RMS < 0.01` длительностью `≥ 300мс` — там cut. Если в окне 2 минуты ни одной паузы — резать в локальном минимуме RMS (rare для разговорной речи).

Реализация: ~30 LOC Rust. Reuse: RMS feed уже идёт через [`audio:level`](../apps/desktop/src-tauri/src/audio/macos.rs#L21) event'ы для эквалайзера.

### 3.2 Context priming через `--prompt` (must)

Whisper без левого контекста хуже узнаёт первые 1-3 секунды нового chunk'а. Точность первой фразы вырастает с ~80% до ~95% через:

```bash
whisper-cli chunk_N+1.wav --prompt "<last 50 words from chunk_N transcript>"
```

`whisper.cpp`-нативный флаг. Реализация: добавить `with_prompt` builder method к [`LocalWhisperRequest`](../apps/desktop/src-tauri/src/local_engine/stt.rs).

### 3.3 Global speaker re-clustering (must)

Pyannote per-chunk выдаёт `Speaker_0, Speaker_1...` **локально** в chunk'е. Тот же физический человек в chunk 1 может быть `Speaker_0`, а в chunk 2 — `Speaker_1`.

Решение:
1. Pyannote per-chunk → segments + 256-dim embedding на segment (через WeSpeaker).
2. Накапливаем все `(chunk_idx, local_speaker_id, embedding)` tuples.
3. После последнего chunk'а — **agglomerative clustering single-link на cosine similarity** с threshold 0.75 → global speaker IDs.
4. Apply `HashMap<(chunk, local), global>` маппинг ко всем segments при concat'е.

Reuse: [`OnnxEmbedder`](../apps/desktop/src-tauri/src/voice_model.rs) (WeSpeaker через `voice-onnx` feature) — уже подключён для biometric matching через contacts. Тот же путь для inter-chunk linking без явных contact-сэмплов.

### 3.4 Overlap dedup (optional, deferred)

Можно делать chunks с 5-сек overlap'ом: chunk N+1 включает последние 5 сек chunk N. После транскрипции дедуплицировать в overlap-зоне (предпочесть версию из chunk N — там больше right-context). Применяется когда нужна **100% seam-free склейка для legal-grade транскриптов**. Для Wotold MVP — overkill, silence-cut + prompt уже даёт 99% качество.

### 3.5 Timestamp offsets (trivial)

`global_ms = chunk_start_ms + chunk_local_ms`. `chunk_start_ms` известен (мы сами решили где резать). Whisper.cpp выдаёт `offsets.from_ms` / `to_ms` per segment относительно chunk'а — добавить offset перед persist'ом.

## 4. Architecture

### 4.1 Sidecar (Swift) — новая команда `rotate`

JSON-протокол [`App.swift`](../apps/desktop/sidecars/macos-audio/Sources/WotoldAudio/App.swift) расширяется:

```jsonc
// stdin
{ "cmd": "rotate",
  "next_mic_path": "/abs/.../chunk_3_mic.wav",
  "next_system_path": "/abs/.../chunk_3_system.wav" }

// stdout
{ "event": "rotated",
  "chunk_idx": 3,
  "duration_sec": 612.4,
  "mic_bytes": 9794560,
  "system_bytes": 9794560 }
```

`rotate` атомарно:
1. Flush + close current `mic.wav` + `system.wav`
2. Open new WAV files по `next_*_path`
3. AudioRecorder + ProcessTapRecorder продолжают писать **без drop'а сэмплов** (через ту же serial queue что и audio callbacks)

**Files:**
- `App.swift` (новая ветка switch для `rotate`)
- `AudioRecorder.swift` (метод `rotate(to: URL)`)
- `ProcessTapRecorder.swift` (метод `rotate(to: URL)`)
- Опционально `WAVWriter.swift` (helper для atomic file swap)

**Effort:** ~50 LOC Swift + Swift Testing unit-tests на rotate atomicity.

### 4.2 Rust orchestrator — silence detector + rotate trigger

Новая утилита `audio::silence_detector`:
- Слушает RMS feed (тот же `audio:level` events)
- Накапливает 60-сек rolling buffer
- На каждой минуте >9 от старта chunk'а ищет тихий сегмент в окне `[T+9:00, T+11:00]`
- Если найдено — отправляет `rotate` команду в sidecar + enqueue'ит pipeline job

**Files:**
- `audio/silence_detector.rs` (новый, ~50 LOC + 3-5 unit-tests)
- `audio/macos.rs` (rotate API wrapper)
- `commands/recording.rs` (subscribe на rotate-events, enqueue jobs)

### 4.3 Per-chunk pipeline orchestrator

Новый модуль `pipeline::chunk_runner`:

```rust
async fn run_chunk(
    call_id: Uuid,
    chunk_idx: u32,
    chunk_start_ms: u64,
    mic_path: PathBuf,
    system_path: PathBuf,
    prev_transcript_tail: Option<String>,
) -> Result<ChunkArtifacts, AppError> {
    let mixed_path = mix_to_mono(&mic_path, &system_path).await?;
    let whisper = LocalWhisperRequest::new(&mixed_path)
        .with_prompt(prev_transcript_tail.as_deref())
        .run()
        .await?;
    let pyannote = run_pyannote(&mixed_path).await?;
    let embeddings = extract_per_segment_embeddings(&mixed_path, &pyannote).await?;
    let aligned = align_with_offset(whisper, pyannote, chunk_start_ms);
    emit_event("transcript:chunk_done", ChunkProgress {
        call_id, chunk_idx, segments: aligned.len()
    });
    Ok(ChunkArtifacts { transcript: aligned, embeddings })
}
```

**Reuse:**
- [`LocalWhisperProvider::run`](../apps/desktop/src-tauri/src/local_engine/stt.rs) (добавить `with_prompt`)
- [`run_pyannote`](../apps/desktop/src-tauri/src/local_engine/diarization.rs)
- [`Embedder::extract`](../apps/desktop/src-tauri/src/voice_model.rs) (StubEmbedder или OnnxEmbedder)
- [`audio::wav_chunker::read_wav_segment`](../apps/desktop/src-tauri/src/audio/wav_chunker.rs) (для per-segment crops)

**Files:**
- `pipeline/chunk_runner.rs` (новый, ~100 LOC + unit-tests)
- `local_engine/stt.rs` (with_prompt builder)

### 4.4 Global speaker re-clustering

После `stop` + await всех chunk pipelines:

```rust
async fn finalize_recording(call_id: Uuid, chunks: Vec<ChunkArtifacts>) -> Result<Call> {
    let global_map = agglomerative_cluster(
        chunks.iter()
              .flat_map(|c| c.embeddings.iter().map(|e| (e.local_speaker, e.vec.clone())))
              .collect(),
        /* cosine_threshold */ 0.75,
    );
    let final_transcript = concat_with_global_speakers(chunks, &global_map);
    let recap = run_llm_recap(&final_transcript).await?;
    persist_call(call_id, final_transcript, recap).await
}
```

Agglomerative single-link на cosine similarity — ~50 LOC. WeSpeaker embeddings обычно дают `>0.9` для same-speaker, threshold 0.75 даёт robust separation.

**Files:**
- `pipeline/speaker_reclustering.rs` (новый, ~80 LOC + unit-tests)
- `pipeline/mod.rs` (post-stop wire-up)

### 4.5 Frontend UX

Активная запись (HomePage / CallDetailPage):

```
┌─ Идёт запись • 1ч 12 мин · локально ────────┐
│                                              │
│  ✓ Сегмент 1 · 0:00–10:02 · готов            │
│  ⏵ Сегмент 2 · 10:02–20:08 · обрабатывается  │
│  ▱ Сегмент 3 · 20:08–30:14 · ждёт            │
│  ▱ Сегмент 4 · 30:14–...   · запись          │
│                                              │
└──────────────────────────────────────────────┘
```

После `stop`:
- Chunks ещё доделываются: `Готовим транскрипт... · 4/4 сегментов готовы`
- LLM recap фаза: `Составляем саммари...`
- Готово → как сейчас, [`CallDetailPage`](../apps/desktop/src/pages/CallDetailPage.tsx) с интерактивным транскриптом

**Files:**
- `components/ChunkProgressStrip.tsx` (новый, ~50 LOC)
- `pages/CallDetailPage.tsx`, `pages/HomePage.tsx` (intermediate states)
- `recording/RecordingContext.tsx` (chunk progress state)
- `api/recording.ts` (listen на `transcript:chunk_done` events)
- `i18n/{ru,en,kk}.ts` (chunk progress keys)

### 4.6 DB

Новая таблица или поля в существующей `calls`:

```sql
-- Option A: новая таблица
CREATE TABLE call_chunks (
    call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
    chunk_idx INTEGER NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    mic_path TEXT NOT NULL,
    system_path TEXT NOT NULL,
    status TEXT NOT NULL,  -- pending/processing/done/failed
    transcript_json TEXT,
    PRIMARY KEY (call_id, chunk_idx)
);
```

Reuse паттерна из существующего [`db/calls.rs`](../apps/desktop/src-tauri/src/db/calls.rs).

## 5. Acceptance Criteria

> Статус: реализовано и включено по умолчанию (M13.3.4). Пункты, требующие измерений на
> двухчасовой записи, вынесены в [`ROADMAP.md`](ROADMAP.md) §B — держать их здесь значит
> дублировать один и тот же блокер в двух местах.

### Functional

- [x] **F1.** 10-минутные chunks с silence-aware cut в окне `[T+9:00, T+11:00]` — M13.1.1, `audio/silence_detector.rs`
- [x] **F2.** Pipeline per-chunk параллельно с записью следующего — M13.2.2
- [x] **F3.** Global speaker clustering между chunks — M13.2.1, `pipeline/speaker_reclustering.rs`
- [x] **F5.** После `stop` все chunks await'аются, потом одноразовый LLM recap — M13.2.2 (drain-on-stop)

### UX

- [x] **U1.** Progress strip во время активной записи — M13.3.1, `ChunkProgressStrip.tsx`
- [x] **U2.** Промежуточные состояния «Готовим транскрипт… → Составляем саммари…» — M13.3.2
- [x] **U3.** CallDetail после готовности идентичен прежнему flow — M13.3.x

### Robustness

- [x] **R1.** Crash-safety: готовые chunks видны после перезапуска — персист `call_chunks` (M13.1.4) + авто-восстановление на старте (B28.2)
- [x] **R2.** Пустое окно silence-cut → fallback к local min RMS — M13.1.1
- [x] **R3.** pyannote вернул 0 speakers → чанк пропускается gracefully — M13.2.1

### Измерения на реальной записи → `ROADMAP.md` §B

F4 (≥99% совпадения с full-file baseline на 25-мин фикстуре), P1 (stop→ready ≤ 5 мин),
P2 (Quality на 2 ч не упирается в таймаут), P3 (RAM ≤ 800 МБ) — требуют двухчасовой
многоспикерной записи, которой в репозитории нет.

## 6. Phased rollout

Реализация в 3 фазы для снижения риска регрессий:

### Phase 1 — Silent cut + sequential pipeline (no UX surfacing)

Chunks создаются и обрабатываются **последовательно** (no pipelining). Final concat работает. Behavior на уровне финального результата идентичен текущему flow. Feature flag `chunked_pipeline=false` по умолчанию.

**Effort:** ~3-4 дня. **Verification:** dual-run на 30-мин фикстуре, diff transcripts.

### Phase 2 — Parallel pipelining + speaker re-clustering

Chunk N обрабатывается **параллельно** с записью chunk N+1. Global speaker re-clustering. По-прежнему flag-gated.

**Effort:** ~4-5 дней. **Verification:** multi-speaker фикстура + сравнение global speaker IDs.

### Phase 3 — UX surfacing + flag-on по умолчанию

Frontend chunk-progress strip, intermediate states, finalize feature flag → ON по умолчанию.

**Effort:** ~2-3 дня. **Verification:** UX QA на 6 theme×accent + live запись.

**Total:** ~2 спринта (10-15 рабочих дней) с QA и tuning.

## 7. Tests

- **Silence-detector:**
  - Silence in window → cut в её середине
  - No silence (2 минуты подряд гул) → fall к local RMS minimum
  - Edge case: silence at exact window boundary
- **Chunk runner:**
  - Sequential prompts передаются (chunk N+1 видит prompt из chunk N)
  - Offsets correctly merged (chunk N+1 segments timestamp ≥ chunk N end)
  - Pyannote embeddings собраны (≥1 per segment ≥0.5s)
- **Speaker re-clustering:**
  - Same physical speaker в 2 chunks → single global ID
  - 2 разных speaker'а → 2 разных global ID
  - Threshold tuning: borderline case 0.74 vs 0.76
- **Sidecar `rotate`:**
  - Атомарность (no sample drop, проверка через SHA на raw PCM)
  - File sync после rotate (next file имеет valid WAV header)
- **Integration:**
  - 25-минутная multi-speaker фикстура → 3 chunks → final transcript ≥99% совпадает с full-file baseline
  - Crash mid-recording → recovery shows готовые chunks

## 8. Risks

1. **Sidecar `rotate` race conditions** — если `rotate` приходит между чтением CoreAudio buffer и записью в WAV, можно потерять 5-10мс. **Mitigation:** `rotate` обрабатывается в той же serial queue что и audio callbacks (lock-free hand-off через atomic file swap).

2. **Speaker re-clustering accuracy** — agglomerative с threshold 0.75 может слепить двух разных людей с похожими голосами. **Mitigation:** integration test на multi-speaker фикстуре + param tuning через grid search. WeSpeaker embeddings обычно дают `>0.9` для same-speaker.

3. **Disk I/O contention** — параллельно: запись 2 WAV (mic+system, ~32KB/s) + чтение 2 WAV для STT + чтение для diarization. **Mitigation:** на M-серии NVMe (>1GB/s sustained) — не проблема. На старых HDD-Macs — feature flag можно убрать через hw probe.

4. **Whisper.cpp `--prompt` limit** — там max 224 tokens (≈ 50 слов). Если последние 50 слов chunk'а N длиннее → truncate с конца. **Mitigation:** word-counted slice, не char-counted.

5. **R11 паспорта формальное противоречие** — обновляется в этой же сессии (см. action items).

## 9. Что НЕ входит в M13

- **True realtime live captions** (1-4 сек лаг) — отдельный milestone (M14), если будет реальный запрос. Требует другой UX модель: live-captions pane + incremental recap + handling unstable partial-transcripts.
- **Cloud chunking** — Soniox/Gladia уже стримятся inherently, не требуют наших chunks. Cloud-managed path остаётся без изменений.
- **Resumable recording** (продолжить запись после crash) — отдельная feature, требует другой UX и DB-схему.
- **Speaker biometric matching через contacts** — это [V7/B3.7](ROADMAP.md), независимо от chunks.

## 10. Existing infrastructure to reuse

| Что | Где | Зачем |
|---|---|---|
| `LocalWhisperProvider::run` | [`local_engine/stt.rs`](../apps/desktop/src-tauri/src/local_engine/stt.rs) | STT engine — добавить `with_prompt` |
| `run_pyannote` | [`local_engine/diarization.rs`](../apps/desktop/src-tauri/src/local_engine/diarization.rs) | Per-chunk segmentation |
| `audio::wav_chunker::read_wav_segment` | [`audio/wav_chunker.rs`](../apps/desktop/src-tauri/src/audio/wav_chunker.rs) | Read WAV slice для embedding |
| `OnnxEmbedder::extract` | [`voice_model.rs`](../apps/desktop/src-tauri/src/voice_model.rs) | WeSpeaker embeddings per segment |
| `audio:level` Tauri event | [`audio/macos.rs`](../apps/desktop/src-tauri/src/audio/macos.rs) | RMS feed для silence detector |
| `EventBus` (Rust) + `listen()` (TS) | [`events.rs`](../apps/desktop/src-tauri/src/events.rs) | Pattern для `transcript:chunk_done` |
| `db::test_support::fresh_db` | [`db/mod.rs`](../apps/desktop/src-tauri/src/db/mod.rs) | Integration test fixture |

## 11. Open questions

- **DB schema:** новая таблица `call_chunks` или JSONB-поле в `calls.chunks_metadata`? — TBD в Phase 1.
- **Feature flag механизм:** настройка в Settings UI или env-флаг? — TBD, рекомендация: settings toggle (debug section), переключится в `default ON` в Phase 3.
- **Min chunk duration:** что если запись завершилась через 7 минут (меньше chunk size)? — Trivial fallback: записываем как single chunk, pipeline тот же. No chunk strip UX.

## 12. References

- Inline chat-discussion 2026-05-22 (сессия с M12 polish + Settings rework + Process Tap + widget fixes)
- [ROADMAP §M13](ROADMAP.md#m13--chunked-pipelined-transcription)
- [ПАСПОРТ §11 + §12 R11](ПАСПОРТ_ПРОЕКТА.md)
- whisper.cpp `--prompt` docs: https://github.com/ggml-org/whisper.cpp
- WeSpeaker paper: https://arxiv.org/abs/2210.17016
- Apple Core Audio Process Tap (related M1 work): macOS 14.4+ AudioCap
