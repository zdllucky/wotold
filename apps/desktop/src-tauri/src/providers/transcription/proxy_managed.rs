// [B16] Общий managed-stt helper для Soniox и Gladia (раньше код был
// дублирован построчно в transcribe_managed обоих провайдеров).
//
// Flow:
//   1. POST {base}/v1/stt/staging-url  → presigned PUT URL + r2_key
//   2. PUT audio_file → R2 staging
//   3. POST {base}/v1/stt с r2Key и opts.provider → DiarizedTranscript
//
// Errors mapping в TranscriptionError единые для обоих.

use std::path::Path;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::providers::transcription::{DiarizedTranscript, TranscriptionError, TranscriptionOpts};

const PROXY_STAGING_PATH: &str = "/v1/stt/staging-url";
const PROXY_STT_PATH: &str = "/v1/stt";

#[derive(Deserialize)]
struct StagingUrlResp {
    #[serde(rename = "uploadUrl")]
    upload_url: String,
    #[serde(rename = "r2Key")]
    r2_key: String,
    headers: Option<std::collections::HashMap<String, String>>,
}

/// Запросить presigned R2 URL → upload audio → запросить STT через proxy.
/// `provider_label` — `'soniox'` / `'gladia'`, передаётся прокси для дальнейшего routing.
pub async fn transcribe_via_proxy(
    http: &reqwest::Client,
    proxy_base_url: &str,
    device_id: &str,
    audio_path: &Path,
    provider_label: &str,
    opts: &TranscriptionOpts,
) -> Result<DiarizedTranscript, TranscriptionError> {
    let base = proxy_base_url.trim_end_matches('/');

    // 1. Запросить presigned URL.
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

    // 2. Upload audio в R2.
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

    // 3. Запросить STT.
    let stt_resp = http
        .post(format!("{base}{PROXY_STT_PATH}"))
        .header("x-device-id", device_id)
        .json(&json!({
            "r2Key": staging.r2_key,
            "opts": {
                "provider": provider_label,
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

fn net(e: reqwest::Error) -> TranscriptionError {
    TranscriptionError::Network(format!("{e}"))
}

fn proxy_status(e: reqwest::Error) -> TranscriptionError {
    let status = e.status();
    if matches!(status, Some(StatusCode::TOO_MANY_REQUESTS)) {
        TranscriptionError::QuotaExceeded
    } else if matches!(
        status,
        Some(StatusCode::UNAUTHORIZED) | Some(StatusCode::FORBIDDEN)
    ) {
        TranscriptionError::Auth(format!("proxy {}", status.unwrap()))
    } else {
        TranscriptionError::Provider(format!("proxy {e}"))
    }
}
