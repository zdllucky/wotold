//! [M12.4] Model Catalog & Manager — runtime download + SHA256 verify + preset.
//!
//! Расширение паттерна B3.7c ([crate::voice_model]) на 6 моделей: Whisper
//! small/medium/large-v3 для STT + Qwen 2.5 1.5B / 3B / 7B GGUF для LLM.
//! См. PRD v0.2 §M12.4 + §11 O1.
//!
//! # Контракт безопасности (W5, PRD M12.4.6)
//!
//! SHA256 — единственная защита от подмены release-файла. CDN (HuggingFace)
//! может быть скомпрометнут — но хэш в коде → atomic rename не происходит,
//! partial файл удаляется. `/security-scan` обязателен на этот модуль перед
//! merge.
//!
//! # Обновление каталога
//!
//! SHA256 + size_bytes получены через [`scripts/refresh-model-catalog.sh`](../../../../../scripts/refresh-model-catalog.sh)
//! (PRD §14 pre-flight). Скрипт делает HEAD к HuggingFace, читает
//! `X-Linked-Etag` (LFS SHA256) + `Content-Length`. Для bump'а модели —
//! отредактировать MODELS в скрипте, перезапустить, вставить вывод сюда.
//!
//! # LLM выбор (PRD §11 O1 deviation)
//!
//! Изначально Gemma 3 2B был в Light preset, но репы
//! `bartowski/gemma-3-2b-it-GGUF` / `unsloth/gemma-3-2b-it-GGUF` /
//! `google/gemma-2-2b-it` гейтятся Google TOS (HTTP 401 без HF token).
//! Заменили на Qwen 2.5 1.5B — приличный русский, без accept-license flow.
//! PRD §11 O1 явно разрешает замену LLM («не финал, дверь открыта»).
//!
//! # События для UI
//!
//! `model:progress` — `{ id, pct, bytes_done, bytes_total }`
//! `model:done`     — `{ id, status: "ok" | "verify_failed" | "io_error" }`

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::AppError;

/// Тип модели в каталоге — STT (Whisper) / LLM (GGUF) / Diarization
/// (pyannote segmentation .onnx для sherpa-onnx OfflineSpeakerDiarization).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Stt,
    Llm,
    Diarization,
}

/// Стабильный id записи в каталоге. Newtype-обёртка чтобы не путать со
/// строковыми ключами settings. См. PRD §M12.4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ModelId(pub &'static str);

impl ModelId {
    pub const WHISPER_SMALL: ModelId = ModelId("whisper-small");
    pub const WHISPER_MEDIUM: ModelId = ModelId("whisper-medium");
    pub const WHISPER_LARGE_V3: ModelId = ModelId("whisper-large-v3");
    pub const QWEN25_1_5B: ModelId = ModelId("qwen25-1_5b");
    pub const QWEN25_3B: ModelId = ModelId("qwen25-3b");
    pub const QWEN25_7B: ModelId = ModelId("qwen25-7b");
    /// [M12-D5] Pyannote segmentation 3.0 для sherpa-onnx
    /// OfflineSpeakerDiarization. Shared across all 3 presets (~6 MB).
    pub const PYANNOTE_SEGMENTATION: ModelId = ModelId("pyannote-segmentation");

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Запись каталога. Захардкожено в `MODEL_CATALOG` (PRD §M12.4.1, DEFERRED
/// alternative — `models-manifest.json` endpoint).
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: ModelId,
    pub kind: ModelKind,
    pub display_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub license_url: &'static str,
}

/// Каталог — 3 Whisper + 3 LLM модели. SHA256 + size_bytes получены через
/// `scripts/refresh-model-catalog.sh` (PRD §14 pre-flight) на 2026-05-22.
/// При замене файла на HF — bump version в скрипте + регенерировать.
pub const MODEL_CATALOG: [ModelEntry; 7] = [
    ModelEntry {
        id: ModelId::WHISPER_SMALL,
        kind: ModelKind::Stt,
        display_name: "Whisper Small (RU+EN)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        size_bytes: 190_085_487,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        id: ModelId::WHISPER_MEDIUM,
        kind: ModelKind::Stt,
        display_name: "Whisper Medium (RU+EN)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        size_bytes: 539_212_467,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        id: ModelId::WHISPER_LARGE_V3,
        kind: ModelKind::Stt,
        display_name: "Whisper Large v3",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        size_bytes: 1_081_140_203,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        id: ModelId::QWEN25_1_5B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (1.5B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
        sha256: "1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370",
        size_bytes: 986_048_768,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::QWEN25_3B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (3B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
        sha256: "9c9f56a391a3abbd5b89d0245bf6106081bcc3173119d4229235dd9d23253f94",
        size_bytes: 1_929_903_264,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::QWEN25_7B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (7B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
        sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423",
        size_bytes: 4_683_074_240,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::PYANNOTE_SEGMENTATION,
        kind: ModelKind::Diarization,
        display_name: "Pyannote Segmentation 3.0",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx",
        sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
        size_bytes: 5_992_913,
        license_url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0",
    },
];

/// Найти запись каталога по id. `None` если id неизвестен — caller обязан
/// сообщить ошибку, никогда не fallback'иться на «дефолт».
pub fn lookup(id: &str) -> Option<&'static ModelEntry> {
    MODEL_CATALOG.iter().find(|m| m.id.as_str() == id)
}

/// Записать timestamp использования модели — для Storage UI (M12.4.4-bis).
/// Вызывается pipeline'ом по success completion'у локального run'а.
pub async fn touch_usage(pool: &sqlx::SqlitePool, id: &str) -> Result<(), AppError> {
    if lookup(id).is_none() {
        return Err(AppError::Other(format!("unknown model id: {id}")));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO local_engine_model_usage (model_id, last_used_at) VALUES (?1, ?2)
         ON CONFLICT(model_id) DO UPDATE SET last_used_at = excluded.last_used_at",
    )
    .bind(id)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// Прочитать last_used_at для всех моделей. `None` если модель ни разу не
/// использовалась. Возвращает HashMap по id для удобства фронта.
pub async fn list_usage(
    pool: &sqlx::SqlitePool,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT model_id, last_used_at FROM local_engine_model_usage")
            .fetch_all(pool)
            .await
            .map_err(AppError::from)?;
    Ok(rows.into_iter().collect())
}

/// Path к файлу модели на диске: `$APP_DATA/local_engine/models/<id>.bin`.
/// LLM-модели всё равно `.bin` (GGUF — это контейнер, расширение не обязано
/// быть `.gguf`). Унифицировано чтобы один resolver работал на оба kind.
pub fn model_path(app_data_dir: &Path, id: &str) -> PathBuf {
    app_data_dir
        .join("local_engine")
        .join("models")
        .join(format!("{id}.bin"))
}

/// Статус модели на диске. Совместим с contract'ом
/// `packages/contracts/src/local-engine.ts::ModelStatus`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelStatus {
    /// Файл отсутствует.
    Absent { id: String, bytes_total: u64 },
    /// Файл на месте + SHA256 совпал.
    Present { id: String, bytes_total: u64 },
    /// Файл есть, но SHA256 не совпадает (corruption / outdated release / TODO_SHA).
    Corrupted {
        id: String,
        bytes_done: u64,
        bytes_total: u64,
        expected: String,
        got: String,
    },
}

/// Быстрая проверка: файл есть на диске + ненулевой размер → `Present`, иначе
/// `Absent`. Без SHA256 — для списков/UI где скорость важнее верификации.
/// SHA256-проверка (corruption) делается только в `check_status` перед реальным
/// использованием модели.
pub async fn check_status_fast(
    app_data_dir: &Path,
    id: &str,
) -> Result<ModelStatus, AppError> {
    let entry = lookup(id).ok_or_else(|| AppError::Other(format!("unknown model id: {id}")))?;
    let path = model_path(app_data_dir, id);
    let meta = tokio::fs::metadata(&path).await;
    match meta {
        Ok(m) if m.len() > 0 => Ok(ModelStatus::Present {
            id: id.to_string(),
            bytes_total: m.len(),
        }),
        _ => Ok(ModelStatus::Absent {
            id: id.to_string(),
            bytes_total: entry.size_bytes,
        }),
    }
}

/// Потоковый SHA256 + размер. Стриминг чтобы не держать в RAM 4GB.
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

/// Узнать статус модели. Никаких сетевых вызовов. Неизвестный id → ошибка.
pub async fn check_status(app_data_dir: &Path, id: &str) -> Result<ModelStatus, AppError> {
    let entry = lookup(id).ok_or_else(|| AppError::Other(format!("unknown model id: {id}")))?;
    let path = model_path(app_data_dir, id);
    if !path.exists() {
        return Ok(ModelStatus::Absent {
            id: id.to_string(),
            bytes_total: entry.size_bytes,
        });
    }
    match file_sha256(&path).await {
        Ok((hash, size)) => {
            if hash.eq_ignore_ascii_case(entry.sha256) {
                Ok(ModelStatus::Present {
                    id: id.to_string(),
                    bytes_total: size,
                })
            } else {
                Ok(ModelStatus::Corrupted {
                    id: id.to_string(),
                    bytes_done: size,
                    bytes_total: entry.size_bytes,
                    expected: entry.sha256.to_string(),
                    got: hash,
                })
            }
        }
        Err(_) => Ok(ModelStatus::Absent {
            id: id.to_string(),
            bytes_total: entry.size_bytes,
        }),
    }
}

/// Payload события `model:progress`. Совместим с contracts/local-engine.ts.
#[derive(Clone, Serialize)]
pub struct ProgressEvent {
    pub id: String,
    pub pct: f32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Payload события `model:done`.
#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DoneEvent {
    Ok {
        id: String,
    },
    VerifyFailed {
        id: String,
        expected: String,
        got: String,
    },
    IoError {
        id: String,
        message: String,
    },
    AlreadyPresent {
        id: String,
    },
}

pub const MODEL_PROGRESS: &str = "model:progress";
pub const MODEL_DONE: &str = "model:done";

fn emit<T: Serialize + Clone>(app: Option<&AppHandle>, name: &str, payload: &T) {
    let Some(handle) = app else {
        return;
    };
    if let Err(e) = handle.emit(name, payload) {
        log::warn!("emit {name} failed: {e}");
    }
}

/// Скачать модель. Идемпотентно: если уже Present — no-op + emit
/// `AlreadyPresent`. Atomic rename `.partial → final` после SHA256 match.
/// Партиал при mismatch удаляется. См. PRD §M12.4.2, §M12.4.5.
pub async fn download(
    app_data_dir: &Path,
    id: &str,
    app: Option<&AppHandle>,
) -> Result<PathBuf, AppError> {
    let entry = lookup(id).ok_or_else(|| AppError::Other(format!("unknown model id: {id}")))?;
    let dest = model_path(app_data_dir, id);

    // Идемпотентность (M12.4.5): уже Present → no-op.
    if let Ok(ModelStatus::Present { .. }) = check_status(app_data_dir, id).await {
        emit(
            app,
            MODEL_DONE,
            &DoneEvent::AlreadyPresent { id: id.to_string() },
        );
        return Ok(dest);
    }

    let result = download_inner(app_data_dir, entry, app).await;
    if let Err(e) = &result {
        emit(
            app,
            MODEL_DONE,
            &DoneEvent::IoError {
                id: id.to_string(),
                message: e.to_string(),
            },
        );
    }
    result
}

async fn download_inner(
    app_data_dir: &Path,
    entry: &ModelEntry,
    app: Option<&AppHandle>,
) -> Result<PathBuf, AppError> {
    let dest = model_path(app_data_dir, entry.id.as_str());
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::Other("invalid model path".to_string()))?;
    fs::create_dir_all(parent)
        .await
        .map_err(|e| AppError::Other(format!("mkdir {}: {e}", parent.display())))?;

    let tmp = parent.join(format!("{}.bin.partial", entry.id.as_str()));
    let _ = fs::remove_file(&tmp).await;

    let client = reqwest::Client::builder()
        .user_agent("wotold/0.0.1")
        .build()
        .map_err(|e| AppError::Other(format!("reqwest builder: {e}")))?;
    let resp = client
        .get(entry.url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {}: {e}", entry.url)))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "model {} download HTTP {}",
            entry.id.as_str(),
            resp.status()
        )));
    }
    let total = resp.content_length().unwrap_or(entry.size_bytes);

    let mut file = File::create(&tmp)
        .await
        .map_err(|e| AppError::Other(format!("create {}: {e}", tmp.display())))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut next_emit_at: u64 = 0;
    // Throttle событий — модели до 4.5GB, эмит на каждые 256KB заспамит UI.
    // 1MB шаг даёт ~4500 событий на large model — нормально для прогресс-бара.
    const EMIT_STEP: u64 = 1024 * 1024;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::Other(format!("download chunk: {e}")))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| AppError::Other(format!("write {}: {e}", tmp.display())))?;
        hasher.update(&bytes);
        downloaded += bytes.len() as u64;
        if downloaded >= next_emit_at {
            let pct = if total > 0 {
                (downloaded as f64 / total as f64 * 100.0) as f32
            } else {
                0.0
            };
            emit(
                app,
                MODEL_PROGRESS,
                &ProgressEvent {
                    id: entry.id.as_str().to_string(),
                    pct,
                    bytes_done: downloaded,
                    bytes_total: total,
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
    if !got.eq_ignore_ascii_case(entry.sha256) {
        // Corrupted / placeholder SHA → удаляем партиал, сообщаем юзеру.
        let _ = fs::remove_file(&tmp).await;
        emit(
            app,
            MODEL_DONE,
            &DoneEvent::VerifyFailed {
                id: entry.id.as_str().to_string(),
                expected: entry.sha256.to_string(),
                got: got.clone(),
            },
        );
        return Err(AppError::Other(format!(
            "SHA256 mismatch for {}: expected {}, got {}",
            entry.id.as_str(),
            entry.sha256,
            got
        )));
    }

    // Atomic rename — `.partial → final`. Existing файл (corrupted) затрётся.
    fs::rename(&tmp, &dest).await.map_err(|e| {
        AppError::Other(format!(
            "rename {} → {}: {e}",
            tmp.display(),
            dest.display()
        ))
    })?;

    emit(
        app,
        MODEL_DONE,
        &DoneEvent::Ok {
            id: entry.id.as_str().to_string(),
        },
    );
    log::info!(
        "local-engine model downloaded: {} ({} bytes)",
        dest.display(),
        downloaded
    );
    Ok(dest)
}

/// Удалить модель с диска. Идемпотентно (no-op если отсутствует). См. PRD
/// §M12.4.4: cleanup при смене preset — решение пользователя, не авто.
pub async fn delete(app_data_dir: &Path, id: &str) -> Result<(), AppError> {
    if lookup(id).is_none() {
        return Err(AppError::Other(format!("unknown model id: {id}")));
    }
    let path = model_path(app_data_dir, id);
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

    #[test]
    fn catalog_has_three_stt_three_llm_one_diarization() {
        let stt = MODEL_CATALOG
            .iter()
            .filter(|m| m.kind == ModelKind::Stt)
            .count();
        let llm = MODEL_CATALOG
            .iter()
            .filter(|m| m.kind == ModelKind::Llm)
            .count();
        let diar = MODEL_CATALOG
            .iter()
            .filter(|m| m.kind == ModelKind::Diarization)
            .count();
        assert_eq!(
            stt, 3,
            "expected 3 STT models (whisper small/medium/large-v3)"
        );
        assert_eq!(llm, 3, "expected 3 LLM models (qwen25-1_5b/3b/7b)");
        assert_eq!(diar, 1, "expected 1 diarization model (pyannote)");
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for entry in MODEL_CATALOG.iter() {
            assert!(
                seen.insert(entry.id.as_str()),
                "duplicate model id: {}",
                entry.id.as_str()
            );
        }
    }

    #[test]
    fn catalog_urls_use_https() {
        for entry in MODEL_CATALOG.iter() {
            assert!(
                entry.url.starts_with("https://"),
                "model {} URL must be HTTPS: {}",
                entry.id.as_str(),
                entry.url
            );
        }
    }

    #[test]
    fn lookup_finds_each_canonical_id() {
        for entry in MODEL_CATALOG.iter() {
            assert_eq!(
                lookup(entry.id.as_str()).map(|e| e.id.as_str()),
                Some(entry.id.as_str())
            );
        }
        assert!(lookup("not-a-model").is_none());
    }

    #[test]
    fn model_path_uses_local_engine_subdir() {
        let p = model_path(Path::new("/data"), "whisper-small");
        assert!(
            p.to_string_lossy()
                .ends_with("/data/local_engine/models/whisper-small.bin"),
            "got {}",
            p.display()
        );
    }

    #[tokio::test]
    async fn check_status_absent_when_file_missing() {
        let dir = tempdir().unwrap();
        let status = check_status(dir.path(), "whisper-small").await.unwrap();
        match status {
            ModelStatus::Absent { id, .. } => assert_eq!(id, "whisper-small"),
            s => panic!("expected Absent, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_corrupted_when_sha_mismatch() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"not-a-real-whisper-model").await.unwrap();
        let status = check_status(dir.path(), "whisper-small").await.unwrap();
        match status {
            ModelStatus::Corrupted {
                id,
                bytes_done,
                expected,
                got,
                ..
            } => {
                assert_eq!(id, "whisper-small");
                assert_eq!(bytes_done, 24);
                // Placeholder SHA — `got` ≠ TODO_SHA256.
                assert_ne!(got, expected);
            }
            s => panic!("expected Corrupted, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_rejects_unknown_id() {
        let dir = tempdir().unwrap();
        let err = check_status(dir.path(), "no-such-model").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn check_status_fast_present_when_file_exists_nonzero() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"some-bytes").await.unwrap();
        let status = check_status_fast(dir.path(), "whisper-small").await.unwrap();
        match status {
            ModelStatus::Present { id, bytes_total } => {
                assert_eq!(id, "whisper-small");
                assert_eq!(bytes_total, 10);
            }
            s => panic!("expected Present, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_fast_absent_when_zero_bytes() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"").await.unwrap();
        let status = check_status_fast(dir.path(), "whisper-small").await.unwrap();
        match status {
            ModelStatus::Absent { id, .. } => assert_eq!(id, "whisper-small"),
            s => panic!("expected Absent, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_fast_absent_when_missing() {
        let dir = tempdir().unwrap();
        let status = check_status_fast(dir.path(), "qwen25-1_5b").await.unwrap();
        match status {
            ModelStatus::Absent { id, bytes_total } => {
                assert_eq!(id, "qwen25-1_5b");
                // bytes_total = catalog entry size, not 0
                assert!(bytes_total > 0);
            }
            s => panic!("expected Absent, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_fast_rejects_unknown_id() {
        let dir = tempdir().unwrap();
        let err = check_status_fast(dir.path(), "not-real").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn check_status_fast_does_not_compute_sha256() {
        // Если бы fast-path делал SHA256, то «не-валидный» payload вернул
        // бы Corrupted (как делает обычный check_status). Fast возвращает
        // Present игнорируя содержимое.
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"not-a-real-model-payload").await.unwrap();
        let status = check_status_fast(dir.path(), "whisper-small").await.unwrap();
        assert!(matches!(status, ModelStatus::Present { .. }));
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let dir = tempdir().unwrap();
        // Удаление отсутствующей модели — no-op (not an error).
        delete(dir.path(), "whisper-small").await.unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"dummy").await.unwrap();
        assert!(p.exists());
        delete(dir.path(), "whisper-small").await.unwrap();
        assert!(!p.exists());
        delete(dir.path(), "whisper-small").await.unwrap();
    }

    #[tokio::test]
    async fn delete_rejects_unknown_id() {
        let dir = tempdir().unwrap();
        assert!(delete(dir.path(), "no-such-model").await.is_err());
    }
}
