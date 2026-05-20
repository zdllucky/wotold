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

    // [B12] клиент-side retry: на 5xx или Network один раз пробуем ещё с
    // паузой 2 секунды. Покрывает кейс когда proxy внутренний retry тоже
    // упал в transient Groq glitch.
    let mut last_provider_err: Option<String> = None;
    for attempt in 0..2_u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
            return Err(LlmError::QuotaExceeded);
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(LlmError::Auth(format!("proxy {status}")));
        }
        if status.is_server_error() {
            let body = resp.text().await.unwrap_or_default();
            last_provider_err = Some(format!("proxy {status}: {}", body.chars().take(200).collect::<String>()));
            continue;
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "proxy {status}: {}",
                body.chars().take(200).collect::<String>()
            )));
        }
        return parse_managed_body(resp).await;
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
