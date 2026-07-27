//! [TD-41] Разбор записей, застрявших в `status='recording'` после краша.
//!
//! Выделено из `commands/recording.rs` (1426 строк при лимите 800, правило 8).
//! Вызывается один раз на старте из `state::init`. Логика не менялась.

use sqlx::SqlitePool;

use crate::{call_id::CallId, call_store::CallStore, db, AppError};

use super::recording::MIN_RECORDING_SEC;

/// [B19.6] Длительность WAV (сек) по фактическому размеру файла, а не по полю
/// `data`-чанка в заголовке: у прерванной (не финализированной) записи поле
/// размера может быть нулевым/устаревшим, а длина на диске — достоверна.
/// `None` если файл отсутствует/не WAV/повреждён. Не грузит весь файл — читает
/// только заголовок до `data`-чанка.
fn wav_duration_secs(path: &std::path::Path) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len();
    let mut riff = [0u8; 12];
    f.read_exact(&mut riff).ok()?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return None;
    }
    let mut byte_rate: Option<u32> = None;
    let mut data_offset: Option<u64> = None;
    let mut pos: u64 = 12;
    loop {
        let mut chdr = [0u8; 8];
        if f.read_exact(&mut chdr).is_err() {
            break;
        }
        let size = u32::from_le_bytes([chdr[4], chdr[5], chdr[6], chdr[7]]) as u64;
        let body = pos + 8;
        if &chdr[0..4] == b"fmt " {
            // Sane fmt chunk is 16/18/40 bytes; reject absurd sizes (malformed/
            // adversarial) instead of computing a huge seek.
            if !(16..=128).contains(&size) {
                return None;
            }
            let mut fmt = [0u8; 16];
            if f.read_exact(&mut fmt).is_err() {
                break;
            }
            byte_rate = Some(u32::from_le_bytes([fmt[8], fmt[9], fmt[10], fmt[11]]));
            let skip = size.saturating_sub(16) + (size & 1);
            if skip > 0 {
                f.seek(SeekFrom::Current(skip as i64)).ok()?;
            }
        } else if &chdr[0..4] == b"data" {
            data_offset = Some(body);
            break;
        } else {
            f.seek(SeekFrom::Current((size + (size & 1)) as i64)).ok()?;
        }
        pos = body + size + (size & 1);
    }
    let br = byte_rate? as f64;
    let off = data_offset?;
    if br <= 0.0 || file_len <= off {
        return None;
    }
    Some((file_len - off) as f64 / br)
}

/// [B19.6] На старте разбираем строки, застрявшие в status='recording' (краш/
/// force-quit во время записи). По длине частичного mic-WAV: `<MIN_RECORDING_SEC`
/// или нет аудио → удаляем строку + temp WAV'ы; `≥MIN_RECORDING_SEC` → помечаем
/// 'failed' (восстановимо, юзер сможет переобработать). Возвращает число
/// обработанных строк.
pub async fn reconcile_orphan_recordings(
    pool: &SqlitePool,
    store: &CallStore,
) -> Result<usize, AppError> {
    let ids = db::list_orphan_recording_ids(pool).await?;
    let mut handled = 0usize;
    for id in ids {
        let dur = wav_duration_secs(&store.mic_path(&CallId::from_db(&id)));
        match dur {
            Some(d) if d >= MIN_RECORDING_SEC => {
                // Per-id non-fatal: одна битая строка не должна блокировать остальные.
                if let Err(e) =
                    db::fail_recording_with_reason(pool, &id, Some("Запись прервана")).await
                {
                    log::warn!("orphan recording {id}: mark-failed failed: {e}");
                    continue;
                }
                log::warn!("orphan recording {id}: {d:.0}s → failed (recoverable)");
            }
            _ => {
                // Файлы трём ТОЛЬКО при успешном DB-delete: иначе ghost-строка
                // 'recording' с удалёнными WAV → reconcile зациклится на старте.
                // remove_call_dir сносит весь calls/<id>/ (вкл. chunks/) — C5.
                match db::delete_call_and_samples(pool, &id).await {
                    Ok(()) => {
                        let _ = store.remove_call_dir(&CallId::from_db(&id)).await;
                        log::warn!(
                            "orphan recording {id}: discarded (interrupted <30s or no audio)"
                        );
                    }
                    Err(e) => {
                        log::warn!("orphan recording {id}: delete failed, leaving for retry: {e}");
                        continue;
                    }
                }
            }
        }
        handled += 1;
    }
    Ok(handled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    /// Bytes of a minimal PCM WAV with `data_len` payload bytes. `data_size_field`
    /// lets us simulate an unfinalized header (0) vs a correct size.
    fn wav_bytes(byte_rate: u32, data_len: usize, data_size_field: u32) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&0u32.to_le_bytes()); // riff size — ignored by parser
        b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        b.extend_from_slice(&byte_rate.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes()); // block align
        b.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_size_field.to_le_bytes());
        b.extend(std::iter::repeat(0u8).take(data_len));
        b
    }

    fn write_wav(byte_rate: u32, data_len: usize, data_size_field: u32) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wotold-wavtest-{byte_rate}-{data_len}-{data_size_field}.wav"
        ));
        std::fs::write(&path, wav_bytes(byte_rate, data_len, data_size_field)).unwrap();
        path
    }

    #[test]
    fn duration_uses_file_size_not_header_field() {
        let br = 32_000u32; // 16kHz * 2 bytes/sample
                            // 2s of audio, but header data-size is 0 (unfinalized, crash).
        let p = write_wav(br, 64_000, 0);
        let d = wav_duration_secs(&p).expect("duration");
        assert!((d - 2.0).abs() < 0.05, "expected ~2.0s, got {d}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn duration_correct_with_valid_header() {
        let br = 32_000u32;
        let p = write_wav(br, 16_000, 16_000); // 0.5s
        let d = wav_duration_secs(&p).expect("duration");
        assert!((d - 0.5).abs() < 0.05, "expected ~0.5s, got {d}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn duration_none_for_missing_or_nonwav() {
        assert!(wav_duration_secs(std::path::Path::new("/no/such/file.wav")).is_none());
        let p = std::env::temp_dir().join("wotold-not-a-wav.bin");
        std::fs::write(&p, b"not a wav file at all").unwrap();
        assert!(wav_duration_secs(&p).is_none());
        let _ = std::fs::remove_file(&p);
    }

    /// Helper: insert a status='recording' orphan + write a mic WAV reporting
    /// `secs` of audio (byte_rate=1000 → file_len ≈ secs*1000).
    async fn seed_orphan(pool: &SqlitePool, store: &CallStore, secs: usize) -> String {
        let call = db::insert_recording(pool, "managed").await.unwrap();
        std::fs::create_dir_all(store.call_dir(&CallId::from_db(&call.id))).unwrap();
        std::fs::write(
            store.mic_path(&CallId::from_db(&call.id)),
            wav_bytes(1000, secs * 1000, 0),
        )
        .unwrap();
        call.id
    }

    #[tokio::test]
    async fn reconcile_keeps_long_interrupted_recording_as_failed() {
        let db = fresh_db().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let id = seed_orphan(&db.pool, &store, 35).await; // ≥30s

        let n = reconcile_orphan_recordings(&db.pool, &store).await.unwrap();
        assert_eq!(n, 1);

        let after = db::get_call(&db.pool, &id).await.unwrap().unwrap();
        assert_eq!(
            after.status, "failed",
            "≥30s interrupted → recoverable failed"
        );
        assert_eq!(after.failed_reason.as_deref(), Some("Запись прервана"));
        assert!(
            store.mic_path(&CallId::from_db(&id)).exists(),
            "audio kept for recovery"
        );
    }

    #[tokio::test]
    async fn reconcile_discards_short_interrupted_recording() {
        let db = fresh_db().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let id = seed_orphan(&db.pool, &store, 8).await; // <30s

        let n = reconcile_orphan_recordings(&db.pool, &store).await.unwrap();
        assert_eq!(n, 1);

        assert!(
            db::get_call(&db.pool, &id).await.unwrap().is_none(),
            "<30s interrupted → row deleted"
        );
        assert!(
            !store.call_dir(&CallId::from_db(&id)).exists(),
            "call dir (incl. chunks) removed"
        );
    }
}
