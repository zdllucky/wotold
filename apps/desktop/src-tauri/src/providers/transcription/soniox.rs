use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{multipart, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
    TranscriptionProvider,
};
use crate::providers::ProviderMode;

pub const SONIOX_DIRECT_URL: &str = "https://api.soniox.com/v1";
const DEFAULT_MODEL: &str = "stt-async-preview";
const BYO_POLL_INTERVAL: Duration = Duration::from_secs(1);
const BYO_POLL_MAX_SECS: u64 = 300; // 5 минут на запись/обработку
const PROXY_STAGING_PATH: &str = "/v1/stt/staging-url";
const PROXY_STT_PATH: &str = "/v1/stt";

/// SonioxProvider — primary STT (M2.2). Два пути:
/// - Managed: POST {proxy}/v1/stt/staging-url → PUT в R2 → POST {proxy}/v1/stt
///   Прокси внутри ходит к Soniox, мы получаем готовый DiarizedTranscript.
/// - BYO: POST {soniox}/v1/files (multipart) → POST /v1/transcriptions →
///   polling GET /v1/transcriptions/{id} → GET /v1/transcriptions/{id}/transcript
pub struct SonioxProvider {
    pub mode: ProviderMode,
    pub direct_url: String,
    pub model: Option<String>,
    http: reqwest::Client,
}

impl SonioxProvider {
    pub fn new(mode: ProviderMode) -> Self {
        Self {
            mode,
            direct_url: SONIOX_DIRECT_URL.to_string(),
            model: None,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl TranscriptionProvider for SonioxProvider {
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
                transcribe_byo(
                    &self.http,
                    &self.direct_url,
                    api_key,
                    audio_path,
                    &opts,
                    self.model.as_deref(),
                )
                .await
            }
        }
    }
}

#[derive(Deserialize)]
struct StagingUrlResp {
    #[serde(rename = "r2Key")]
    r2_key: String,
    #[serde(rename = "uploadUrl")]
    upload_url: String,
    headers: Option<std::collections::HashMap<String, String>>,
}

async fn transcribe_managed(
    http: &reqwest::Client,
    proxy_base_url: &str,
    device_id: &str,
    audio_path: &Path,
    opts: &TranscriptionOpts,
) -> Result<DiarizedTranscript, TranscriptionError> {
    let base = proxy_base_url.trim_end_matches('/');

    // 1. Запрашиваем presigned PUT URL.
    let staging: StagingUrlResp = http
        .post(format!("{base}{PROXY_STAGING_PATH}"))
        .header("x-device-id", device_id)
        .json(&json!({"contentType": "audio/wav"}))
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(proxy_status)?
        .json()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("proxy staging-url parse: {e}")))?;

    // 2. PUT файла напрямую в R2 (мимо прокси, R8).
    let bytes = tokio::fs::read(audio_path)
        .await
        .map_err(|e| TranscriptionError::Provider(format!("read audio file: {e}")))?;
    let mut put = http.put(&staging.upload_url).body(bytes);
    if let Some(headers) = &staging.headers {
        for (k, v) in headers {
            put = put.header(k, v);
        }
    }
    let put_resp = put.send().await.map_err(net)?;
    if !put_resp.status().is_success() {
        return Err(TranscriptionError::Provider(format!(
            "R2 upload {}",
            put_resp.status()
        )));
    }

    // 3. POST /v1/stt — прокси сходит к Soniox, поллит, нормализует.
    let stt_resp = http
        .post(format!("{base}{PROXY_STT_PATH}"))
        .header("x-device-id", device_id)
        .json(&json!({
            "r2Key": staging.r2_key,
            "opts": {
                "provider": "soniox",
                "diarization": true,
                "lang": opts.lang,
            },
        }))
        .send()
        .await
        .map_err(net)?;

    let status = stt_resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(TranscriptionError::QuotaExceeded);
    }
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(TranscriptionError::Auth(format!("proxy {status}")));
    }

    let body: Value = stt_resp
        .json()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("proxy stt parse: {e}")))?;

    if body.get("ok") == Some(&Value::Bool(true)) {
        let transcript = body
            .get("transcript")
            .cloned()
            .ok_or_else(|| TranscriptionError::Provider("missing transcript in response".into()))?;
        serde_json::from_value(transcript)
            .map_err(|e| TranscriptionError::Provider(format!("transcript shape: {e}")))
    } else {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        match body.get("code").and_then(Value::as_str) {
            Some("quota_exceeded") => Err(TranscriptionError::QuotaExceeded),
            Some("invalid_device_id") => Err(TranscriptionError::Auth(message)),
            _ => Err(TranscriptionError::Provider(message)),
        }
    }
}

#[derive(Deserialize)]
struct IdResp {
    id: String,
}

#[derive(Deserialize)]
struct StatusResp {
    status: String,
}

#[derive(Deserialize, Default)]
struct SonioxToken {
    text: String,
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    end_ms: Option<u64>,
    #[serde(default)]
    speaker: Option<i64>,
    #[serde(default)]
    confidence: Option<f64>,
}

#[derive(Deserialize, Default)]
struct SonioxTranscript {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    tokens: Vec<SonioxToken>,
}

async fn transcribe_byo(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    audio_path: &Path,
    opts: &TranscriptionOpts,
    model: Option<&str>,
) -> Result<DiarizedTranscript, TranscriptionError> {
    let base = base_url.trim_end_matches('/');

    // 1. Multipart upload в /v1/files.
    let bytes = tokio::fs::read(audio_path)
        .await
        .map_err(|e| TranscriptionError::Provider(format!("read audio file: {e}")))?;
    let part = multipart::Part::bytes(bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| TranscriptionError::Provider(e.to_string()))?;
    let form = multipart::Form::new().part("file", part);

    let file_id = http
        .post(format!("{base}/files"))
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(soniox_status)?
        .json::<IdResp>()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("files parse: {e}")))?
        .id;

    // 2. POST /v1/transcriptions.
    let mut body = json!({
        "file_id": file_id,
        "model": model.unwrap_or(DEFAULT_MODEL),
        "enable_speaker_diarization": true,
        "enable_language_identification": opts.lang == "auto",
    });
    if opts.lang != "auto" {
        body["language_hints"] = json!([opts.lang]);
        body["language_hints_strict"] = json!(true);
    }

    let job_id = http
        .post(format!("{base}/transcriptions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(soniox_status)?
        .json::<IdResp>()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("transcriptions parse: {e}")))?
        .id;

    // 3. Polling до completed/failed/timeout.
    let started = std::time::Instant::now();
    loop {
        if started.elapsed().as_secs() > BYO_POLL_MAX_SECS {
            return Err(TranscriptionError::Provider(format!(
                "soniox poll timeout ({BYO_POLL_MAX_SECS}s)"
            )));
        }
        tokio::time::sleep(BYO_POLL_INTERVAL).await;

        let status: StatusResp = http
            .get(format!("{base}/transcriptions/{job_id}"))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(net)?
            .error_for_status()
            .map_err(soniox_status)?
            .json()
            .await
            .map_err(|e| TranscriptionError::Provider(format!("status parse: {e}")))?;

        match status.status.as_str() {
            "completed" => break,
            "failed" => return Err(TranscriptionError::Provider("soniox job failed".into())),
            _ => {}
        }
    }

    // 4. Забираем transcript.
    let transcript: SonioxTranscript = http
        .get(format!("{base}/transcriptions/{job_id}/transcript"))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(soniox_status)?
        .json()
        .await
        .map_err(|e| TranscriptionError::Provider(format!("transcript parse: {e}")))?;

    Ok(normalize_soniox(transcript))
}

/// Группирует подряд идущие токены одного спикера в TranscriptSegment.
fn normalize_soniox(t: SonioxTranscript) -> DiarizedTranscript {
    let mut segments: Vec<TranscriptSegment> = Vec::new();

    for tok in t.tokens {
        let speaker_tag = match tok.speaker {
            Some(s) => format!("Speaker {s}"),
            None => "Speaker 0".to_string(),
        };
        let start = (tok.start_ms.unwrap_or(0) as f64) / 1000.0;
        let end = (tok.end_ms.unwrap_or(tok.start_ms.unwrap_or(0)) as f64) / 1000.0;

        if let Some(last) = segments.last_mut() {
            if last.speaker_tag == speaker_tag {
                last.text.push_str(&tok.text);
                last.end = end;
                continue;
            }
        }
        segments.push(TranscriptSegment {
            start,
            end,
            text: tok.text,
            speaker_tag,
            confidence: tok.confidence,
        });
    }

    DiarizedTranscript {
        version: 1,
        provider: "soniox".to_string(),
        lang_detected: t.language,
        duration_sec: (t.duration_ms.unwrap_or(0) as f64) / 1000.0,
        segments,
    }
}

fn net(e: reqwest::Error) -> TranscriptionError {
    TranscriptionError::Network(e.to_string())
}

fn soniox_status(e: reqwest::Error) -> TranscriptionError {
    if let Some(status) = e.status() {
        if status == StatusCode::UNAUTHORIZED {
            return TranscriptionError::Auth("soniox 401".into());
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return TranscriptionError::QuotaExceeded;
        }
        return TranscriptionError::Provider(format!("soniox {status}"));
    }
    TranscriptionError::Network(e.to_string())
}

fn proxy_status(e: reqwest::Error) -> TranscriptionError {
    if let Some(status) = e.status() {
        if status == StatusCode::TOO_MANY_REQUESTS {
            return TranscriptionError::QuotaExceeded;
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return TranscriptionError::Auth(format!("proxy {status}"));
        }
        return TranscriptionError::Provider(format!("proxy {status}"));
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
        // Минимальный валидный WAV-заголовок + 1 сэмпл, чтобы reqwest имел что отправить.
        // Содержимое нерелевантно — мок всё равно возвращает фиксированный transcript.
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
    async fn managed_happy_path_3_steps() {
        let server = MockServer::start_async().await;

        // /v1/stt/staging-url
        let staging_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/stt/staging-url")
                    .header("x-device-id", "dev-1");
                then.status(200).json_body(json!({
                    "r2Key": "stt/dev-1/abc",
                    "uploadUrl": format!("{}/r2/PUT/abc", server.base_url()),
                    "headers": {"content-type": "audio/wav"},
                    "expiresAt": "2026-01-01T00:00:00Z",
                }));
            })
            .await;

        // R2 PUT (мокаем тот же сервер)
        let r2_mock = server
            .mock_async(|when, then| {
                when.method(PUT).path("/r2/PUT/abc");
                then.status(200);
            })
            .await;

        // /v1/stt
        let stt_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/stt")
                    .header("x-device-id", "dev-1");
                then.status(200).json_body(json!({
                    "ok": true,
                    "transcript": {
                        "version": 1,
                        "lang_detected": "en",
                        "duration_sec": 12.5,
                        "provider": "soniox",
                        "segments": [
                            {"start": 0.0, "end": 5.0, "text": "Hello", "speaker_tag": "Speaker 0"},
                            {"start": 5.0, "end": 12.5, "text": "Hi", "speaker_tag": "Speaker 1"}
                        ]
                    }
                }));
            })
            .await;

        let provider = SonioxProvider {
            mode: ProviderMode::Managed {
                proxy_base_url: server.base_url(),
                device_id: "dev-1".into(),
            },
            direct_url: SONIOX_DIRECT_URL.into(),
            model: None,
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let result = provider
            .transcribe(wav.path(), opts())
            .await
            .expect("managed should succeed");

        staging_mock.assert_async().await;
        r2_mock.assert_async().await;
        stt_mock.assert_async().await;

        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.provider, "soniox");
        assert_eq!(result.duration_sec, 12.5);
    }

    #[tokio::test]
    async fn managed_quota_exceeded_maps() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/stt/staging-url");
                then.status(200).json_body(json!({
                    "r2Key": "x", "uploadUrl": format!("{}/r2/PUT/x", server.base_url())
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(PUT).path("/r2/PUT/x");
                then.status(200);
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/stt");
                then.status(429).json_body(json!({
                    "ok": false, "code": "quota_exceeded", "message": "daily limit hit"
                }));
            })
            .await;

        let provider = SonioxProvider {
            mode: ProviderMode::Managed {
                proxy_base_url: server.base_url(),
                device_id: "dev-1".into(),
            },
            direct_url: SONIOX_DIRECT_URL.into(),
            model: None,
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let err = provider.transcribe(wav.path(), opts()).await.unwrap_err();
        assert!(
            matches!(err, TranscriptionError::QuotaExceeded),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn byo_full_flow_normalizes_tokens_by_speaker() {
        let server = MockServer::start_async().await;

        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/files")
                    .header_exists("authorization");
                then.status(200).json_body(json!({"id": "file-1"}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/transcriptions");
                then.status(200).json_body(json!({"id": "job-1"}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/transcriptions/job-1");
                then.status(200).json_body(json!({"status": "completed"}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/transcriptions/job-1/transcript");
                then.status(200).json_body(json!({
                    "language": "ru",
                    "duration_ms": 3500,
                    "tokens": [
                        {"text": "Привет", "start_ms": 0,    "end_ms": 500,  "speaker": 0},
                        {"text": " мир",    "start_ms": 500,  "end_ms": 1000, "speaker": 0},
                        {"text": "пока",    "start_ms": 1500, "end_ms": 2000, "speaker": 1}
                    ]
                }));
            })
            .await;

        let provider = SonioxProvider {
            mode: ProviderMode::Byo {
                api_key: "sk-test".into(),
            },
            direct_url: server.base_url(),
            model: None,
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let result = provider
            .transcribe(wav.path(), opts())
            .await
            .expect("byo should succeed");

        assert_eq!(result.lang_detected.as_deref(), Some("ru"));
        assert_eq!(result.duration_sec, 3.5);
        // Два подряд токена Speaker 0 → один сегмент. Третий Speaker 1 → второй сегмент.
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].text, "Привет мир");
        assert_eq!(result.segments[0].speaker_tag, "Speaker 0");
        assert_eq!(result.segments[1].text, "пока");
        assert_eq!(result.segments[1].speaker_tag, "Speaker 1");
    }

    #[tokio::test]
    async fn byo_unauthorized_maps_to_auth_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/files");
                then.status(401);
            })
            .await;

        let provider = SonioxProvider {
            mode: ProviderMode::Byo {
                api_key: "sk-bad".into(),
            },
            direct_url: server.base_url(),
            model: None,
            http: reqwest::Client::new(),
        };

        let wav = dummy_wav();
        let err = provider.transcribe(wav.path(), opts()).await.unwrap_err();
        assert!(matches!(err, TranscriptionError::Auth(_)), "got {err:?}");
    }
}
