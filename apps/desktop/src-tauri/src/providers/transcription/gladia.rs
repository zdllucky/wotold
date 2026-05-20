use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{multipart, StatusCode};
use serde::Deserialize;
use serde_json::json;

use super::{
    DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
    TranscriptionProvider,
};
use crate::providers::ProviderMode;

pub const GLADIA_DIRECT_URL: &str = "https://api.gladia.io/v2";
const BYO_POLL_INTERVAL: Duration = Duration::from_secs(1);
const BYO_POLL_MAX_SECS: u64 = 300;

/// GladiaProvider — fallback STT (M2.2). Два пути:
/// - Managed: 3-step через прокси (тот же flow что у SonioxProvider, но
///   с opts.provider = "gladia").
/// - BYO: POST {gladia}/v2/upload (multipart) → POST /v2/pre-recorded
///   {audio_url, diarization} → polling result_url до status=done.
pub struct GladiaProvider {
    pub mode: ProviderMode,
    pub direct_url: String,
    http: reqwest::Client,
}

impl GladiaProvider {
    pub fn new(mode: ProviderMode) -> Self {
        Self {
            mode,
            direct_url: GLADIA_DIRECT_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl TranscriptionProvider for GladiaProvider {
    async fn transcribe(
        &self,
        audio_path: &Path,
        opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError> {
        match &self.mode {
            ProviderMode::Managed {
                proxy_base_url,
                device_id,
            } => transcribe_managed(&self.http, proxy_base_url, device_id, audio_path, &opts).await,
            ProviderMode::Byo { api_key } => {
                transcribe_byo(&self.http, &self.direct_url, api_key, audio_path, &opts).await
            }
        }
    }
}

async fn transcribe_managed(
    http: &reqwest::Client,
    proxy_base_url: &str,
    device_id: &str,
    audio_path: &Path,
    opts: &TranscriptionOpts,
) -> Result<DiarizedTranscript, TranscriptionError> {
    // [B16] Делегируем shared helper — устраняет ~95 строк дубликации с soniox.rs.
    super::proxy_managed::transcribe_via_proxy(
        http,
        proxy_base_url,
        device_id,
        audio_path,
        "gladia",
        opts,
    )
    .await
}

#[derive(Deserialize)]
struct UploadResp {
    audio_url: String,
}

#[derive(Deserialize)]
struct CreateResp {
    #[allow(dead_code)]
    id: String,
    result_url: String,
}

#[derive(Deserialize)]
struct PollResp {
    status: String,
    result: Option<GladiaResult>,
    error_code: Option<String>,
}

#[derive(Deserialize)]
struct GladiaResult {
    metadata: Option<GladiaMetadata>,
    transcription: Option<GladiaTranscription>,
}

#[derive(Deserialize)]
struct GladiaMetadata {
    audio_duration: Option<f64>,
}

#[derive(Deserialize)]
struct GladiaTranscription {
    languages: Option<Vec<String>>,
    utterances: Option<Vec<GladiaUtterance>>,
}

#[derive(Deserialize)]
struct GladiaUtterance {
    start: f64,
    end: f64,
    text: String,
    speaker: Option<i64>,
    confidence: Option<f64>,
}

async fn transcribe_byo(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    audio_path: &Path,
    opts: &TranscriptionOpts,
) -> Result<DiarizedTranscript, TranscriptionError> {
    let base = base_url.trim_end_matches('/');

    // 1. Upload файла в /v2/upload (multipart, field name "audio").
    let bytes = tokio::fs::read(audio_path)
        .await
        .map_err(|e| TranscriptionError::Provider(format!("read audio file: {e}")))?;
    let part = multipart::Part::bytes(bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| TranscriptionError::Provider(e.to_string()))?;
    let form = multipart::Form::new().part("audio", part);

    let audio_url = http
        .post(format!("{base}/upload"))
        .header("x-gladia-key", api_key)
        .multipart(form)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(gladia_status)?
        .json::<UploadResp>()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("upload parse: {e}")))?
        .audio_url;

    // 2. POST /v2/pre-recorded.
    let mut body = json!({
        "audio_url": audio_url,
        "diarization": true,
    });
    if opts.lang != "auto" {
        body["language_config"] = json!({
            "languages": [opts.lang],
            "code_switching": true,
        });
    }

    let result_url = http
        .post(format!("{base}/pre-recorded"))
        .header("x-gladia-key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(gladia_status)?
        .json::<CreateResp>()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("pre-recorded parse: {e}")))?
        .result_url;

    // 3. Polling result_url до status=done.
    let started = std::time::Instant::now();
    loop {
        if started.elapsed().as_secs() > BYO_POLL_MAX_SECS {
            return Err(TranscriptionError::Provider(format!(
                "gladia poll timeout ({BYO_POLL_MAX_SECS}s)"
            )));
        }
        tokio::time::sleep(BYO_POLL_INTERVAL).await;

        let resp = http
            .get(&result_url)
            .header("x-gladia-key", api_key)
            .send()
            .await
            .map_err(net)?
            .error_for_status()
            .map_err(gladia_status)?
            .json::<PollResp>()
            .await
            .map_err(|e| TranscriptionError::Provider(format!("poll parse: {e}")))?;

        match resp.status.as_str() {
            "done" => {
                return Ok(normalize_gladia(resp));
            }
            "error" => {
                return Err(TranscriptionError::Provider(format!(
                    "gladia error_code: {}",
                    resp.error_code.as_deref().unwrap_or("unknown")
                )));
            }
            _ => {}
        }
    }
}

fn normalize_gladia(r: PollResp) -> DiarizedTranscript {
    let result = r.result;
    let transcription = result.as_ref().and_then(|x| x.transcription.as_ref());

    let segments: Vec<TranscriptSegment> = transcription
        .and_then(|t| t.utterances.as_ref())
        .map(|us| {
            us.iter()
                .map(|u| TranscriptSegment {
                    start: u.start,
                    end: u.end,
                    text: u.text.clone(),
                    speaker_tag: match u.speaker {
                        Some(s) => format!("Speaker {s}"),
                        None => "Speaker 0".to_string(),
                    },
                    confidence: u.confidence,
                })
                .collect()
        })
        .unwrap_or_default();

    DiarizedTranscript {
        version: 1,
        provider: "gladia".to_string(),
        lang_detected: transcription
            .and_then(|t| t.languages.as_ref())
            .and_then(|ls| ls.first().cloned()),
        duration_sec: result
            .as_ref()
            .and_then(|x| x.metadata.as_ref())
            .and_then(|m| m.audio_duration)
            .unwrap_or(0.0),
        segments,
    }
}

fn net(e: reqwest::Error) -> TranscriptionError {
    TranscriptionError::Network(e.to_string())
}

fn gladia_status(e: reqwest::Error) -> TranscriptionError {
    if let Some(status) = e.status() {
        if status == StatusCode::UNAUTHORIZED {
            return TranscriptionError::Auth("gladia 401".into());
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return TranscriptionError::QuotaExceeded;
        }
        return TranscriptionError::Provider(format!("gladia {status}"));
    }
    TranscriptionError::Network(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn dummy_wav() -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("temp file");
        f.write_all(b"RIFF\x24\x00\x00\x00WAVE").unwrap();
        f
    }

    fn opts() -> TranscriptionOpts {
        TranscriptionOpts {
            lang: "auto".into(),
            diarization: true,
        }
    }

    #[tokio::test]
    async fn managed_happy_path_proxies_with_gladia_provider_label() {
        let server = MockServer::start_async().await;

        server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/stt/staging-url");
                then.status(200).json_body(json!({
                    "r2Key": "k",
                    "uploadUrl": format!("{}/r2/PUT/k", server.base_url()),
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(PUT).path("/r2/PUT/k");
                then.status(200);
            })
            .await;
        let stt_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/stt")
                    .json_body_partial(r#"{"opts":{"provider":"gladia"}}"#);
                then.status(200).json_body(json!({
                    "ok": true,
                    "transcript": {
                        "version": 1,
                        "langDetected": "ru",
                        "durationSec": 8.0,
                        "provider": "gladia",
                        "segments": []
                    }
                }));
            })
            .await;

        let provider = GladiaProvider {
            mode: ProviderMode::Managed {
                proxy_base_url: server.base_url(),
                device_id: "dev-x".into(),
            },
            direct_url: GLADIA_DIRECT_URL.into(),
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let res = provider.transcribe(wav.path(), opts()).await.unwrap();
        stt_mock.assert_async().await;
        assert_eq!(res.provider, "gladia");
        assert_eq!(res.duration_sec, 8.0);
    }

    #[tokio::test]
    async fn byo_uploads_then_polls_and_normalizes() {
        let server = MockServer::start_async().await;
        let audio_url = format!("{}/file/abc", server.base_url());
        let result_url = format!("{}/v2/pre-recorded/job-1", server.base_url());

        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/upload")
                    .header_exists("x-gladia-key");
                then.status(200).json_body(json!({"audio_url": audio_url}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/pre-recorded");
                then.status(200).json_body(json!({
                    "id": "job-1",
                    "result_url": result_url
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/pre-recorded/job-1");
                then.status(200).json_body(json!({
                    "status": "done",
                    "result": {
                        "metadata": {"audio_duration": 5.5},
                        "transcription": {
                            "languages": ["en"],
                            "utterances": [
                                {"start": 0.0, "end": 2.0, "text": "Hello", "speaker": 0, "confidence": 0.9},
                                {"start": 2.5, "end": 5.5, "text": "Hi back", "speaker": 1}
                            ]
                        }
                    }
                }));
            })
            .await;

        let provider = GladiaProvider {
            mode: ProviderMode::Byo {
                api_key: "gl-test".into(),
            },
            direct_url: server.base_url(),
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let res = provider.transcribe(wav.path(), opts()).await.unwrap();
        assert_eq!(res.provider, "gladia");
        assert_eq!(res.lang_detected.as_deref(), Some("en"));
        assert_eq!(res.duration_sec, 5.5);
        assert_eq!(res.segments.len(), 2);
        assert_eq!(res.segments[0].speaker_tag, "Speaker 0");
        assert_eq!(res.segments[1].speaker_tag, "Speaker 1");
    }

    #[tokio::test]
    async fn byo_error_status_propagates() {
        let server = MockServer::start_async().await;
        let audio_url = format!("{}/file/x", server.base_url());
        let result_url = format!("{}/v2/pre-recorded/job-x", server.base_url());

        server
            .mock_async(|when, then| {
                when.method(POST).path("/upload");
                then.status(200).json_body(json!({"audio_url": audio_url}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/pre-recorded");
                then.status(200).json_body(json!({
                    "id": "job-x", "result_url": result_url
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/pre-recorded/job-x");
                then.status(200).json_body(json!({
                    "status": "error",
                    "error_code": "unsupported_format"
                }));
            })
            .await;

        let provider = GladiaProvider {
            mode: ProviderMode::Byo {
                api_key: "gl".into(),
            },
            direct_url: server.base_url(),
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let err = provider.transcribe(wav.path(), opts()).await.unwrap_err();
        match err {
            TranscriptionError::Provider(msg) => {
                assert!(msg.contains("unsupported_format"), "msg: {msg}");
            }
            other => panic!("expected Provider, got {other:?}"),
        }
    }
}
