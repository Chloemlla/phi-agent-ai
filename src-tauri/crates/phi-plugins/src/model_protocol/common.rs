//! Shared HTTP, request, SSE, and error helpers for the protocol adapters.

use serde_json::{Value, json};

use phi_contracts::ports::{InvalidToolCall, ModelError, ModelRequest, ThinkingEffort};
use phi_contracts::provider::{AuthMode, Protocol, ProviderProfile};
use phi_domain::message::ContentBlock;
use phi_i18n as i18n;

#[derive(Clone, Debug)]
pub struct HttpPlan {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

#[derive(Clone, Debug)]
pub struct ProtocolResponse {
    pub blocks: Vec<ContentBlock>,
    /// Non-cached input tokens. Providers that include cached tokens in the
    /// prompt total are normalized here.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Whether the provider actually reported the field. A reported `0` is
    /// real (for example a fully cached prompt) and must not be replaced by an
    /// estimate.
    pub input_tokens_known: bool,
    pub output_tokens_known: bool,
    /// Input tokens served from the provider's prompt cache.
    pub cache_read_tokens: u64,
    /// Input tokens written to the provider's prompt cache.
    pub cache_write_tokens: u64,
    /// Calls whose arguments could not be parsed. Their blocks are still in
    /// `blocks`; the loop must not execute them.
    pub invalid_tool_calls: Vec<InvalidToolCall>,
    pub finish_reason: String,
    /// True when the protocol delivered an explicit terminal marker
    /// (`message_stop`, `[DONE]`, a stop reason, or `response.completed`).
    pub saw_terminal: bool,
    pub estimated_usage: bool,
}

#[derive(Clone, Debug)]
pub enum StreamDelta {
    Text {
        index: u32,
        text: String,
    },
    ToolStart {
        index: u32,
        id: String,
        name: String,
    },
    ToolArgs {
        index: u32,
        delta: String,
    },
    ToolEnd {
        index: u32,
    },
    Done,
}

pub trait StreamDecoder: Send {
    fn feed(&mut self, event: &str, data: &str) -> Result<Vec<StreamDelta>, ModelError>;
    fn finish(self: Box<Self>) -> Result<ProtocolResponse, ModelError>;
}

/// Accumulated OpenAI tool call shared by the Chat and Responses decoders.
#[derive(Default)]
pub(super) struct ToolAccum {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) args: String,
    pub(super) started: bool,
}

pub fn endpoint_url(profile: &ProviderProfile) -> String {
    let base = profile.base_url.trim_end_matches('/');
    let suffix = profile.protocol.endpoint_suffix();
    if base.ends_with(suffix) {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}{suffix}")
    } else {
        format!("{base}/v1{suffix}")
    }
}

pub fn models_url(profile: &ProviderProfile) -> String {
    let mut base = profile.base_url.trim_end_matches('/');
    // A profile may point at a full endpoint; the model list lives beside it.
    for suffix in ["/chat/completions", "/responses", "/messages"] {
        if let Some(rest) = base.strip_suffix(suffix) {
            base = rest.trim_end_matches('/');
            break;
        }
    }
    if base.ends_with("/v1/models") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

/// Values shared by every protocol's request body builder.
pub(super) struct PreparedRequest {
    pub(super) model: String,
    pub(super) max_tokens: u32,
    pub(super) effort: Option<ThinkingEffort>,
}

pub(super) fn prepare_request(
    profile: &ProviderProfile,
    req: &ModelRequest,
) -> Result<PreparedRequest, ModelError> {
    let model = profile.resolve_model(&req.model);
    if model.trim().is_empty() {
        return Err(model_error(i18n::t("Provider has no model configured")));
    }
    let max_tokens = profile
        .max_output_tokens
        .map(|limit| req.max_output_tokens.min(limit))
        .unwrap_or(req.max_output_tokens)
        .max(1);
    // `Off` and `None` are equivalent: leave the provider default untouched.
    let effort = req.thinking.filter(|effort| *effort != ThinkingEffort::Off);
    Ok(PreparedRequest {
        model,
        max_tokens,
        effort,
    })
}

/// Build authentication and profile headers, rejecting injected newlines.
pub(super) fn request_headers(
    profile: &ProviderProfile,
    req: &ModelRequest,
) -> Result<Vec<(String, String)>, ModelError> {
    let mut headers = auth_headers(profile, req)?;
    for (key, value) in &profile.headers {
        if value.contains('\r') || value.contains('\n') {
            return Err(model_error(i18n::t_args(
                "Header {key} contains an invalid newline",
                &[("key", key.clone())],
            )));
        }
        headers.push((key.clone(), value.clone()));
    }
    Ok(headers)
}

fn auth_headers(
    profile: &ProviderProfile,
    req: &ModelRequest,
) -> Result<Vec<(String, String)>, ModelError> {
    let key = match profile.auth_mode() {
        AuthMode::None => None,
        _ => Some(
            profile
                .api_key
                .as_ref()
                .ok_or_else(|| model_error(i18n::t("Provider has no API key configured")))?
                .resolve()
                .map_err(model_error)?,
        ),
    };
    let _ = req;
    let mut headers = Vec::new();
    match profile.auth_mode() {
        AuthMode::XApiKey => {
            headers.push(("x-api-key".to_string(), key.unwrap_or_default()));
        }
        AuthMode::Bearer => {
            headers.push((
                "authorization".to_string(),
                format!("Bearer {}", key.unwrap_or_default()),
            ));
        }
        AuthMode::None => {}
    }
    if profile.protocol == Protocol::AnthropicMessages {
        headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
    }
    Ok(headers)
}

/// OpenAI reasoning models (the GPT-6 family) use `max_completion_tokens` on
/// Chat Completions and reject sampling parameters such as `temperature` on
/// both Chat and Responses.
pub(super) fn is_reasoning_model(model: &str) -> bool {
    let model = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .trim()
        .to_ascii_lowercase();
    family_match(&model, "gpt-6")
}

/// Match `prefix` at the start of `id` as a family boundary, so `gpt-5`
/// matches `gpt-6.1` and `gpt-6-mini` but not `gpt-60`.
fn family_match(id: &str, prefix: &str) -> bool {
    match id.strip_prefix(prefix) {
        Some(rest) => {
            rest.is_empty()
                || rest.starts_with('-')
                || rest.starts_with('.')
                || rest.starts_with(':')
                || rest.starts_with('_')
        }
        None => false,
    }
}

/// Parse tool arguments without ever failing. Returns the best-effort value
/// and an error string when recovery did not produce valid JSON.
///
/// A malformed call must not abort the whole model response: the assistant
/// text and the tool block are still returned, and the loop sends a paired
/// tool_result error instead of executing the tool.
pub(super) fn parse_arguments(value: &Value) -> (Value, Option<String>) {
    match value {
        Value::String(raw) => parse_raw_arguments(raw),
        Value::Null => (json!({}), None),
        other => (other.clone(), None),
    }
}

fn parse_raw_arguments(raw: &str) -> (Value, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return (json!({}), None);
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return (value, None);
    }
    let error = serde_json::from_str::<Value>(trimmed)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| i18n::t("Tool arguments are not valid JSON"));
    // Models sometimes wrap JSON in markdown fences or prose. Recover the
    // outermost balanced object or array before giving up.
    let mut candidates: Vec<&str> = Vec::new();
    if let Some(unfenced) = strip_code_fence(trimmed) {
        candidates.push(unfenced);
    }
    candidates.push(trimmed);
    for candidate in candidates {
        if let Some(json_text) = extract_balanced_json(candidate) {
            // Reject ambiguous output like `{...} {...}`: the trailing text
            // must be whitespace, not a second payload.
            let end = json_text.as_ptr() as usize + json_text.len();
            let trailing = &candidate[end - candidate.as_ptr() as usize..];
            // Trailing prose is fine; a second object/array is ambiguous.
            if trailing.contains('{') || trailing.contains('[') {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(json_text) {
                return (value, None);
            }
        }
    }
    (json!({}), Some(error))
}

fn strip_code_fence(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let after_open = trimmed.strip_prefix("```")?;
    let after_language = match after_open.find('\n') {
        Some(index) => &after_open[index + 1..],
        None => "",
    };
    let without_close = after_language.trim_end();
    let without_close = without_close.strip_suffix("```").unwrap_or(without_close);
    Some(without_close.trim())
}

/// Extract the outermost balanced `{...}` or `[...]` without trusting the
/// surrounding text. String literals and escapes are skipped so braces inside
/// values do not end the candidate early.
fn extract_balanced_json(raw: &str) -> Option<&str> {
    let start = raw.find(|c| c == '{' || c == '[')?;
    let bytes = raw.as_bytes();
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        if byte == b'"' {
            in_string = true;
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(&raw[start..start + offset + 1]);
            }
        }
    }
    None
}

pub fn model_error(message: impl Into<String>) -> ModelError {
    ModelError::new(message, false)
}

/// Stable, human-readable classification for HTTP failures. The prefixes are
/// intentionally part of the contract: shells can show a targeted message.
pub fn parse_http_error(status: u16, body: &str) -> ModelError {
    let detail = extract_error_detail(body);
    let (kind, retryable) = match status {
        401 | 403 => ("auth", false),
        404 => ("not_found", false),
        400 | 422 => ("invalid_request", false),
        408 | 409 | 425 | 429 => ("rate_limit", true),
        500..=599 => ("server", true),
        _ => ("http", false),
    };
    ModelError::new(format!("{kind}: HTTP {status}: {detail}"), retryable)
}

fn extract_error_detail(body: &str) -> String {
    let parsed = serde_json::from_str::<Value>(body).ok();
    let message = parsed.as_ref().and_then(|value| {
        value
            .pointer("/error/message")
            .or_else(|| value.get("message"))
            .or_else(|| value.get("detail"))
            .and_then(|value| value.as_str())
    });
    match message {
        Some(message) => truncate(message, 500),
        None => truncate(body, 500),
    }
}

pub(super) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

pub fn conservative_request_tokens(req: &ModelRequest) -> (u64, u64) {
    let mut bytes = req.system.len() + 256;
    for message in &req.messages {
        bytes += 32;
        for block in &message.blocks {
            bytes += match block {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::Thinking {
                    thinking,
                    signature,
                } => thinking.len() + signature.len(),
                ContentBlock::RedactedThinking { data } => data.len(),
                ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                ContentBlock::ToolResult { content, .. } => content.len(),
                ContentBlock::Image { .. } => 0,
            };
        }
    }
    for tool in &req.tools {
        bytes += 64 + tool.description.len() + tool.input_schema.to_string().len();
    }
    (bytes as u64 + 256, req.max_output_tokens as u64)
}
