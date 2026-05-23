use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{json, Value};

use super::{LlmError, LlmProvider, LlmRequest};
use crate::providers::ProviderMode;

const ANTHROPIC_DIRECT_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const DEFAULT_MAX_TOKENS: u32 = 4096;
const PROXY_LLM_PATH: &str = "/v1/llm";

/// AnthropicProvider — единая обёртка для двух путей:
/// - Managed: через прокси (`POST {proxy}/v1/llm` с заголовком `x-device-id`)
/// - BYO: напрямую в Anthropic Messages API с ключом из keychain
///
/// См. M4.1 паспорта.
pub struct AnthropicProvider {
    pub mode: ProviderMode,
    /// URL endpoint Anthropic напрямую. Меняется только в тестах.
    pub direct_url: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(mode: ProviderMode) -> Self {
        Self {
            mode,
            direct_url: ANTHROPIC_DIRECT_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn generate(&self, request: LlmRequest) -> Result<Value, LlmError> {
        match &self.mode {
            ProviderMode::Managed {
                proxy_base_url,
                device_id,
            } => generate_managed(&self.http, proxy_base_url, device_id, &request).await,
            ProviderMode::Byo { api_key } => {
                generate_byo(&self.http, &self.direct_url, api_key, &request).await
            }
        }
    }
}

/// [Bug-fix] Backoff schedule для transient errors (5xx / 429 / network /
/// proxy-wrapped upstream-error). 3 attempts → 1s → 3s → 9s + small jitter.
const BACKOFF_BASE_MS: [u64; 3] = [1000, 3000, 9000];

/// [Bug-fix] Транзиентность сообщения от proxy/upstream — на эти patterns
/// retry'имся, иначе propagate как permanent error. Cloudflare Workers
/// часто оборачивает 429 от Anthropic в `provider_error` body с текстом
/// "LLM upstream error (429)" — это тот же transient throttle, ретраим.
pub(super) fn is_retryable_message(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("upstream error")
        || lower.contains("bad gateway")
        || lower.contains("rate limit")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
}

/// [Bug-fix] Тонкий jitter (±~250ms) без `rand` crate — берём nanos
/// текущего времени. Не криптографически случайно, но достаточно для
/// разъезда параллельных retry-волн.
fn jitter_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as i64)
        .unwrap_or(0);
    (nanos % 500) - 250
}

async fn generate_managed(
    http: &reqwest::Client,
    proxy_base_url: &str,
    device_id: &str,
    request: &LlmRequest,
) -> Result<Value, LlmError> {
    let url = format!("{}{}", proxy_base_url.trim_end_matches('/'), PROXY_LLM_PATH);
    let payload = json!({
        "model": request.model,
        "system": request.system,
        "input": request.input,
        "maxTokens": request.max_tokens,
    });

    // [Bug-fix] Client-side retry с exponential backoff для transient errors:
    // - network err
    // - 5xx HTTP status
    // - 200 + `ok:false` + retryable message ("429"/"upstream"/"Bad Gateway")
    //   — Cloudflare proxy wraps Anthropic 429 как provider_error внутри 200.
    // 3 attempts: 1s → 3s → 9s + ±~250ms jitter. После исчерпания → last err.
    let mut last_provider_err: Option<String> = None;
    for attempt in 0..3_u32 {
        if attempt > 0 {
            let base = BACKOFF_BASE_MS[(attempt - 1).min(2) as usize] as i64;
            let wait_ms = (base + jitter_ms()).max(100) as u64;
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        }
        let resp = match http
            .post(&url)
            .header("x-device-id", device_id)
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_provider_err = Some(format!("network {e}"));
                continue;
            }
        };

        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            // 429 path: пробуем разобрать body — если `code:"quota_exceeded"`,
            // это hard-cap (R7 паспорта) → QuotaExceeded (no retry, юзеру
            // нужно ждать до завтра / переключаться на BYO). Если другой
            // code или body не парсится — считаем transient throttle от
            // upstream Anthropic и ретраимся с backoff.
            let body_text = resp.text().await.unwrap_or_default();
            let parsed: Option<Value> = serde_json::from_str(&body_text).ok();
            let code = parsed
                .as_ref()
                .and_then(|v| v.get("code"))
                .and_then(Value::as_str);
            if matches!(code, Some("quota_exceeded")) {
                return Err(LlmError::QuotaExceeded);
            }
            last_provider_err = Some(format!(
                "proxy 429: {}",
                body_text.chars().take(200).collect::<String>()
            ));
            continue;
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(LlmError::Auth(format!("proxy {status}")));
        }
        if status.is_server_error() {
            let body = resp.text().await.unwrap_or_default();
            last_provider_err = Some(format!(
                "proxy {status}: {}",
                body.chars().take(200).collect::<String>()
            ));
            continue;
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "proxy {status}: {}",
                body.chars().take(200).collect::<String>()
            )));
        }
        match parse_managed_body(resp).await {
            Ok(v) => return Ok(v),
            // Retry для ok:false с transient message; quota/auth — propagate.
            Err(LlmError::Provider(msg)) if is_retryable_message(&msg) => {
                last_provider_err = Some(msg);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(LlmError::Provider(
        last_provider_err.unwrap_or_else(|| "proxy unknown".into()),
    ))
}

async fn parse_managed_body(resp: reqwest::Response) -> Result<Value, LlmError> {
    // Прокси отдаёт LlmResponse: { ok: true, json: ... } | { ok: false, code, message }.
    let body: Value = resp
        .json()
        .await
        .map_err(|e| LlmError::Provider(e.to_string()))?;

    if body.get("ok") == Some(&Value::Bool(true)) {
        Ok(body.get("json").cloned().unwrap_or(Value::Null))
    } else {
        let message = body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown proxy error")
            .to_string();
        match body.get("code").and_then(|v| v.as_str()) {
            Some("quota_exceeded") => Err(LlmError::QuotaExceeded),
            Some("invalid_device_id") => Err(LlmError::Auth(message)),
            _ => Err(LlmError::Provider(message)),
        }
    }
}

async fn generate_byo(
    http: &reqwest::Client,
    direct_url: &str,
    api_key: &str,
    request: &LlmRequest,
) -> Result<Value, LlmError> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let resp = http
        .post(direct_url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": model,
            "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "system": request.system,
            "messages": [{"role": "user", "content": request.input}],
        }))
        .send()
        .await
        .map_err(|e| LlmError::Network(e.to_string()))?;

    let status = resp.status();
    if status == StatusCode::UNAUTHORIZED {
        return Err(LlmError::Auth("anthropic 401".into()));
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(LlmError::QuotaExceeded);
    }
    if !status.is_success() {
        return Err(LlmError::Provider(format!("anthropic {status}")));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| LlmError::Provider(e.to_string()))?;

    // Anthropic возвращает { content: [{type: "text", text: "..."}, ...], usage: {...} }.
    // Извлекаем первый text-блок и парсим JSON (LLM должен возвращать JSON по prompt'у).
    let text = body
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter().find_map(|block| {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    block.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| LlmError::Provider("no text block in anthropic response".into()))?;

    serde_json::from_str(text).map_err(|e| LlmError::Provider(format!("not JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn req() -> LlmRequest {
        LlmRequest {
            model: None,
            system: "you are a recap bot, return JSON".into(),
            input: "Alice: hi\nBob: hello".into(),
            max_tokens: None,
            grammar: None,
        }
    }

    fn managed_provider(proxy_base_url: String) -> AnthropicProvider {
        AnthropicProvider {
            mode: ProviderMode::Managed {
                proxy_base_url,
                device_id: "dev-uuid-1".into(),
            },
            direct_url: ANTHROPIC_DIRECT_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }

    fn byo_provider(direct_url: String, api_key: &str) -> AnthropicProvider {
        AnthropicProvider {
            mode: ProviderMode::Byo {
                api_key: api_key.into(),
            },
            direct_url,
            http: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn managed_ok_returns_json_payload() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/llm")
                    .header("x-device-id", "dev-uuid-1");
                then.status(200).json_body(json!({
                    "ok": true,
                    "json": {"summary": "ok", "key_points": ["one"]}
                }));
            })
            .await;

        let p = managed_provider(server.base_url());
        let result = p.generate(req()).await.expect("managed should succeed");

        mock.assert_async().await;
        assert_eq!(result["summary"], "ok");
        assert_eq!(result["key_points"][0], "one");
    }

    #[tokio::test]
    async fn managed_quota_exceeded_maps_to_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/llm");
                then.status(429).json_body(json!({
                    "ok": false,
                    "code": "quota_exceeded",
                    "message": "stt daily quota exceeded"
                }));
            })
            .await;

        let p = managed_provider(server.base_url());
        let err = p.generate(req()).await.unwrap_err();
        assert!(matches!(err, LlmError::QuotaExceeded), "got {err:?}");
    }

    #[tokio::test]
    async fn managed_proxy_error_body_propagates() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/llm");
                then.status(200).json_body(json!({
                    "ok": false,
                    "code": "provider_error",
                    "message": "upstream 502"
                }));
            })
            .await;

        let p = managed_provider(server.base_url());
        let err = p.generate(req()).await.unwrap_err();
        match err {
            LlmError::Provider(msg) => assert_eq!(msg, "upstream 502"),
            other => panic!("expected Provider, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn byo_parses_anthropic_text_block_as_json() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .header("x-api-key", "sk-test-123")
                    .header("anthropic-version", ANTHROPIC_VERSION);
                then.status(200).json_body(json!({
                    "content": [{"type": "text", "text": "{\"summary\":\"byo ok\"}"}],
                    "usage": {"input_tokens": 10, "output_tokens": 5}
                }));
            })
            .await;

        let p = byo_provider(server.url("/v1/messages"), "sk-test-123");
        let result = p.generate(req()).await.expect("byo should succeed");

        mock.assert_async().await;
        assert_eq!(result["summary"], "byo ok");
    }

    #[tokio::test]
    async fn byo_unauthorized_maps_to_auth_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST);
                then.status(401);
            })
            .await;

        let p = byo_provider(server.url("/v1/messages"), "sk-bad");
        let err = p.generate(req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Auth(_)), "got {err:?}");
    }

    // [Bug-fix #1] 200 + ok:false + retryable message ("LLM upstream error (429)")
    // → 3 attempts. Cloudflare Workers оборачивает Anthropic 429 в provider_error.
    // Backoff prevents test зависание via overriding constants — но cargo test
    // должен finish < 30s, поэтому используем mock который сразу отвечает.
    #[tokio::test]
    async fn managed_429_in_provider_error_retries_three_times() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/llm");
                then.status(200).json_body(json!({
                    "ok": false,
                    "code": "provider_error",
                    "message": "LLM upstream error (429)"
                }));
            })
            .await;

        let p = managed_provider(server.base_url());
        let err = p.generate(req()).await.unwrap_err();
        // 3 attempts всего — все 3 раза один и тот же ok:false response.
        mock.assert_hits_async(3).await;
        match err {
            LlmError::Provider(msg) => assert!(
                msg.contains("upstream error (429)"),
                "expected retryable msg in final err: {msg}"
            ),
            other => panic!("expected Provider after retries, got {other:?}"),
        }
    }

    // [Bug-fix #1] HTTP 429 без code:"quota_exceeded" → транзиент, ретраим.
    // Direct upstream 429 редок (обычно proxy wrap'ает) но возможен.
    #[tokio::test]
    async fn managed_http_429_transient_retries() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/llm");
                then.status(429).body("Too Many Requests");
            })
            .await;

        let p = managed_provider(server.base_url());
        let _ = p.generate(req()).await.unwrap_err();
        mock.assert_hits_async(3).await;
    }

    #[test]
    fn is_retryable_message_patterns() {
        assert!(is_retryable_message("LLM upstream error (429)"));
        assert!(is_retryable_message("Bad Gateway"));
        assert!(is_retryable_message("rate limit exceeded"));
        assert!(is_retryable_message("proxy 502: ..."));
        assert!(is_retryable_message("503 service unavailable"));
        assert!(!is_retryable_message("invalid api key"));
        assert!(!is_retryable_message("quota exhausted"));
    }

    #[tokio::test]
    async fn byo_non_json_text_returns_provider_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST);
                then.status(200).json_body(json!({
                    "content": [{"type": "text", "text": "definitely not JSON"}]
                }));
            })
            .await;

        let p = byo_provider(server.url("/v1/messages"), "sk-test");
        let err = p.generate(req()).await.unwrap_err();
        match err {
            LlmError::Provider(msg) => assert!(msg.contains("not JSON"), "msg: {msg}"),
            other => panic!("expected Provider, got {other:?}"),
        }
    }
}
