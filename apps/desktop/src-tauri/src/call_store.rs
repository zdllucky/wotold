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
    pub fn call_dir(&self, call_id: &str) -> PathBuf {
        self.calls_root().join(call_id)
    }

    /// Путь к mic.wav.
    pub fn mic_path(&self, call_id: &str) -> PathBuf {
        self.call_dir(call_id).join(AudioKind::Mic.filename())
    }

    /// Путь к system.wav.
    pub fn system_path(&self, call_id: &str) -> PathBuf {
        self.call_dir(call_id).join(AudioKind::System.filename())
    }

    /// Путь к произвольной аудио-дорожке.
    pub fn audio_path(&self, call_id: &str, kind: AudioKind) -> PathBuf {
        self.call_dir(call_id).join(kind.filename())
    }

    /// Путь к артефакту (recap / transcript / raw_stt).
    pub fn artifact_path(&self, call_id: &str, kind: ArtifactKind) -> PathBuf {
        self.call_dir(call_id).join(kind.filename())
    }

    // ── [M13.1.3b] Chunk paths ───────────────────────────────────────────
    //
    // Структура: `calls/<call_id>/chunks/<idx>/{mic,system}.wav`. Каждый
    // chunk изолирован в своей поддиректории — лёгкая очистка по chunk_idx
    // при failed-chunk retry + понятно где partial recovery после crash'а.

    /// Корневая директория для всех chunks одного звонка.
    #[allow(dead_code)]
    pub fn chunks_dir(&self, call_id: &str) -> PathBuf {
        self.call_dir(call_id).join("chunks")
    }

    /// Директория конкретного chunk'а (содержит mic.wav + system.wav).
    #[allow(dead_code)]
    pub fn chunk_dir(&self, call_id: &str, idx: u32) -> PathBuf {
        self.chunks_dir(call_id).join(idx.to_string())
    }

    /// Путь к mic.wav конкретного chunk'а.
    #[allow(dead_code)]
    pub fn chunk_mic_path(&self, call_id: &str, idx: u32) -> PathBuf {
        self.chunk_dir(call_id, idx).join("mic.wav")
    }

    /// Путь к system.wav конкретного chunk'а.
    #[allow(dead_code)]
    pub fn chunk_system_path(&self, call_id: &str, idx: u32) -> PathBuf {
        self.chunk_dir(call_id, idx).join("system.wav")
    }

    /// Создать chunk-директорию (idempotent). Возвращает path для удобства
    /// chaining (caller обычно сразу же открывает mic/system WAV там).
    #[allow(dead_code)]
    pub async fn ensure_chunk_dir(&self, call_id: &str, idx: u32) -> Result<PathBuf, AppError> {
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
        call_id: &str,
        kind: ArtifactKind,
    ) -> Result<Option<String>, AppError> {
        let path = self.artifact_path(call_id, kind);
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Other(format!("read {}: {e}", kind.filename()))),
        }
    }

    /// Удалить директорию звонка целиком. Идемпотентно: если её нет — Ok.
    /// Используется в `delete_call` (cascade C5) и cancel_reprocess (если
    /// что-то останется).
    pub async fn remove_call_dir(&self, call_id: &str) -> Result<(), AppError> {
        let dir = self.call_dir(call_id);
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
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn call_dir_joins_app_data_calls_id() {
        let store = CallStore::new(PathBuf::from("/data"));
        let dir = store.call_dir("abc");
        assert!(dir.to_string_lossy().ends_with("/data/calls/abc"));
    }

    #[test]
    fn mic_and_system_paths_match_layout() {
        let store = CallStore::new(PathBuf::from("/data"));
        assert!(store
            .mic_path("c1")
            .to_string_lossy()
            .ends_with("/data/calls/c1/mic.wav"));
        assert!(store
            .system_path("c1")
            .to_string_lossy()
            .ends_with("/data/calls/c1/system.wav"));
    }

    #[test]
    fn artifact_path_uses_kind_filename() {
        let store = CallStore::new(PathBuf::from("/data"));
        assert!(store
            .artifact_path("c1", ArtifactKind::Recap)
            .to_string_lossy()
            .ends_with("/data/calls/c1/recap.md"));
        assert!(store
            .artifact_path("c1", ArtifactKind::Transcript)
            .to_string_lossy()
            .ends_with("/data/calls/c1/transcript.md"));
        assert!(store
            .artifact_path("c1", ArtifactKind::RawStt)
            .to_string_lossy()
            .ends_with("/data/calls/c1/raw_stt.json"));
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
            .read_artifact("ghost", ArtifactKind::Recap)
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn read_artifact_returns_some_when_present() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let call_dir = store.call_dir("c1");
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("recap.md"), "# Recap\n")
            .await
            .unwrap();

        let got = store
            .read_artifact("c1", ArtifactKind::Recap)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("# Recap\n"));
    }

    #[tokio::test]
    async fn remove_call_dir_is_idempotent() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        // Нет директории — Ok.
        store.remove_call_dir("ghost").await.unwrap();

        // Создаём + удаляем + повторное удаление.
        let call_dir = store.call_dir("c1");
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("mic.wav"), b"raw")
            .await
            .unwrap();
        assert!(call_dir.exists());
        store.remove_call_dir("c1").await.unwrap();
        assert!(!call_dir.exists());
        store.remove_call_dir("c1").await.unwrap();
    }

    #[tokio::test]
    async fn remove_all_calls_clears_root() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let c1 = store.call_dir("c1");
        let c2 = store.call_dir("c2");
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
        let mic = store.chunk_mic_path("abc-123", 5);
        let sys = store.chunk_system_path("abc-123", 5);
        assert!(mic.ends_with("calls/abc-123/chunks/5/mic.wav"));
        assert!(sys.ends_with("calls/abc-123/chunks/5/system.wav"));
        // chunks_dir и chunk_dir вкладываются последовательно.
        assert_eq!(
            store.chunk_dir("abc-123", 5).parent().unwrap(),
            store.chunks_dir("abc-123")
        );
    }

    #[tokio::test]
    async fn ensure_chunk_dir_creates_recursively() {
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let path = store.ensure_chunk_dir("c1", 3).await.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
        // Idempotent.
        store.ensure_chunk_dir("c1", 3).await.unwrap();
    }
}
