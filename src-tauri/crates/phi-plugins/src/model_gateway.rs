//! Model gateway: provider selection, protocol dispatch, streaming, metering.
//!
//! Every model call in the node goes through this plugin. It resolves the
//! active provider profile, builds the wire request, streams deltas to the
//! registered shell sink (ephemeral, not journaled), and returns the final
//! response for the durable assistant message.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::model_protocol::{
    ProtocolResponse, StreamDelta, build_http, conservative_request_tokens, decoder, model_error,
    models_url, parse_http_error, parse_response,
};
use crate::provider_store::{ProviderStore, ResolvedProvider};
use phi_contracts::SLOT_MODEL_INVOKE;
use phi_contracts::ports::{ModelError, ModelPort, ModelRequest, ModelResponse, ModelStatus};
use phi_contracts::provider::{AuthMode, Pricing, ProviderProfile};
use phi_contracts::stream::{ModelStreamEvent, StreamHub};
use phi_i18n as i18n;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::registry::Contribution;

pub struct ModelGateway {
    client: reqwest::Client,
    providers: Arc<ProviderStore>,
    streams: Arc<StreamHub>,
}

fn compute_cost(
    pricing: Pricing,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> f64 {
    // Unknown cache prices fall back to the ordinary input price, which
    // overestimates rather than hiding spend.
    let cache_read_price = pricing
        .cache_read_per_mtok
        .unwrap_or(pricing.input_per_mtok);
    let cache_write_price = pricing
        .cache_write_per_mtok
        .unwrap_or(pricing.input_per_mtok);
    input_tokens as f64 / 1_000_000.0 * pricing.input_per_mtok
        + output_tokens as f64 / 1_000_000.0 * pricing.output_per_mtok
        + cache_read_tokens as f64 / 1_000_000.0 * cache_read_price
        + cache_write_tokens as f64 / 1_000_000.0 * cache_write_price
}

impl ModelGateway {
    pub fn new(providers: Arc<ProviderStore>, streams: Arc<StreamHub>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            // Streaming responses can legitimately take a long time; the run
            // is stopped by cancellation, not by a short request timeout.
            .timeout(Duration::from_secs(900))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            providers,
            streams,
        }
    }

    fn compute_cost(
        &self,
        profile: &ProviderProfile,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    ) -> f64 {
        compute_cost(
            profile.pricing_or_default(),
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
        )
    }

    async fn invoke_resolved(
        &self,
        resolved: ResolvedProvider,
        req: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        let profile = resolved.profile;
        let stream = req.stream && self.streams.is_enabled();
        // Per-model metadata overrides the provider defaults for this call.
        let model_id = profile.resolved_model(&req.model);
        if profile
            .model_spec(&model_id)
            .is_some_and(|spec| !spec.enabled)
        {
            return Err(model_error(i18n::t_args(
                "Model {model} is disabled in this provider",
                &[("model", model_id)],
            )));
        }
        let profile = profile.with_model_overrides(&model_id);
        let plan = build_http(&profile, &req, stream)?;
        let mut builder = self.client.post(&plan.url).json(&plan.body);
        for (key, value) in &plan.headers {
            builder = builder.header(key, value);
        }

        let response = tokio::select! {
            result = builder.send() => result.map_err(|error| ModelError {
                message: i18n::t_args(
                    "network: request failed: {error}",
                    &[("error", error.to_string())],
                ),
                retryable: true,
            })?,
            _ = cancel.cancelled() => return Err(cancelled_error()),
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(parse_http_error(status.as_u16(), &body));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if stream && content_type.contains("text/event-stream") {
            return self.consume_stream(response, profile, &req, cancel).await;
        }

        let raw = response.text().await.map_err(|error| ModelError {
            message: i18n::t_args(
                "network: failed to read response: {error}",
                &[("error", error.to_string())],
            ),
            retryable: true,
        })?;
        let value: Value = serde_json::from_str(&raw).map_err(|error| ModelError {
            message: i18n::t_args(
                "invalid_response: response is not valid JSON: {error}",
                &[("error", error.to_string())],
            ),
            retryable: false,
        })?;
        let parsed = parse_response(profile.protocol, &value)?;
        Ok(self.finish_response(&profile, &req, parsed))
    }

    async fn consume_stream(
        &self,
        mut response: reqwest::Response,
        profile: ProviderProfile,
        req: &ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        let mut buffer = SseBuffer::default();
        let mut decoder = decoder(profile.protocol);
        let mut done_emitted = false;

        loop {
            let chunk = tokio::select! {
                result = response.chunk() => result.map_err(|error| ModelError {
                    message: i18n::t_args(
                        "network: failed to read stream: {error}",
                        &[("error", error.to_string())],
                    ),
                    retryable: true,
                })?,
                _ = cancel.cancelled() => return Err(cancelled_error()),
            };
            let Some(bytes) = chunk else {
                break;
            };
            buffer.push(&bytes);
            while let Some((event, data)) = buffer.next_event() {
                for delta in decoder.feed(&event, &data)? {
                    if matches!(delta, StreamDelta::Done) {
                        done_emitted = true;
                    }
                    self.publish_delta(req, delta);
                }
            }
        }

        // Some servers end the stream without a trailing blank line.
        buffer.push(b"\n\n");
        while let Some((event, data)) = buffer.next_event() {
            for delta in decoder.feed(&event, &data)? {
                if matches!(delta, StreamDelta::Done) {
                    done_emitted = true;
                }
                self.publish_delta(req, delta);
            }
        }

        let parsed = decoder.finish()?;
        if !done_emitted {
            self.publish_delta(req, StreamDelta::Done);
        }
        Ok(self.finish_response(&profile, req, parsed))
    }

    fn finish_response(
        &self,
        profile: &ProviderProfile,
        req: &ModelRequest,
        parsed: ProtocolResponse,
    ) -> ModelResponse {
        let (fallback_input, fallback_output) = conservative_request_tokens(req);
        let input_tokens = if parsed.input_tokens == 0 {
            fallback_input
        } else {
            parsed.input_tokens
        };
        let output_tokens = if parsed.output_tokens == 0 {
            fallback_output
        } else {
            parsed.output_tokens
        };
        ModelResponse {
            blocks: parsed.blocks,
            input_tokens,
            output_tokens,
            cache_read_tokens: parsed.cache_read_tokens,
            cache_write_tokens: parsed.cache_write_tokens,
            cost_usd: self.compute_cost(
                profile,
                input_tokens,
                output_tokens,
                parsed.cache_read_tokens,
                parsed.cache_write_tokens,
            ),
            finish_reason: parsed.finish_reason,
            estimated_usage: parsed.estimated_usage,
        }
    }

    fn publish_delta(&self, req: &ModelRequest, delta: StreamDelta) {
        if !self.streams.is_enabled() {
            return;
        }
        let event = match delta {
            StreamDelta::Text { index, text } => ModelStreamEvent {
                run_id: req.run_id.clone(),
                call_id: req.call_id.clone(),
                index,
                kind: "text".to_string(),
                text,
                tool_use_id: None,
                tool_name: None,
            },
            StreamDelta::ToolStart { index, id, name } => ModelStreamEvent {
                run_id: req.run_id.clone(),
                call_id: req.call_id.clone(),
                index,
                kind: "tool_use_start".to_string(),
                text: String::new(),
                tool_use_id: Some(id),
                tool_name: Some(name),
            },
            StreamDelta::ToolArgs { index, delta } => ModelStreamEvent {
                run_id: req.run_id.clone(),
                call_id: req.call_id.clone(),
                index,
                kind: "tool_use_delta".to_string(),
                text: delta,
                tool_use_id: None,
                tool_name: None,
            },
            StreamDelta::ToolEnd { index } => ModelStreamEvent {
                run_id: req.run_id.clone(),
                call_id: req.call_id.clone(),
                index,
                kind: "tool_use_end".to_string(),
                text: String::new(),
                tool_use_id: None,
                tool_name: None,
            },
            StreamDelta::Done => ModelStreamEvent {
                run_id: req.run_id.clone(),
                call_id: req.call_id.clone(),
                index: 0,
                kind: "done".to_string(),
                text: String::new(),
                tool_use_id: None,
                tool_name: None,
            },
        };
        self.streams.publish(&event);
    }

    fn resolve_for_listing(&self, id: Option<&str>) -> Result<ResolvedProvider, String> {
        match id {
            Some(id) => self.providers.resolve_for_listing(id),
            None => self.providers.resolve_active_for_listing()?.ok_or_else(|| {
                i18n::t(
                    "No provider is configured; add a provider or set the environment variables",
                )
            }),
        }
    }

    pub async fn list_models(&self, id: Option<&str>) -> Result<(String, Vec<String>), String> {
        let resolved = self.resolve_for_listing(id)?;
        let profile = resolved.profile;
        let url = models_url(&profile);
        let mut request = self.client.get(&url);
        if let Some(key) = resolved.api_key.as_deref() {
            request = match profile.auth_mode() {
                AuthMode::XApiKey => request.header("x-api-key", key),
                AuthMode::Bearer => request.header("authorization", format!("Bearer {key}")),
                AuthMode::None => request,
            };
        }
        for (key, value) in &profile.headers {
            request = request.header(key, value);
        }
        let response = request.send().await.map_err(|error| {
            i18n::t_args(
                "network: failed to list models: {error}",
                &[("error", error.to_string())],
            )
        })?;
        let status = response.status();
        let raw = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(parse_http_error(status.as_u16(), &raw).message);
        }
        let value: Value = serde_json::from_str(&raw).map_err(|error| {
            i18n::t_args(
                "invalid_response: model list is not valid JSON: {error}",
                &[("error", error.to_string())],
            )
        })?;
        let mut models = Vec::new();
        if let Some(data) = value["data"].as_array() {
            for item in data {
                if let Some(id) = item["id"].as_str() {
                    models.push(id.to_string());
                } else if let Some(name) = item["name"].as_str() {
                    models.push(name.to_string());
                }
            }
        } else if let Some(data) = value["models"].as_array() {
            for item in data {
                if let Some(id) = item["id"].as_str() {
                    models.push(id.to_string());
                } else if let Some(id) = item.as_str() {
                    models.push(id.to_string());
                }
            }
        } else if let Some(data) = value.as_array() {
            for item in data {
                if let Some(id) = item.as_str() {
                    models.push(id.to_string());
                }
            }
        }
        models.sort();
        models.dedup();
        if !models.is_empty() {
            self.providers.mark_models(&profile.id, models.clone())?;
        }
        Ok((profile.id, models))
    }

    pub async fn test_provider(
        &self,
        id: Option<&str>,
    ) -> phi_contracts::provider::ProviderTestView {
        let started = Instant::now();
        match self.list_models(id).await {
            Ok((_, models)) => phi_contracts::provider::ProviderTestView {
                ok: true,
                latency_ms: started.elapsed().as_millis() as u64,
                model_count: models.len(),
                error: None,
            },
            Err(error) => phi_contracts::provider::ProviderTestView {
                ok: false,
                latency_ms: started.elapsed().as_millis() as u64,
                model_count: 0,
                error: Some(error),
            },
        }
    }
}

impl Plugin for ModelGateway {
    fn id(&self) -> &'static str {
        "plugin-model-gw"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_MODEL_INVOKE.to_string(),
            id: "multi-protocol".to_string(),
        }])
    }
}

#[async_trait]
impl ModelPort for ModelGateway {
    fn status(&self) -> ModelStatus {
        match self.providers.active() {
            Some(profile) => {
                let configured = profile.has_key();
                let model_id = profile.resolved_model("");
                let context_window = profile
                    .model_spec(&model_id)
                    .and_then(|spec| spec.context_window);
                ModelStatus {
                    provider: profile.name.clone(),
                    model: profile.model.clone(),
                    configured,
                    provider_id: Some(profile.id.clone()),
                    protocol: profile.protocol.as_str().to_string(),
                    base_url: Some(profile.base_url.clone()),
                    has_key: profile
                        .api_key
                        .as_ref()
                        .map(|key| key.is_configured())
                        .unwrap_or(profile.auth_mode() == AuthMode::None),
                    context_window,
                }
            }
            None => ModelStatus {
                provider: "none".to_string(),
                model: String::new(),
                configured: false,
                provider_id: None,
                protocol: String::new(),
                base_url: None,
                has_key: false,
                context_window: None,
            },
        }
    }

    fn set_api_key(&self, key: &str) -> Result<ModelStatus, String> {
        self.providers.set_api_key(key)?;
        Ok(self.status())
    }

    async fn invoke(
        &self,
        req: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        let resolved = match self.providers.resolve_active() {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                return Err(model_error(i18n::t(
                    "No provider is configured; add one in Settings",
                )));
            }
            Err(error) => {
                // A configured-but-broken provider must be visible; never
                // silently substitute another path.
                return Err(model_error(format!("provider: {error}")));
            }
        };
        self.invoke_resolved(resolved, req, cancel).await
    }
}

fn cancelled_error() -> ModelError {
    ModelError {
        message: "cancelled".to_string(),
        retryable: false,
    }
}

#[derive(Default)]
struct SseBuffer {
    buf: Vec<u8>,
}

impl SseBuffer {
    fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn next_event(&mut self) -> Option<(String, String)> {
        loop {
            let (position, delimiter) = earliest_frame(&self.buf)?;
            let frame: Vec<u8> = self.buf.drain(..position).collect();
            self.buf.drain(..delimiter);
            if let Some(event) = parse_frame(&frame) {
                return Some(event);
            }
        }
    }
}

fn earliest_frame(buf: &[u8]) -> Option<(usize, usize)> {
    let newline = find_subslice(buf, b"\n\n").map(|position| (position, 2));
    let crlf = find_subslice(buf, b"\r\n\r\n").map(|position| (position, 4));
    match (newline, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_frame(frame: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(frame);
    let mut event = String::new();
    let mut data = String::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest);
        }
    }
    if event.is_empty() && data.is_empty() {
        None
    } else {
        Some((event, data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phi_contracts::provider::{ModelSpec, ProviderInput};

    #[test]
    fn cost_accounts_for_cache_read_and_write() {
        let pricing = Pricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        };
        let cost = compute_cost(pricing, 1_000_000, 0, 1_000_000, 1_000_000);
        assert!((cost - (3.0 + 0.3 + 3.75)).abs() < 1e-9, "cost={cost}");
    }

    #[test]
    fn unknown_cache_prices_fall_back_to_the_input_price() {
        let pricing = Pricing {
            input_per_mtok: 2.0,
            output_per_mtok: 10.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        };
        let cost = compute_cost(pricing, 0, 0, 1_000_000, 1_000_000);
        assert!((cost - 4.0).abs() < 1e-9, "cost={cost}");
    }

    #[test]
    fn sse_buffer_handles_split_frames_and_crlf() {
        let mut buffer = SseBuffer::default();
        buffer.push(b"event: message_start\ndata: {\"a\":1}\n\n");
        let (event, data) = buffer.next_event().unwrap();
        assert_eq!(event, "message_start");
        assert_eq!(data, "{\"a\":1}");

        buffer.push(b"data: {\"b\":");
        assert!(buffer.next_event().is_none());
        buffer.push(b"2}\r\n\r\n");
        let (event, data) = buffer.next_event().unwrap();
        assert_eq!(event, "");
        assert_eq!(data, "{\"b\":2}");
    }

    #[test]
    fn sse_buffer_ignores_comments_and_heartbeats() {
        let mut buffer = SseBuffer::default();
        buffer.push(b": ping\n\ndata: [DONE]\n\n");
        let (_, data) = buffer.next_event().unwrap();
        assert_eq!(data, "[DONE]");
    }

    #[tokio::test]
    async fn missing_provider_is_an_honest_error() {
        let dir = std::env::temp_dir().join(format!("phi-gateway-{}", uuid::Uuid::new_v4()));
        let providers = Arc::new(ProviderStore::load(dir.join("providers.json")));
        let gateway = ModelGateway::new(providers, Arc::new(StreamHub::default()));
        let req = ModelRequest {
            model: "default".into(),
            system: "hello".into(),
            messages: vec![phi_domain::message::ChatMessage::user_text("hi")],
            tools: Vec::new(),
            max_output_tokens: 100,
            call_id: "call".into(),
            run_id: "run".into(),
            stream: false,
            workbench: false,
        };
        let error = gateway
            .invoke(req, CancellationToken::new())
            .await
            .expect_err("a missing provider must fail, not fabricate a response");
        assert!(
            error.message.contains("No provider is configured"),
            "unexpected error: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn disabled_models_are_rejected_before_any_request() {
        let dir = std::env::temp_dir().join(format!("phi-gateway-{}", uuid::Uuid::new_v4()));
        let providers = Arc::new(ProviderStore::load(dir.join("providers.json")));
        let mut disabled = ModelSpec::from_id("disabled-model");
        disabled.enabled = false;
        let view = providers
            .upsert(ProviderInput {
                name: Some("Provider".into()),
                protocol: Some("openai_chat".into()),
                base_url: Some("https://example.invalid/v1".into()),
                api_key: Some("literal:test-key".into()),
                model: Some("main-model".into()),
                models: Some(vec![ModelSpec::from_id("main-model"), disabled]),
                ..Default::default()
            })
            .unwrap();
        providers.set_active(&view.id).unwrap();

        let gateway = ModelGateway::new(providers, Arc::new(StreamHub::default()));
        let req = ModelRequest {
            model: "disabled-model".into(),
            system: "hello".into(),
            messages: vec![phi_domain::message::ChatMessage::user_text("hi")],
            tools: Vec::new(),
            max_output_tokens: 100,
            call_id: "call".into(),
            run_id: "run".into(),
            stream: false,
            workbench: false,
        };
        let error = gateway
            .invoke(req, CancellationToken::new())
            .await
            .expect_err("disabled model must not be called");
        assert!(
            error.message.contains("disabled"),
            "unexpected error: {}",
            error.message
        );
    }
}
