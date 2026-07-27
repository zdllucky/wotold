//! Commands for speaker confirmation flow + voice samples management.

use tauri::State;

use crate::{
    db::{CallSpeakerView, VoiceSampleView},
    state::AppState,
    AppError,
};

// ============================================================
// M3.5 (#26) speaker confirmation flow
// ============================================================

/// Спикеры звонка + текущая привязка + suggestion. UI рисует на основе этого.
#[tauri::command]
pub async fn list_call_speakers(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<CallSpeakerView>, AppError> {
    crate::db::list_call_speakers(&state.db, &call_id).await
}

/// [TD-46] Спикеры сразу для списка звонков — один запрос вместо запроса на
/// строку инбокса. Ответ — карта `call_id → спикеры`; звонки без спикеров в
/// ней отсутствуют.
#[tauri::command]
pub async fn list_call_speakers_batch(
    state: State<'_, AppState>,
    call_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<CallSpeakerView>>, AppError> {
    // [TD-05, правило 7] id приходят из webview. Запрос параметризован, но
    // валидация всё равно обязательна: невалидный id тут означает ошибку
    // вызывающего, и молча отдавать по нему пустоту — прятать её.
    for id in &call_ids {
        crate::call_id::CallId::parse(id)?;
    }
    crate::db::list_speakers_for_calls(&state.db, &call_ids).await
}

/// R2 паспорта: финальная привязка спикер↔контакт ТОЛЬКО через явное действие
/// пользователя. Используется UI confirmation flow.
#[tauri::command]
pub async fn confirm_call_speaker(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    call_speaker_id: String,
    contact_id: String,
) -> Result<(), AppError> {
    crate::db::confirm_call_speaker(&state.db, &call_speaker_id, &contact_id).await?;
    respawn_assistant_index(&app, &state, &call_speaker_id).await;
    Ok(())
}

/// Откатить ранее подтверждённую привязку (юзер передумал).
#[tauri::command]
pub async fn unbind_call_speaker(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    call_speaker_id: String,
) -> Result<(), AppError> {
    crate::db::unbind_call_speaker(&state.db, &call_speaker_id).await?;
    respawn_assistant_index(&app, &state, &call_speaker_id).await;
    Ok(())
}

/// [M16.6] Смена привязки спикера меняет имена в пассажах ассистента —
/// переиндексировать звонок (fire-and-forget, ошибки резолва — тихий скип:
/// индекс догонит startup-backfill'ом).
async fn respawn_assistant_index(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    call_speaker_id: &str,
) {
    let call_id: Option<(String,)> =
        sqlx::query_as("SELECT call_id FROM call_speakers WHERE id = ?1")
            .bind(call_speaker_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_else(|e| {
                log::warn!("assistant reindex on speaker change: lookup failed: {e}");
                None
            });
    if let Some((call_id,)) = call_id {
        crate::assistant::indexer::spawn_index(app, &call_id);
    }
}

// ============================================================
// M3.6 / M7.4 (#45) voice samples view + manual delete (C3)
// ============================================================

#[tauri::command]
pub async fn list_voice_samples(
    state: State<'_, AppState>,
    contact_id: String,
) -> Result<Vec<VoiceSampleView>, AppError> {
    crate::db::list_voice_samples(&state.db, &contact_id).await
}

/// Manual delete одного семпла (C3 паспорта). Используется когда пользователь
/// ошибочно подтвердил спикера или хочет очистить устаревший биометрический
/// слепок.
#[tauri::command]
pub async fn delete_voice_sample(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    crate::db::delete_voice_sample(&state.db, &id).await
}

// ============================================================
// [P4] Voice sample slice playback — extract WAV bytes для UI playback.
// ============================================================

/// [P4] Get WAV bytes для voice sample slice (start_sec..end_sec из
/// {mic|system}.wav). Replacement для P3 «play full source call» подхода,
/// который давал silence когда sample был с другой track.
///
/// Errors:
/// - `voice_sample_not_found` — id не существует.
/// - `voice_sample_legacy_no_slice` — legacy row (start_sec / end_sec /
///   track_kind = NULL). UI выключает play button по этому пути.
/// - `voice_sample_source_missing` — source_call NULL либо WAV file gone.
#[tauri::command]
pub async fn get_voice_sample_audio(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<u8>, AppError> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT source_call, start_sec, end_sec, track_kind
         FROM voice_samples WHERE id = ?1",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Other(format!("voice_sample_not_found: {id}")))?;

    let source_call: Option<String> = row.try_get("source_call")?;
    let start_sec: Option<f64> = row.try_get("start_sec")?;
    let end_sec: Option<f64> = row.try_get("end_sec")?;
    let track_kind: Option<String> = row.try_get("track_kind")?;

    let (source_call, start_sec, end_sec, track_kind) =
        match (source_call, start_sec, end_sec, track_kind) {
            (Some(sc), Some(s), Some(e), Some(t)) => (sc, s, e, t),
            _ => {
                return Err(AppError::Other(
                    "voice_sample_legacy_no_slice: slice metadata missing".into(),
                ));
            }
        };

    // [TD-05] Путь строит CallStore, а не ручной join: source_call приходит из
    // БД (voice_samples.source_call), но единая точка сборки пути дешевле, чем
    // ещё один callsite, который легко забыть при следующей правке.
    let track = crate::call_store::AudioKind::from_str(&track_kind)
        .ok_or_else(|| AppError::Other(format!("voice_sample_track_invalid: {track_kind}")))?;
    let wav_path = state
        .store
        .audio_path(&crate::call_id::CallId::from_db(&source_call), track);
    if !wav_path.exists() {
        return Err(AppError::Other(format!(
            "voice_sample_source_missing: {}",
            wav_path.display()
        )));
    }

    // Heavy hound read — на blocking pool чтобы не залипать в async runtime.
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let clip = crate::audio_io::extract_segment(&wav_path, start_sec, end_sec)?;
        clip_to_wav_bytes(&clip)
    })
    .await
    .map_err(|e| AppError::Other(format!("voice_sample_blocking_join: {e}")))??;

    Ok(bytes)
}

/// [P4] Encode `AudioClip` (f32 PCM, mono) → in-memory WAV bytes.
/// Reuses hound::WavWriter pattern из `pipeline::audio_merger`. f32 → i16
/// нормализация: clamp [-1.0, 1.0] × i16::MAX.
fn clip_to_wav_bytes(clip: &crate::audio_io::AudioClip) -> Result<Vec<u8>, AppError> {
    use std::io::Cursor;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: clip.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::with_capacity(clip.samples.len() * 2 + 44);
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| AppError::Other(format!("wav writer init: {e}")))?;
        for s in &clip.samples {
            let clamped = s.clamp(-1.0, 1.0);
            let i = (clamped * i16::MAX as f32) as i16;
            writer
                .write_sample(i)
                .map_err(|e| AppError::Other(format!("wav write sample: {e}")))?;
        }
        writer
            .finalize()
            .map_err(|e| AppError::Other(format!("wav finalize: {e}")))?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_io::AudioClip;

    #[test]
    fn clip_to_wav_bytes_produces_riff_header() {
        let clip = AudioClip {
            samples: vec![0.0, 0.5, -0.5, 1.0, -1.0],
            sample_rate: 16_000,
        };
        let bytes = clip_to_wav_bytes(&clip).unwrap();
        // RIFF header: "RIFF" + 4 bytes size + "WAVE"
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // 5 i16 samples = 10 bytes data + 44 bytes header = 54 bytes total.
        assert_eq!(bytes.len(), 44 + 10);
    }

    #[test]
    fn clip_to_wav_bytes_handles_empty_clip() {
        let clip = AudioClip {
            samples: vec![],
            sample_rate: 16_000,
        };
        let bytes = clip_to_wav_bytes(&clip).unwrap();
        // Header only — valid empty WAV.
        assert_eq!(bytes.len(), 44);
        assert_eq!(&bytes[0..4], b"RIFF");
    }

    #[test]
    fn clip_to_wav_bytes_clamps_out_of_range() {
        // > 1.0 must clamp without panic.
        let clip = AudioClip {
            samples: vec![2.0, -2.0, 1.5],
            sample_rate: 16_000,
        };
        let bytes = clip_to_wav_bytes(&clip).unwrap();
        assert_eq!(bytes.len(), 44 + 6); // 3 i16 samples
    }
}
