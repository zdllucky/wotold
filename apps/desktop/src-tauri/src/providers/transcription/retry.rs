use std::future::Future;
use std::path::Path;
use std::time::Duration;

use super::{DiarizedTranscript, TranscriptionError, TranscriptionOpts, TranscriptionProvider};

/// Конфиг exp backoff. Дефолт — 3 попытки, 500ms → 1500ms → 4500ms (3x).
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub multiplier: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            multiplier: 3,
        }
    }
}

/// Retry-able классификация. ECC: только transient ошибки достойны retry —
/// auth и quota_exceeded permanent (R7).
fn is_retryable(err: &TranscriptionError) -> bool {
    matches!(err, TranscriptionError::Network(_))
}

/// Запускает `op` с exp backoff на retry-able ошибках. Sleep инжектится для тестов
/// (стандартный `tokio::time::sleep` в production).
pub async fn with_backoff<F, Fut, T, Sleep, SleepFut>(
    cfg: RetryConfig,
    sleep: Sleep,
    mut op: F,
) -> Result<T, TranscriptionError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, TranscriptionError>>,
    Sleep: Fn(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
{
    let mut last_err: Option<TranscriptionError> = None;
    let mut delay = cfg.base_delay;
    for attempt in 1..=cfg.max_attempts {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                if !is_retryable(&e) || attempt == cfg.max_attempts {
                    return Err(e);
                }
                log::warn!("transcription retry {}/{}: {e}", attempt, cfg.max_attempts);
                sleep(delay).await;
                delay *= cfg.multiplier;
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or(TranscriptionError::Provider("retry loop exhausted".into())))
}

/// Auto-fallback (#23): пробует каждый провайдер по очереди с retry-backoff.
/// Network errors → retry внутри провайдера. Permanent (auth/quota/provider) ИЛИ
/// retry-exhausted → переход к следующему провайдеру. Auth/quota на одном провайдере
/// не означает что на другом тоже — у них разные ключи и квоты.
pub async fn transcribe_with_fallback(
    providers: &[Box<dyn TranscriptionProvider>],
    audio: &Path,
    opts: TranscriptionOpts,
    cfg: RetryConfig,
) -> Result<DiarizedTranscript, TranscriptionError> {
    if providers.is_empty() {
        return Err(TranscriptionError::Provider(
            "no providers configured".into(),
        ));
    }
    let mut last_err = None;
    for provider in providers {
        let result = with_backoff(
            cfg,
            |d| async move { tokio::time::sleep(d).await },
            || async { provider.transcribe(audio, opts.clone()).await },
        )
        .await;
        match result {
            Ok(t) => return Ok(t),
            Err(e) => {
                log::warn!("provider failed (will try next if any): {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or(TranscriptionError::Provider("all providers failed".into())))
}

/// UX-readable причина для `calls.failed_reason`. Технические детали в логах,
/// пользователь видит коротко и понятно.
pub fn failure_reason(err: &TranscriptionError) -> String {
    match err {
        TranscriptionError::Auth(_) => {
            "Авторизация провайдера STT не прошла. Проверь BYO-ключи в Настройках.".into()
        }
        TranscriptionError::QuotaExceeded => {
            "Квота STT исчерпана. Подожди до следующих суток или переключись на BYO.".into()
        }
        TranscriptionError::Network(_) => {
            "Сетевая ошибка при обращении к STT — все retries исчерпаны.".into()
        }
        TranscriptionError::Provider(_) => {
            "STT-провайдер вернул ошибку. Можно повторить транскрипцию вручную из деталей звонка."
                .into()
        }
        TranscriptionError::NotImplemented => "STT не реализован для этой платформы.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    async fn noop_sleep(_: Duration) {}

    #[tokio::test]
    async fn returns_value_on_first_success() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let result = with_backoff(RetryConfig::default(), noop_sleep, || async {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            Ok::<_, TranscriptionError>(42)
        })
        .await
        .unwrap();
        assert_eq!(result, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_network_errors_until_success() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let result = with_backoff(RetryConfig::default(), noop_sleep, || {
            let n = calls_ref.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err(TranscriptionError::Network("transient".into()))
                } else {
                    Ok::<_, TranscriptionError>("ok")
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(result, "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn fails_fast_on_non_retryable_auth() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let err = with_backoff(RetryConfig::default(), noop_sleep, || {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TranscriptionError::Auth("bad key".into())) }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::Auth(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fails_fast_on_quota_exceeded() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let err = with_backoff(RetryConfig::default(), noop_sleep, || {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TranscriptionError::QuotaExceeded) }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::QuotaExceeded));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn exhausts_max_attempts_on_persistent_network_error() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let cfg = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            multiplier: 2,
        };
        let err = with_backoff(cfg, noop_sleep, || {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TranscriptionError::Network("down".into())) }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::Network(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    // ----- fallback tests -----

    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::Mutex;

    use super::super::{DiarizedTranscript, TranscriptionOpts, TranscriptionProvider};

    struct MockProvider {
        name: &'static str,
        results: Mutex<Vec<Result<DiarizedTranscript, TranscriptionError>>>,
        calls: AtomicU32,
    }

    impl MockProvider {
        fn ok(name: &'static str) -> Self {
            Self {
                name,
                results: Mutex::new(vec![Ok(DiarizedTranscript {
                    version: 1,
                    lang_detected: Some("en".into()),
                    duration_sec: 1.0,
                    provider: name.into(),
                    segments: vec![],
                })]),
                calls: AtomicU32::new(0),
            }
        }

        fn err(name: &'static str, err: TranscriptionError) -> Self {
            Self {
                name,
                results: Mutex::new(vec![Err(err)]),
                calls: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl TranscriptionProvider for MockProvider {
        async fn transcribe(
            &self,
            _audio_path: &Path,
            _opts: TranscriptionOpts,
        ) -> Result<DiarizedTranscript, TranscriptionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // Если в очереди один результат — клонируем (через consume).
            let mut q = self.results.lock().unwrap();
            if q.len() == 1 {
                // Multi-call: возвращаем тот же результат снова (через клон если Ok).
                match &q[0] {
                    Ok(t) => Ok(t.clone()),
                    Err(_) => {
                        let e = q.remove(0);
                        q.push(match &e {
                            Err(TranscriptionError::Network(s)) => {
                                Err(TranscriptionError::Network(s.clone()))
                            }
                            Err(TranscriptionError::Auth(s)) => {
                                Err(TranscriptionError::Auth(s.clone()))
                            }
                            Err(TranscriptionError::Provider(s)) => {
                                Err(TranscriptionError::Provider(s.clone()))
                            }
                            Err(TranscriptionError::QuotaExceeded) => {
                                Err(TranscriptionError::QuotaExceeded)
                            }
                            Err(TranscriptionError::NotImplemented) => {
                                Err(TranscriptionError::NotImplemented)
                            }
                            Ok(_) => unreachable!(),
                        });
                        e
                    }
                }
            } else {
                q.remove(0)
            }
        }
    }

    fn fast_cfg() -> RetryConfig {
        RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            multiplier: 1,
        }
    }

    #[tokio::test]
    async fn fallback_returns_first_success() {
        let providers: Vec<Box<dyn TranscriptionProvider>> = vec![
            Box::new(MockProvider::ok("soniox")),
            Box::new(MockProvider::ok("gladia")),
        ];
        let t = transcribe_with_fallback(
            &providers,
            Path::new("/tmp/x.wav"),
            TranscriptionOpts {
                lang: "auto".into(),
                diarization: true,
            },
            fast_cfg(),
        )
        .await
        .unwrap();
        assert_eq!(t.provider, "soniox");
    }

    #[tokio::test]
    async fn fallback_switches_after_primary_fails() {
        let primary = Box::new(MockProvider::err(
            "soniox",
            TranscriptionError::Auth("bad key".into()),
        ));
        let secondary = Box::new(MockProvider::ok("gladia"));
        let providers: Vec<Box<dyn TranscriptionProvider>> = vec![primary, secondary];
        let t = transcribe_with_fallback(
            &providers,
            Path::new("/tmp/x.wav"),
            TranscriptionOpts {
                lang: "auto".into(),
                diarization: true,
            },
            fast_cfg(),
        )
        .await
        .unwrap();
        assert_eq!(t.provider, "gladia");
    }

    #[tokio::test]
    async fn fallback_returns_last_error_when_all_fail() {
        let providers: Vec<Box<dyn TranscriptionProvider>> = vec![
            Box::new(MockProvider::err(
                "soniox",
                TranscriptionError::Network("down".into()),
            )),
            Box::new(MockProvider::err(
                "gladia",
                TranscriptionError::QuotaExceeded,
            )),
        ];
        let err = transcribe_with_fallback(
            &providers,
            Path::new("/tmp/x.wav"),
            TranscriptionOpts {
                lang: "auto".into(),
                diarization: true,
            },
            fast_cfg(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::QuotaExceeded));
    }

    #[tokio::test]
    async fn fallback_empty_providers_returns_provider_error() {
        let providers: Vec<Box<dyn TranscriptionProvider>> = vec![];
        let err = transcribe_with_fallback(
            &providers,
            Path::new("/tmp/x.wav"),
            TranscriptionOpts {
                lang: "auto".into(),
                diarization: true,
            },
            fast_cfg(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::Provider(_)));
    }

    #[test]
    fn failure_reason_handles_all_variants() {
        assert!(failure_reason(&TranscriptionError::Auth("x".into())).contains("Авторизация"));
        assert!(failure_reason(&TranscriptionError::QuotaExceeded).contains("Квота"));
        assert!(failure_reason(&TranscriptionError::Network("x".into())).contains("Сетевая"));
        assert!(failure_reason(&TranscriptionError::Provider("x".into())).contains("провайдер"));
        assert!(failure_reason(&TranscriptionError::NotImplemented).contains("не реализован"));
    }

    #[tokio::test]
    async fn does_not_retry_provider_error() {
        // Provider error — например parsing — повторять бессмысленно.
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let err = with_backoff(RetryConfig::default(), noop_sleep, || {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TranscriptionError::Provider("malformed json".into())) }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, TranscriptionError::Provider(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
