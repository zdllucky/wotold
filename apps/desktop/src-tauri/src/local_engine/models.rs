//! [M12.4] Model Catalog & Manager — runtime download + SHA256 verify + preset.
//!
//! Единственная качалка моделей приложения. Изначально — 6 моделей: Whisper
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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex as StdMutex, OnceLock};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::AppError;

pub use super::model_catalog::{lookup, ModelEntry, ModelId, ModelKind, MODEL_CATALOG};

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

/// Быстрая проверка без SHA256: файл есть + размер ТОЧНО равен каталожному →
/// `Present`; размер иной → `Corrupted` (обрубок докачки / чужой файл);
/// нет файла → `Absent`.
///
/// [perf] Используется и в UI-списках, и в горячем пути пайплайна
/// (`run_local_inner` / `build_local_llm_provider` / `diarize_track`):
/// полный SHA256 на 1.5-6GB моделей при каждом прогоне давал десятки секунд
/// «Сохраняем аудио». Криптографическая верификация (подмена HF-релиза,
/// M12.4/W5) остаётся на download-пути (`check_status` после скачивания) —
/// exact-size ловит битые докачки, но не подмену с тем же размером.
///
/// Placeholder-энтри каталога (`sha256 = "PLACEHOLDER_…"`, точный размер
/// неизвестен) деградируют к прежней семантике «len > 0 → Present».
pub async fn check_status_fast(app_data_dir: &Path, id: &str) -> Result<ModelStatus, AppError> {
    let entry = lookup(id).ok_or_else(|| AppError::Other(format!("unknown model id: {id}")))?;
    let path = model_path(app_data_dir, id);
    let size_is_authoritative = !entry.sha256.starts_with("PLACEHOLDER");
    let meta = tokio::fs::metadata(&path).await;
    match meta {
        Ok(m) if m.len() == entry.size_bytes => Ok(ModelStatus::Present {
            id: id.to_string(),
            bytes_total: m.len(),
        }),
        Ok(m) if m.len() > 0 && !size_is_authoritative => Ok(ModelStatus::Present {
            id: id.to_string(),
            bytes_total: m.len(),
        }),
        Ok(m) if m.len() > 0 => Ok(ModelStatus::Corrupted {
            id: id.to_string(),
            bytes_done: m.len(),
            bytes_total: entry.size_bytes,
            expected: format!("size {}", entry.size_bytes),
            got: format!("size {}", m.len()),
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
/// [TD-10] Per-id async-мьютекс скачивания. Раньше два параллельных
/// `download` одного id (кнопка/авто-догрузка пресета + фоновой авто-download
/// ассистента) писали в один партиал `{id}.bin.partial`, и SHA считался по
/// сетевому потоку каждого — оба могли «сойтись», пока на диске лежал
/// интерливинг. Лок берётся ДО идемпотентной проверки, поэтому второй вызов
/// дожидается первого и коротко замыкается на `AlreadyPresent`.
///
/// `OnceLock`, не `LazyLock` — MSRV 1.77 (см. resource_queue.rs).
fn download_lock(id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<StdMutex<HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let map = LOCKS.get_or_init(|| StdMutex::new(HashMap::new()));
    // PoisonError::into_inner: реестр — просто HashMap<id, Arc>, паника
    // держателя лока его не портит (тот же приём, что в resource_queue).
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard.entry(id.to_string()).or_default().clone()
}

/// [TD-10] Имя временного файла скачивания. Уникально между вызовами, чтобы
/// параллельные писатели одного id не делили файл (см. `download_lock`).
fn partial_name(id: &str) -> String {
    format!("{}.{}.partial", id, uuid::Uuid::new_v4().simple())
}

pub async fn download(
    app_data_dir: &Path,
    id: &str,
    app: Option<&AppHandle>,
) -> Result<PathBuf, AppError> {
    let entry = lookup(id).ok_or_else(|| AppError::Other(format!("unknown model id: {id}")))?;
    let dest = model_path(app_data_dir, id);

    // [TD-10] Сериализуем скачивания одного id: держим лок на всё время
    // проверки + скачивания, чтобы конкурент дождался и увидел AlreadyPresent.
    let lock = download_lock(id);
    let _dl_guard = lock.lock().await;

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

    // [TD-10] Уникальное имя партиала: даже при обходе лока писатели не делят
    // файл. UUID вместо фиксированного `{id}.bin.partial`.
    let tmp = parent.join(partial_name(entry.id.as_str()));

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

    // [TD-10] Хешируем ЗАПИСАННЫЙ файл, а не сетевой поток: проверяются ровно
    // те байты, что легли на диск. Ловит и интерливинг конкурентов, и
    // повреждение при записи, которые stream-hasher пропускал.
    let (got, _) = file_sha256(&tmp)
        .await
        .map_err(|e| AppError::Other(format!("sha256 {}: {e}", tmp.display())))?;
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
    async fn check_status_fast_present_when_exact_catalog_size() {
        // [perf] Точный размер по каталогу → Present (pyannote самый мелкий
        // authoritative-энтри, ~6MB — ок для теста).
        let dir = tempdir().unwrap();
        let id = ModelId::PYANNOTE_SEGMENTATION.as_str();
        let expected = lookup(id).unwrap().size_bytes;
        let p = model_path(dir.path(), id);
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, vec![0u8; expected as usize]).await.unwrap();
        let status = check_status_fast(dir.path(), id).await.unwrap();
        match status {
            ModelStatus::Present { bytes_total, .. } => assert_eq!(bytes_total, expected),
            s => panic!("expected Present, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_fast_corrupted_on_size_mismatch() {
        // Обрубок докачки: размер не совпал с каталожным → Corrupted.
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"some-bytes").await.unwrap();
        let status = check_status_fast(dir.path(), "whisper-small")
            .await
            .unwrap();
        match status {
            ModelStatus::Corrupted {
                bytes_done,
                bytes_total,
                ..
            } => {
                assert_eq!(bytes_done, 10);
                assert!(bytes_total > 10);
            }
            s => panic!("expected Corrupted, got {s:?}"),
        }
    }

    #[tokio::test]
    async fn check_status_fast_rejects_wrong_size_for_silero() {
        // [TD-10] Регрессия: раньше silero был placeholder-записью и обходил
        // size-check (len>0 → Present), поэтому любой мусорный файл нужного
        // имени скармливался whisper-cli как VAD-модель. Теперь размер
        // авторитетен для всех — маленький файл → Corrupted.
        let dir = tempdir().unwrap();
        let id = ModelId::SILERO_VAD.as_str();
        let p = model_path(dir.path(), id);
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"tiny").await.unwrap();
        let status = check_status_fast(dir.path(), id).await.unwrap();
        assert!(
            matches!(status, ModelStatus::Corrupted { .. }),
            "мусорный файл нужного имени не должен считаться моделью, got {status:?}"
        );
    }

    #[test]
    fn partial_name_is_unique_per_call() {
        let a = partial_name("qwen25-3b");
        let b = partial_name("qwen25-3b");
        assert_ne!(a, b, "имена партиалов должны различаться между вызовами");
        assert!(a.starts_with("qwen25-3b."));
        assert!(a.ends_with(".partial"));
    }

    #[test]
    fn catalog_has_no_placeholder_entries() {
        // [TD-10] Инвариант вместо прежней тихой деградации: placeholder-SHA
        // делал модель недокачиваемой И отключал size-check. Тест не даёт
        // вернуть его незаметно.
        for e in MODEL_CATALOG.iter() {
            assert!(
                !e.sha256.contains("PLACEHOLDER"),
                "{} — placeholder SHA",
                e.id.as_str()
            );
            assert_eq!(e.sha256.len(), 64, "{}: SHA не 64 hex", e.id.as_str());
            assert!(
                e.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: SHA не hex",
                e.id.as_str()
            );
            assert!(e.size_bytes > 0, "{}: нулевой размер", e.id.as_str());
        }
    }

    #[tokio::test]
    async fn check_status_fast_absent_when_zero_bytes() {
        let dir = tempdir().unwrap();
        let p = model_path(dir.path(), "whisper-small");
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, b"").await.unwrap();
        let status = check_status_fast(dir.path(), "whisper-small")
            .await
            .unwrap();
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
        // Мусорный payload ТОЧНОГО каталожного размера → Present: fast-путь
        // сверяет только размер, содержимое не хеширует (иначе был бы
        // Corrupted как в полном check_status).
        let dir = tempdir().unwrap();
        let id = ModelId::PYANNOTE_SEGMENTATION.as_str();
        let expected = lookup(id).unwrap().size_bytes;
        let p = model_path(dir.path(), id);
        fs::create_dir_all(p.parent().unwrap()).await.unwrap();
        fs::write(&p, vec![0xAB; expected as usize]).await.unwrap();
        let status = check_status_fast(dir.path(), id).await.unwrap();
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

    /// [TD-10] Живая проверка: скачать silero (885KB) настоящим кодом и
    /// убедиться, что SHA сходится и статус становится Present. Требует сети,
    /// поэтому `#[ignore]` — запуск: `cargo test live_download_silero -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_download_silero_verifies_sha() {
        let dir = tempdir().unwrap();
        let id = ModelId::SILERO_VAD.as_str();
        let path = download(dir.path(), id, None)
            .await
            .expect("download silero");
        assert!(path.exists());
        // full-SHA путь: check_status считает хеш по файлу.
        let status = check_status(dir.path(), id).await.unwrap();
        assert!(
            matches!(status, ModelStatus::Present { .. }),
            "после скачивания silero должен быть Present, got {status:?}"
        );
        // партиалы не должны остаться.
        let leftover: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".partial"))
            .collect();
        assert!(leftover.is_empty(), "остались партиалы: {leftover:?}");
    }
}
