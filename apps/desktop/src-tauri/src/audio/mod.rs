use async_trait::async_trait;

/// Захват системного и микрофонного звука в раздельные дорожки.
/// См. M1 паспорта. Конкретная имплементация — Этап 2 (macOS), R4 для Windows.
#[async_trait]
pub trait AudioCapture: Send + Sync {
    async fn start(&self) -> Result<(), CaptureError>;
    async fn stop(&self) -> Result<CaptureResult, CaptureError>;
}

#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub mic_wav: std::path::PathBuf,
    pub system_wav: std::path::PathBuf,
    pub duration_sec: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not implemented on this platform")]
    NotImplemented,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod permissions;
#[cfg(target_os = "macos")]
pub use macos::MacOsCoreAudioCapture;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsWasapiCapture;

// [B16 audit P2] Linux build guard: explicit early fail с понятным сообщением.
// Без guard сборка падает позже в callsite не linked AudioCapture impl.
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
compile_error!(
    "Wotold не собирается на этой платформе — поддержка только macOS (Linux/прочие OS \
     не реализованы, см. R4 паспорта). Используй macOS 14+ для разработки."
);
