//! [B3.7c] Voice embedder model — runtime download + SHA256 verify.
//!
//! WeSpeaker `wespeaker_en_voxceleb_resnet34_LM.onnx` (25.3MB) скачивается
//! с sherpa-onnx releases по требованию пользователя в Settings (а не на
//! первой записи — иначе оффлайн-юзер увидит «качаем 25MB» в самый
//! неподходящий момент).
//!
//! Хранится в `$APP_DATA/models/embedder.onnx`. `OnnxEmbedder::load()`
//! (под `--features voice-onnx`) подхватывает файл если он есть; иначе
//! pipeline fallback'ит на StubEmbedder (R2 паспорта: без модели юзер
//! сам confirm'ит спикеров).
//!
//! SHA256 захардкожен — если sherpa-onnx когда-нибудь перезальёт релиз,
//! verify сломается → пользователь увидит «модель повреждена, пересохранить».
//!
//! # События для UI
//!
//! `voice-model:progress` — `{ downloaded: u64, total: u64, percent: f32 }`
//! `voice-model:done`     — `{ status: "ok" | "verify_failed" | "io_error" }`

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::AppError;

/// URL release-файла. Sherpa-onnx releases page (k2-fsa). Pinned релиз,
/// SHA256 верифицируется ниже — если в репо перезальют, нужен явный update.
pub const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34_LM.onnx";

/// SHA256 файла (lowercase hex). Из `checksum.txt` в том же release.
/// Несовпадение → атомарный rename не происходит, partial файл удаляется.
pub const MODEL_SHA256: &str = "e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012";

/// Грубый размер для UI «Скачиваем 25MB...» pre-fetch hint. Реальный
/// размер берётся из Content-Length response header.
pub const MODEL_SIZE_HINT: u64 = 26_530_550;

/// Статус модели на диске.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelStatus {
    /// Файл отсутствует или другой ошибки I/O.
    Missing,
    /// Файл на месте + SHA256 совпал.
    Valid { size: u64 },
    /// Файл есть, но SHA256 не совпадает (corruption / outdated release).
    Corrupted {
        size: u64,
        expected: String,
        got: String,
    },
}

/// Path где лежит / должна лежать модель.
pub fn model_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join("embedder.onnx")
}

/// Прочитать файл и посчитать SHA256. Streaming-чтение (большие файлы 26MB
/// в RAM один раз — приемлемо но streaming чище для будущих > 100MB моделей).
async fn file_sha256(path: &Path) -> Result<(String, u64), std::io::Error> {
    use tokio::io::AsyncReadExt;
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

/// Узнать статус модели на диске. Никаких сетевых вызовов.
pub async fn check_status(app_data_dir: &Path) -> ModelStatus {
    let path = model_path(app_data_dir);
    if !path.exists() {
        return ModelStatus::Missing;
    }
    match file_sha256(&path).await {
        Ok((hash, size)) => {
            if hash.eq_ignore_ascii_case(MODEL_SHA256) {
                ModelStatus::Valid { size }
            } else {
                ModelStatus::Corrupted {
                    size,
                    expected: MODEL_SHA256.to_string(),
                    got: hash,
                }
            }
        }
        Err(_) => ModelStatus::Missing,
    }
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    downloaded: u64,
    total: u64,
    percent: f32,
}

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum DoneEvent {
    Ok,
    VerifyFailed { expected: String, got: String },
    IoError { message: String },
}

/// Скачать модель с MODEL_URL в `$APP_DATA/models/embedder.onnx.partial`,
/// проверить SHA256, атомарно переименовать. Эмитит `voice-model:progress`
/// каждые ~64KB прочитанных байт, `voice-model:done` в конце.
///
/// На success возвращает Ok(path к финальному файлу).
pub async fn download(app_data_dir: &Path, app: &AppHandle) -> Result<PathBuf, AppError> {
    let result = download_inner(app_data_dir, app).await;
    // Emit done event с типизированным statusом — frontend слушает один
    // канал `voice-model:done` независимо от типа ошибки.
    if let Err(e) = &result {
        let _ = app.emit(
            "voice-model:done",
            DoneEvent::IoError {
                message: e.to_string(),
            },
        );
    }
    result
}

async fn download_inner(app_data_dir: &Path, app: &AppHandle) -> Result<PathBuf, AppError> {
    let dest = model_path(app_data_dir);
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::Other("invalid model path".to_string()))?;
    fs::create_dir_all(parent)
        .await
        .map_err(|e| AppError::Other(format!("mkdir {}: {e}", parent.display())))?;

    let tmp = parent.join("embedder.onnx.partial");
    // Чистим предыдущий partial если был.
    let _ = fs::remove_file(&tmp).await;

    let client = reqwest::Client::builder()
        .user_agent("wotold/0.0.1")
        .build()
        .map_err(|e| AppError::Other(format!("reqwest builder: {e}")))?;
    let resp = client
        .get(MODEL_URL)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {MODEL_URL}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "model download HTTP {}",
            resp.status()
        )));
    }
    let total = resp.content_length().unwrap_or(MODEL_SIZE_HINT);

    let mut file = File::create(&tmp)
        .await
        .map_err(|e| AppError::Other(format!("create {}: {e}", tmp.display())))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut next_emit_at: u64 = 0;
    // Эмитим прогресс каждые 256KB чтобы не залить event-bus, но юзер видел
    // живой прогресс-бар.
    const EMIT_STEP: u64 = 256 * 1024;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::Other(format!("download chunk: {e}")))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| AppError::Other(format!("write {}: {e}", tmp.display())))?;
        hasher.update(&bytes);
        downloaded += bytes.len() as u64;
        if downloaded >= next_emit_at {
            let percent = if total > 0 {
                (downloaded as f64 / total as f64 * 100.0) as f32
            } else {
                0.0
            };
            let _ = app.emit(
                "voice-model:progress",
                ProgressEvent {
                    downloaded,
                    total,
                    percent,
                },
            );
            next_emit_at = downloaded + EMIT_STEP;
        }
    }
    file.flush()
        .await
        .map_err(|e| AppError::Other(format!("flush {}: {e}", tmp.display())))?;
    drop(file);

    let got = format!("{:x}", hasher.finalize());
    if !got.eq_ignore_ascii_case(MODEL_SHA256) {
        // Удалить corrupted partial и сообщить пользователю.
        let _ = fs::remove_file(&tmp).await;
        let _ = app.emit(
            "voice-model:done",
            DoneEvent::VerifyFailed {
                expected: MODEL_SHA256.to_string(),
                got: got.clone(),
            },
        );
        return Err(AppError::Other(format!(
            "SHA256 mismatch: expected {MODEL_SHA256}, got {got}"
        )));
    }

    // Atomic rename — partial → final. Если existing там есть (corrupted
    // ранее), он перезаписывается.
    fs::rename(&tmp, &dest).await.map_err(|e| {
        AppError::Other(format!(
            "rename {} → {}: {e}",
            tmp.display(),
            dest.display()
        ))
    })?;

    let _ = app.emit("voice-model:done", DoneEvent::Ok);
    log::info!(
        "voice model downloaded: {} ({} bytes)",
        dest.display(),
        downloaded
    );
    Ok(dest)
}

/// Удалить модель с диска (для GDPR Art. 17 / опт-аут юзера).
pub async fn delete(app_data_dir: &Path) -> Result<(), AppError> {
    let path = model_path(app_data_dir);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path)
        .await
        .map_err(|e| AppError::Other(format!("delete {}: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn check_status_missing_returns_missing() {
        let dir = tempdir().unwrap();
        assert_eq!(check_status(dir.path()).await, ModelStatus::Missing);
    }

    #[tokio::test]
    async fn check_status_corrupted_when_sha_mismatch() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path());
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"not-a-real-onnx-file").await.unwrap();
        let status = check_status(dir.path()).await;
        match status {
            ModelStatus::Corrupted {
                size,
                expected,
                got,
            } => {
                assert_eq!(size, 20);
                assert_eq!(expected, MODEL_SHA256);
                assert_ne!(got, MODEL_SHA256);
            }
            s => panic!("expected Corrupted, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn delete_removes_file_idempotent() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path());
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"dummy").await.unwrap();
        assert!(p.exists());
        delete(dir.path()).await.unwrap();
        assert!(!p.exists());
        // Повторное удаление — no-op.
        delete(dir.path()).await.unwrap();
    }

    #[test]
    fn model_path_uses_models_subdir() {
        let p = model_path(Path::new("/data"));
        assert!(p.to_string_lossy().ends_with("/data/models/embedder.onnx"));
    }
}
