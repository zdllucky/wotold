//! [TD-35] Диаризация дорожек: системной, микрофонной и общий прогон трека.
//!
//! Выделено из `pipeline/mod.rs` (1914 строк при лимите 800, правило 8).
//! Граница естественная: это отдельная фаза обработки со своими моделями
//! (pyannote-segmentation + WeSpeaker) и своими правилами деградации.
//! Логика не менялась.

use std::path::Path;

use sqlx::SqlitePool;

use crate::providers::transcription::DiarizedTranscript;

/// [M12-D5] Прогнать system-track через sherpa-onnx OfflineSpeakerDiarization
/// и смерджить speaker tags в `sys_t.segments`.
///
/// Non-fatal: при отсутствии pyannote / WeSpeaker модели или ошибке inference
/// возвращаем оригинальный `sys_t` без изменений — system track останется
/// single-bucket (`speaker:0`), pipeline продолжит работать в degraded режиме.
///
/// Шаги:
///
/// - Проверка наличия pyannote-segmentation на диске (MODEL_CATALOG).
/// - Проверка наличия WeSpeaker (B3.7c, `voice_model.rs`).
/// - Spawn `SortformerDiarizer` + `.diarize(system_path)`.
/// - Apply `merge::merge_word_with_speaker` на sys_t.segments.
/// - Вернуть обновлённый sys_t.
#[cfg(target_os = "macos")]
pub(crate) async fn diarize_system_track(
    app_data_dir: &Path,
    system_path: &Path,
    sys_t: DiarizedTranscript,
    num_speakers: Option<i32>,
    call_id: &str,
) -> DiarizedTranscript {
    diarize_track(
        app_data_dir,
        system_path,
        sys_t,
        "system",
        num_speakers,
        call_id,
    )
    .await
}

/// [M13 follow-up] Mirror `diarize_system_track` для mic-дорожки. Применяется
/// когда `MIC_DIARIZATION_ENABLED` ON и engine == local. Owner-tag НЕ
/// присваивается здесь — local `speaker:N` tags сохраняются, owner
/// identification идёт отдельным шагом ([`owner_identify`]).
///
/// [P1.2] `num_speakers` — Labs «Force N speakers» override. `None` =
/// auto-detect (sortformer default). Применяется идентично к mic и system.
#[cfg(target_os = "macos")]
pub(crate) async fn diarize_mic_track(
    app_data_dir: &Path,
    mic_path: &Path,
    mic_t: DiarizedTranscript,
    num_speakers: Option<i32>,
    call_id: &str,
) -> DiarizedTranscript {
    diarize_track(app_data_dir, mic_path, mic_t, "mic", num_speakers, call_id).await
}

/// [M13 follow-up] Non-chunked path post-processing: после `diarize_mic_track`
/// на mic-дорожке local `speaker:N` tags. Извлекаем cluster embeddings
/// через `extract_clusters`, вызываем `identify_owner_speaker` и
/// перетеггиваем выбранный tag → `OWNER_TAG`. Cross-track reflection
/// не обрабатывается (non-chunked = нет global remap).
#[cfg(target_os = "macos")]
pub(crate) async fn relabel_owner_on_mic_full_file(
    pool: &SqlitePool,
    app_data_dir: &Path,
    mic_path: &Path,
    system_path: &Path,
    mut mic_t: DiarizedTranscript,
) -> DiarizedTranscript {
    // Fallback на StubEmbedder когда модель отсутствует → cluster_embeddings
    // empty → identify_owner_speaker уходит в duration fallback (acceptable).
    let clusters = match crate::pipeline::clusters::load_and_extract_clusters(
        mic_t.segments.clone(),
        mic_path.to_path_buf(),
        system_path.to_path_buf(),
        app_data_dir,
        "relabel_owner_on_mic",
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("relabel_owner_on_mic: extract_clusters err: {e} — fallback duration");
            std::collections::HashMap::new()
        }
    };
    match crate::pipeline::owner_identify::identify_owner_speaker(pool, &mic_t.segments, &clusters)
        .await
    {
        Ok(Some(owner_local_tag)) if owner_local_tag != crate::pipeline::merge::OWNER_TAG => {
            log::info!(
                "relabel_owner_on_mic: переменовываем {} → {} (остальные tags сохраняются как анонимные спикеры)",
                owner_local_tag,
                crate::pipeline::merge::OWNER_TAG
            );
            for seg in mic_t.segments.iter_mut() {
                if seg.speaker_tag == owner_local_tag {
                    seg.speaker_tag = crate::pipeline::merge::OWNER_TAG.to_string();
                }
            }
        }
        Ok(other) => log::info!(
            "relabel_owner_on_mic: identify → {other:?} (no relabel; уже OWNER либо не нашли)"
        ),
        Err(e) => log::warn!("relabel_owner_on_mic: identify err: {e}"),
    }
    // [Bug-fix] Diagnostic: финальный распределение tags после relabel — чтобы
    // увидеть сколько уникальных спикеров останется в DB (call_speakers).
    {
        use std::collections::BTreeSet;
        let final_tags: BTreeSet<&str> = mic_t
            .segments
            .iter()
            .map(|s| s.speaker_tag.as_str())
            .collect();
        log::info!(
            "relabel_owner_on_mic: финальный distinct tags ({}): {:?}",
            final_tags.len(),
            final_tags
        );
    }
    mic_t
}

/// [M13 follow-up] Общий helper sortformer-диаризации (mic | system). На
/// degraded path (нет моделей / sortformer err) — возвращаем transcript
/// без изменений.
#[cfg(target_os = "macos")]
async fn diarize_track(
    app_data_dir: &Path,
    audio_path: &Path,
    transcript: DiarizedTranscript,
    track_kind: &'static str,
    num_speakers: Option<i32>,
    call_id: &str,
) -> DiarizedTranscript {
    use crate::local_engine::{
        diarization::{Diarizer, SortformerDiarizer},
        merge,
        models::{self, ModelId, ModelStatus},
    };

    // 1. Pyannote segmentation: catalog entry должен быть present.
    //    [perf] fast-чек (exact-size) — на chunked-пути диаризация зовётся
    //    на каждый чанк; SHA здесь мелкий (~6MB), но незачем.
    let seg_path = models::model_path(app_data_dir, ModelId::PYANNOTE_SEGMENTATION.as_str());
    let seg_present = matches!(
        models::check_status_fast(app_data_dir, ModelId::PYANNOTE_SEGMENTATION.as_str()).await,
        Ok(ModelStatus::Present { .. })
    );
    if !seg_present {
        // [Bug-fix #4] log::warn — pyannote-segmentation missing — это
        // explicit gap который юзер должен видеть (toggle на Speakers UI
        // зависит от этой модели). Раньше было log::info → невидимо в release.
        log::warn!(
            "diarize_track[{track_kind}]: pyannote-segmentation отсутствует — \
             диаризация {track_kind}-дорожки не выполнена. Установите модуль в \
             Настройки → Спикеры."
        );
        return transcript;
    }

    // 2. WeSpeaker embedding (B3.7c) — отдельный путь от model catalog.
    let emb_path = crate::voice_model::model_path(app_data_dir);
    if !emb_path.exists() {
        log::warn!(
            "diarize_track[{track_kind}]: WeSpeaker embedder ({}) отсутствует — fall back",
            emb_path.display()
        );
        return transcript;
    }

    // 3-5. Diarize + merge. Любая ошибка → fall back (degraded).
    // [P1.2] `with_num_speakers` clamp'ит override к 1..=MAX_LOCAL_SPEAKERS;
    // None = sherpa-onnx auto (default `-1`).
    // [Q] call_id → очередь диаризации (QueueMonitor видит чей звонок).
    let diarizer =
        SortformerDiarizer::with_num_speakers(seg_path, emb_path, num_speakers).with_call(call_id);
    if let Some(n) = num_speakers {
        log::info!("diarize_track[{track_kind}]: forcing num_clusters={n} (Labs override)");
    }
    let mut speaker_segments = match diarizer.diarize(audio_path).await {
        Ok(segs) => segs,
        Err(e) => {
            log::warn!("diarize_track[{track_kind}]: sortformer err: {e} — fall back");
            return transcript;
        }
    };

    // [P14.3] Reassign overflow `speaker:unknown` segments к ближайшему
    // named-спикеру в окне ±2s. Снижает шум в ParticipantsRow когда
    // sortformer вывел больше MAX_LOCAL_SPEAKERS (=3) кластеров.
    let reassigned =
        crate::local_engine::diarization::reassign_unknown_to_neighbors(&mut speaker_segments, 2.0);
    if reassigned > 0 {
        log::info!(
            "diarize_track[{track_kind}]: reassigned {reassigned} unknown → neighbor speakers"
        );
    }

    // [Bug-fix] Diagnostic: сколько уникальных спикеров sortformer выделил
    // + суммарные durations. Без этого silent-collapse невозможно отличить
    // от "правда был один голос".
    {
        use std::collections::BTreeMap;
        let mut by_tag: BTreeMap<&str, f64> = BTreeMap::new();
        for s in &speaker_segments {
            *by_tag.entry(s.speaker_tag.as_str()).or_insert(0.0) += s.end - s.start;
        }
        let summary: Vec<String> = by_tag.iter().map(|(k, v)| format!("{k}={v:.1}s")).collect();
        log::info!(
            "diarize_track[{track_kind}]: sortformer вывел {} спикер(ов): [{}]",
            by_tag.len(),
            summary.join(", ")
        );
    }

    let merged_segments = merge::merge_word_with_speaker(&transcript.segments, &speaker_segments);
    log::info!(
        "diarize_track[{track_kind}]: {} STT segments + {} speaker segments → {} merged",
        transcript.segments.len(),
        speaker_segments.len(),
        merged_segments.len()
    );
    DiarizedTranscript {
        segments: merged_segments,
        ..transcript
    }
}
