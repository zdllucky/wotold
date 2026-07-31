use async_trait::async_trait;

// [B16] Trait + types — это спецификация поверх platform-specific impl
// (см. macos.rs / windows.rs). Production-код вызывает их через конкретный
// struct, но trait + CaptureResult+CaptureError полезны для будущего
// Windows-импл'а (R4) и тестов с mock-impl. allow ограничен этим scope.
#[allow(dead_code)]
mod scaffold {
    use super::*;

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
}

#[allow(dead_code)]
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

#[allow(unused_imports)]
pub use scaffold::{AudioCapture, CaptureResult};

#[cfg(target_os = "macos")]
pub mod call_detect;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod permissions;

// [B3.2] WAV chunk reader — используется в pipeline для извлечения voice
// embedding chunks per speaker_tag. Платформо-независимый (hound crate).
pub mod wav_chunker;

// [M13.1.1] Silence-aware cut detector — pure-функция поиска тихого
// сегмента в RMS-buffer для chunked transcription. Без platform deps.
pub mod silence_detector;

// [T4] Подрезка тихого хвоста WAV на авто-стопе. Platform deps нет (hound).
pub mod wav_trim;

// [T1/T2] Silence watch — решение «в записи тишина»: подсказать стоп и
// остановить самим с подрезкой хвоста (R15). Ядро чистое, обёртка на
// каналах; platform deps нет, поэтому тесты бегут на любой платформе.
pub mod silence_watch;

#[cfg(target_os = "windows")]
pub mod windows;

// [B16 audit P2] Linux build guard: explicit early fail с понятным сообщением.
// Без guard сборка падает позже в callsite не linked AudioCapture impl.
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
compile_error!(
    "Wotold не собирается на этой платформе — поддержка только macOS (Linux/прочие OS \
     не реализованы, см. R4 паспорта). Используй macOS 14+ для разработки."
);
