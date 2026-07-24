//! [Phase 4 R10] Filesystem repository for call artifacts.
//!
//! Раньше каждый callsite строил пути inline:
//!   state.app_data_dir.join("calls").join(&call_id).join("transcript.md")
//! Это ломало:
//! - DRY: 8+ копий "calls" / "mic.wav" / "transcript.md" литералов.
//! - Тестируемость: mock'ать app_data_dir не получалось без тащить весь AppState.
//! - Atomicity: операции delete_call / wipe_all_data расползались на много файлов.
//!
//! Теперь:
//! - `CallStore` инкапсулирует `app_data_dir`.
//! - Все хелперы (`mic_path`, `system_path`, `artifact_path`, `read_artifact`,
//!   `remove_call_dir`) живут здесь.
//! - Каждый callsite получает store через `state.store.xxx(...)`.

use std::path::{Path, PathBuf};

use crate::call_id::{ensure_path_under, CallId};
use crate::AppError;

/// Виды артефактов на диске. `ArtifactKind` → filename централизованно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// `recap.md` — Markdown саммари от LLM.
    Recap,
    /// `transcript.md` — отрендеренный диаризованный транскрипт.
    Transcript,
    /// `raw_stt.json` — сырые сегменты mic + system + merged. Нужен для regenerate_recap.
    RawStt,
}

impl ArtifactKind {
    /// Filename внутри call_dir.
    pub fn filename(self) -> &'static str {
        match self {
            ArtifactKind::Recap => "recap.md",
            ArtifactKind::Transcript => "transcript.md",
            ArtifactKind::RawStt => "raw_stt.json",
        }
    }

    /// Парсинг строкового идентификатора (из Tauri command'ы).
    /// Возвращает `None` если kind неизвестен.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "recap" => Some(ArtifactKind::Recap),
            "transcript" => Some(ArtifactKind::Transcript),
            "raw_stt" => Some(ArtifactKind::RawStt),
            _ => None,
        }
    }
}

/// Виды аудио-дорожек.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    /// `mic.wav` — микрофонная дорожка (owner-speaker).
    Mic,
    /// `system.wav` — системное аудио (все собеседники).
    System,
}

impl AudioKind {
    pub fn filename(self) -> &'static str {
        match self {
            AudioKind::Mic => "mic.wav",
            AudioKind::System => "system.wav",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "mic" => Some(AudioKind::Mic),
            "system" => Some(AudioKind::System),
            _ => None,
        }
    }
}

/// Filesystem-репозиторий call-данных. Дёшев в клонировании (один `PathBuf`),
/// держится в `Arc` внутри `AppState`.
#[derive(Debug, Clone)]
pub struct CallStore {
    app_data_dir: PathBuf,
}

impl CallStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self { app_data_dir }
    }

    /// Корень для всех `calls/<id>/...` директорий.
    pub fn calls_root(&self) -> PathBuf {
        self.app_data_dir.join("calls")
    }

    /// Директория конкретного звонка. Создание — caller'у через `tokio::fs::create_dir_all`.
    ///
    /// [TD-05] Принимает [`CallId`], а не `&str`: сырая строка из webview
    /// раньше попадала в `join` напрямую, и `".."` уводил на `app_data_dir`.
    /// Тип — единственная гарантия, что валидацию нельзя забыть.
    pub fn call_dir(&self, call_id: &CallId) -> PathBuf {
        self.calls_root().join(call_id.as_str())
    }

    /// Путь к mic.wav.
    pub fn mic_path(&self, call_id: &CallId) -> PathBuf {
        self.call_dir(call_id).join(AudioKind::Mic.filename())
    }

    /// Путь к system.wav.
    pub fn system_path(&self, call_id: &CallId) -> PathBuf {
        self.call_dir(call_id).join(AudioKind::System.filename())
    }

    /// Путь к произвольной аудио-дорожке.
    pub fn audio_path(&self, call_id: &CallId, kind: AudioKind) -> PathBuf {
        self.call_dir(call_id).join(kind.filename())
    }

    /// Путь к артефакту (recap / transcript / raw_stt).
    pub fn artifact_path(&self, call_id: &CallId, kind: ArtifactKind) -> PathBuf {
        self.call_dir(call_id).join(kind.filename())
    }

    // ── [M13.1.3b] Chunk paths ───────────────────────────────────────────
    //
    // Структура: `calls/<call_id>/chunks/<idx>/{mic,system}.wav`. Каждый
    // chunk изолирован в своей поддиректории — лёгкая очистка по chunk_idx
    // при failed-chunk retry + понятно где partial recovery после crash'а.

    /// Корневая директория для всех chunks одного звонка.
    #[allow(dead_code)]
    pub fn chunks_dir(&self, call_id: &CallId) -> PathBuf {
        self.call_dir(call_id).join("chunks")
    }

    /// Директория конкретного chunk'а (содержит mic.wav + system.wav).
    #[allow(dead_code)]
    pub fn chunk_dir(&self, call_id: &CallId, idx: u32) -> PathBuf {
        self.chunks_dir(call_id).join(idx.to_string())
    }

    /// Путь к mic.wav конкретного chunk'а.
    #[allow(dead_code)]
    pub fn chunk_mic_path(&self, call_id: &CallId, idx: u32) -> PathBuf {
        self.chunk_dir(call_id, idx).join("mic.wav")
    }

    /// Путь к system.wav конкретного chunk'а.
    #[allow(dead_code)]
    pub fn chunk_system_path(&self, call_id: &CallId, idx: u32) -> PathBuf {
        self.chunk_dir(call_id, idx).join("system.wav")
    }

    /// Создать chunk-директорию (idempotent). Возвращает path для удобства
    /// chaining (caller обычно сразу же открывает mic/system WAV там).
    #[allow(dead_code)]
    pub async fn ensure_chunk_dir(&self, call_id: &CallId, idx: u32) -> Result<PathBuf, AppError> {
        let dir = self.chunk_dir(call_id, idx);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AppError::Other(format!("ensure_chunk_dir: {e}")))?;
        Ok(dir)
    }

    /// Корневой `app_data_dir` (для legacy callsite'ов — `voice_model::*`).
    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    /// Прочитать артефакт с диска. `Ok(None)` если файла нет — это НЕ ошибка
    /// (`recap.md` может не быть если LLM упал; `transcript.md` может не быть
    /// если pipeline ещё идёт).
    pub async fn read_artifact(
        &self,
        call_id: &CallId,
        kind: ArtifactKind,
    ) -> Result<Option<String>, AppError> {
        let path = self.artifact_path(call_id, kind);
        // [TD-05] Второй слой поверх CallId: даже если тип когда-нибудь
        // сконструируют из мусора через from_db, за пределы calls/ не выйдем.
        ensure_path_under(&path, &self.calls_root()).map_err(AppError::NotFound)?;
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Other(format!("read {}: {e}", kind.filename()))),
        }
    }

    /// Удалить директорию звонка целиком. Идемпотентно: если её нет — Ok.
    /// Используется в `delete_call` (cascade C5) и cancel_reprocess (если
    /// что-то останется).
    pub async fn remove_call_dir(&self, call_id: &CallId) -> Result<(), AppError> {
        let dir = self.call_dir(call_id);
        // [TD-05] Самая дорогая операция слоя — recursive delete. Guard стоит
        // здесь безусловно: цена проверки нулевая, цена ошибки — вся БД.
        ensure_path_under(&dir, &self.calls_root()).map_err(AppError::NotFound)?;
        if !dir.exists() {
            return Ok(());
        }
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|e| AppError::Other(format!("rm {} failed: {e}", dir.display())))?;
        Ok(())
    }

    /// Удалить весь `calls/` корень (для wipe_all_data / GDPR Art. 17).
    /// Идемпотентно.
    pub async fn remove_all_calls(&self) -> Result<(), AppError> {
        let dir = self.calls_root();
        if !dir.exists() {
            return Ok(());
        }
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|e| AppError::Other(format!("rm calls dir failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::call_id::CallId;
    /// [TD-05] Тестовые id — каноничные v4: `CallStore` принимает только
    /// валидированный `CallId`, прежние литералы вроде "c1" им быть не могут.
    const TEST_CALL_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
    #[allow(dead_code)]
    const TEST_CALL_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
    #[allow(dead_code)]
    const TEST_CALL_GHOST: &str = "99999999-9999-4999-8999-999999999999";
    #[allow(dead_code)]
    fn cid(s: &str) -> CallId {
        CallId::parse(s).expect("тестовый id должен быть каноничным uuid")
    }

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn call_dir_joins_app_data_calls_id() {
        let store = CallStore::new(PathBuf::from("/data"));
        let dir = store.call_dir(&cid(TEST_CALL_A));
        assert!(dir
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa"));
    }

    #[test]
    fn mic_and_system_paths_match_layout() {
        let store = CallStore::new(PathBuf::from("/data"));
        assert!(store
            .mic_path(&cid(TEST_CALL_A))
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/mic.wav"));
        assert!(store
            .system_path(&cid(TEST_CALL_A))
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/system.wav"));
    }

    #[test]
    fn artifact_path_uses_kind_filename() {
        let store = CallStore::new(PathBuf::from("/data"));
        assert!(store
            .artifact_path(&cid(TEST_CALL_A), ArtifactKind::Recap)
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/recap.md"));
        assert!(store
            .artifact_path(&cid(TEST_CALL_A), ArtifactKind::Transcript)
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/transcript.md"));
        assert!(store
            .artifact_path(&cid(TEST_CALL_A), ArtifactKind::RawStt)
            .to_string_lossy()
            .ends_with("/data/calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/raw_stt.json"));
    }

    #[test]
    fn artifact_kind_from_str_matches_legacy_idents() {
        assert_eq!(ArtifactKind::from_str("recap"), Some(ArtifactKind::Recap));
        assert_eq!(
            ArtifactKind::from_str("transcript"),
            Some(ArtifactKind::Transcript)
        );
        assert_eq!(
            ArtifactKind::from_str("raw_stt"),
            Some(ArtifactKind::RawStt)
        );
        assert_eq!(ArtifactKind::from_str("ghost"), None);
    }

    #[test]
    fn audio_kind_from_str_matches_legacy_idents() {
        assert_eq!(AudioKind::from_str("mic"), Some(AudioKind::Mic));
        assert_eq!(AudioKind::from_str("system"), Some(AudioKind::System));
        assert_eq!(AudioKind::from_str("mixed"), None);
    }

    #[tokio::test]
    async fn read_artifact_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let got = store
            .read_artifact(&cid(TEST_CALL_GHOST), ArtifactKind::Recap)
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn read_artifact_returns_some_when_present() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let call_dir = store.call_dir(&cid(TEST_CALL_A));
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("recap.md"), "# Recap\n")
            .await
            .unwrap();

        let got = store
            .read_artifact(&cid(TEST_CALL_A), ArtifactKind::Recap)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("# Recap\n"));
    }

    /// [TD-05] Регрессия: раньше `remove_call_dir("..")` резолвился в
    /// `calls/..` = app_data_dir и сносил всю БД вместе с записями. Теперь
    /// такой id не построить — `CallId::parse` его не пропустит, — но guard
    /// внутри store всё равно проверяем через доверенный конструктор.
    #[tokio::test]
    async fn remove_call_dir_refuses_to_escape_calls_root() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let victim = dir.path().join("app.db");
        tokio::fs::write(&victim, b"important").await.unwrap();
        tokio::fs::create_dir_all(store.calls_root()).await.unwrap();

        let escaping = CallId::from_db("..");
        let err = store
            .remove_call_dir(&escaping)
            .await
            .expect_err("`..` → Err");
        assert!(format!("{err}").contains("'..' segment"), "получили: {err}");
        assert!(victim.exists(), "app.db обязан пережить попытку traversal");
    }

    /// Тот же guard на чтении: `read_artifact` не должен отдавать файлы
    /// за пределами calls/.
    #[tokio::test]
    async fn read_artifact_refuses_to_escape_calls_root() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let err = store
            .read_artifact(&CallId::from_db("../.."), ArtifactKind::Recap)
            .await
            .expect_err("traversal → Err");
        assert!(format!("{err}").contains("'..' segment"), "получили: {err}");
    }

    #[tokio::test]
    async fn remove_call_dir_is_idempotent() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        // Нет директории — Ok.
        store.remove_call_dir(&cid(TEST_CALL_GHOST)).await.unwrap();

        // Создаём + удаляем + повторное удаление.
        let call_dir = store.call_dir(&cid(TEST_CALL_A));
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("mic.wav"), b"raw")
            .await
            .unwrap();
        assert!(call_dir.exists());
        store.remove_call_dir(&cid(TEST_CALL_A)).await.unwrap();
        assert!(!call_dir.exists());
        store.remove_call_dir(&cid(TEST_CALL_A)).await.unwrap();
    }

    #[tokio::test]
    async fn remove_all_calls_clears_root() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let c1 = store.call_dir(&cid(TEST_CALL_A));
        let c2 = store.call_dir(&cid(TEST_CALL_B));
        tokio::fs::create_dir_all(&c1).await.unwrap();
        tokio::fs::create_dir_all(&c2).await.unwrap();
        store.remove_all_calls().await.unwrap();
        assert!(!store.calls_root().exists());
        // Идемпотентно.
        store.remove_all_calls().await.unwrap();
    }

    #[test]
    fn chunk_paths_canonical_structure() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let mic = store.chunk_mic_path(&cid(TEST_CALL_A), 5);
        let sys = store.chunk_system_path(&cid(TEST_CALL_A), 5);
        assert!(mic.ends_with("calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/chunks/5/mic.wav"));
        assert!(sys.ends_with("calls/aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa/chunks/5/system.wav"));
        // chunks_dir и chunk_dir вкладываются последовательно.
        assert_eq!(
            store.chunk_dir(&cid(TEST_CALL_A), 5).parent().unwrap(),
            store.chunks_dir(&cid(TEST_CALL_A))
        );
    }

    #[tokio::test]
    async fn ensure_chunk_dir_creates_recursively() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let path = store.ensure_chunk_dir(&cid(TEST_CALL_A), 3).await.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
        // Idempotent.
        store.ensure_chunk_dir(&cid(TEST_CALL_A), 3).await.unwrap();
    }
}
