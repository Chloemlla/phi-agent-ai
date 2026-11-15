//! Wire-protocol adapters for the model gateway.
//!
//! The rest of the node speaks one internal shape (`ContentBlock`). This
//! module translates it to/from the three protocols users actually configure:
//!
//! - Anthropic Messages (`/v1/messages`)
//! - OpenAI Chat Completions (`/v1/chat/completions`)
//! - OpenAI Responses (`/v1/responses`)
//!
//! Request building and SSE decoding are pure functions so they can be tested
//! without a network or a UI.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use phi_contracts::ports::{
    InvalidToolCall, ModelError, ModelRequest, ThinkingEffort, ToolDefinition,
};
use phi_contracts::provider::{AuthMode, Protocol, ProviderProfile};
use phi_domain::message::{ChatMessage, ContentBlock, Role};
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

pub fn decoder(protocol: Protocol) -> Box<dyn StreamDecoder> {
    match protocol {
        Protocol::AnthropicMessages => Box::new(AnthropicDecoder::default()),
        Protocol::OpenAiChat => Box::new(ChatDecoder::default()),
        Protocol::OpenAiResponses => Box::new(ResponsesDecoder::default()),
    }
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

pub fn build_http(
    profile: &ProviderProfile,
    req: &ModelRequest,
    stream: bool,
) -> Result<HttpPlan, ModelError> {
    let url = endpoint_url(profile);
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

    let body = match profile.protocol {
        Protocol::AnthropicMessages => {
            let last_assistant = req
                .messages
                .iter()
                .rposition(|message| message.role == Role::Assistant);
            let mut body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": req.messages.iter().enumerate()
                    .map(|(index, message)| {
                        // Only the most recent assistant turn's thinking blocks
                        // are needed for tool-use continuation. Older ones are
                        // dropped so history edits do not invalidate the request.
                        message_to_anthropic(message, Some(index) == last_assistant)
                    })
                    .collect::<Vec<_>>(),
                "stream": stream,
            });
            if !req.system.trim().is_empty() {
                // A breakpoint on the system block caches the tool definitions
                // and system prompt (everything before it in the prompt).
                body["system"] = json!([{
                    "type": "text",
                    "text": req.system,
                    "cache_control": { "type": "ephemeral" }
                }]);
            }
            if let Some(effort) = effort {
                // Current Claude models use adaptive thinking; the effort level
                // is the only depth control. Legacy `budget_tokens` shapes are
                // intentionally not supported.
                body["thinking"] = json!({
                    "type": "adaptive",
                    "display": "summarized",
                });
                body["output_config"] = json!({ "effort": effort.as_str() });
            }
            // Anthropic rejects temperature/top_p on every current model.
            if !req.tools.is_empty() {
                body["tools"] = Value::Array(req.tools.iter().map(tool_to_anthropic).collect());
            }
            body
        }
        Protocol::OpenAiChat => {
            let reasoning = is_reasoning_model(&model);
            let mut body = json!({
                "model": model,
                "messages": messages_to_chat(&req.messages),
                "stream": stream,
            });
            if stream {
                // Ask for the final usage chunk; compatible providers ignore it.
                body["stream_options"] = json!({ "include_usage": true });
            }
            // Reasoning models reject `max_tokens`; newer Chat Completions
            // models use `max_completion_tokens`.
            if reasoning {
                body["max_completion_tokens"] = json!(max_tokens);
            } else {
                body["max_tokens"] = json!(max_tokens);
            }
            if !reasoning {
                if let Some(temperature) = profile.temperature {
                    body["temperature"] = json!(temperature);
                }
            }
            if let Some(effort) = effort {
                // Every current model accepts a reasoning effort; providers
                // that do not know the field ignore it.
                body["reasoning_effort"] = json!(effort.as_str());
            }
            if !req.tools.is_empty() {
                body["tools"] = Value::Array(req.tools.iter().map(tool_to_chat).collect());
            }
            body
        }
        Protocol::OpenAiResponses => {
            let reasoning = is_reasoning_model(&model);
            let mut body = json!({
                "model": model,
                "input": messages_to_responses(&req.messages),
                "max_output_tokens": max_tokens,
                "stream": stream,
            });
            if !req.system.trim().is_empty() {
                body["instructions"] = json!(req.system);
            }
            if !reasoning {
                if let Some(temperature) = profile.temperature {
                    body["temperature"] = json!(temperature);
                }
            }
            if let Some(effort) = effort {
                body["reasoning"] = json!({ "effort": effort.as_str() });
            }
            if !req.tools.is_empty() {
                body["tools"] = Value::Array(req.tools.iter().map(tool_to_responses).collect());
            }
            body
        }
    };

    Ok(HttpPlan { url, headers, body })
}

/// OpenAI reasoning models (the GPT-6 family) use `max_completion_tokens` on
/// Chat Completions and reject sampling parameters such as `temperature` on
/// both Chat and Responses.
fn is_reasoning_model(model: &str) -> bool {
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

fn message_to_anthropic(message: &ChatMessage, keep_thinking: bool) -> Value {
    let content: Vec<Value> = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
            ContentBlock::Thinking {
                thinking,
                signature,
            } => {
                // An unsigned thinking block cannot be replayed.
                if keep_thinking && !signature.is_empty() {
                    Some(json!({
                        "type": "thinking",
                        "thinking": thinking,
                        "signature": signature,
                    }))
                } else {
                    None
                }
            }
            ContentBlock::RedactedThinking { data } => {
                if keep_thinking && !data.is_empty() {
                    Some(json!({
                        "type": "redacted_thinking",
                        "data": data,
                    }))
                } else {
                    None
                }
            }
            ContentBlock::ToolUse { id, name, input } => Some(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input,
            })),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Some(json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": is_error,
            })),
        })
        .collect();
    let role = match message.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    json!({ "role": role, "content": content })
}

fn messages_to_chat(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User => {
                let mut texts = Vec::new();
                let mut tool_results = Vec::new();
                for block in &message.blocks {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => texts.push(text.clone()),
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => tool_results.push((tool_use_id.clone(), content.clone())),
                        _ => {}
                    }
                }
                // Tool results belong immediately after the assistant tool
                // calls; emit them before any new user text.
                for (tool_use_id, content) in tool_results {
                    out.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content,
                    }));
                }
                if !texts.is_empty() {
                    out.push(json!({ "role": "user", "content": texts.join("\n") }));
                }
            }
            Role::Assistant => {
                let mut text = String::new();
                let mut tool_calls = Vec::new();
                for block in &message.blocks {
                    match block {
                        ContentBlock::Text { text: piece } => {
                            if !piece.is_empty() {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(piece);
                            }
                        }
                        ContentBlock::ToolUse { id, name, input } => tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": input.to_string(),
                            },
                        })),
                        _ => {}
                    }
                }
                if tool_calls.is_empty() {
                    out.push(json!({
                        "role": "assistant",
                        "content": if text.is_empty() { Value::Null } else { json!(text) },
                    }));
                } else {
                    out.push(json!({
                        "role": "assistant",
                        "content": if text.is_empty() { Value::Null } else { json!(text) },
                        "tool_calls": tool_calls,
                    }));
                }
            }
        }
    }
    out
}

fn messages_to_responses(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User => {
                let mut texts = Vec::new();
                let mut tool_results = Vec::new();
                for block in &message.blocks {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => texts.push(text.clone()),
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => tool_results.push(json!({
                            "type": "function_call_output",
                            "call_id": tool_use_id,
                            "output": content,
                        })),
                        _ => {}
                    }
                }
                out.extend(tool_results);
                if !texts.is_empty() {
                    out.push(json!({ "role": "user", "content": texts.join("\n") }));
                }
            }
            Role::Assistant => {
                let mut text = String::new();
                let mut calls = Vec::new();
                for block in &message.blocks {
                    match block {
                        ContentBlock::Text { text: piece } => {
                            if !piece.is_empty() {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(piece);
                            }
                        }
                        ContentBlock::ToolUse { id, name, input } => calls.push(json!({
                            "type": "function_call",
                            "call_id": id,
                            "name": name,
                            "arguments": input.to_string(),
                        })),
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    out.push(json!({ "role": "assistant", "content": text }));
                }
                out.extend(calls);
            }
        }
    }
    out
}

fn tool_to_anthropic(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

fn tool_to_chat(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        },
    })
}

fn tool_to_responses(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

pub fn parse_response(protocol: Protocol, value: &Value) -> Result<ProtocolResponse, ModelError> {
    match protocol {
        Protocol::AnthropicMessages => parse_anthropic(value),
        Protocol::OpenAiChat => parse_chat(value),
        Protocol::OpenAiResponses => parse_responses(value),
    }
}

fn parse_anthropic(value: &Value) -> Result<ProtocolResponse, ModelError> {
    let mut blocks = Vec::new();
    let mut invalid_tool_calls = Vec::new();
    if let Some(content) = value["content"].as_array() {
        for item in content {
            match item["type"].as_str() {
                Some("text") => {
                    if let Some(text) = item["text"].as_str() {
                        if !text.is_empty() {
                            blocks.push(ContentBlock::text(text));
                        }
                    }
                }
                Some("tool_use") => {
                    let id = item["id"].as_str().unwrap_or_default().to_string();
                    let name = item["name"].as_str().unwrap_or_default().to_string();
                    let (input, error) = parse_arguments(&item["input"]);
                    if let Some(error) = error {
                        invalid_tool_calls.push(InvalidToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            block_index: blocks.len(),
                            raw_arguments: item["input"].as_str().unwrap_or_default().to_string(),
                            error,
                        });
                    }
                    blocks.push(ContentBlock::ToolUse { id, name, input });
                }
                Some("thinking") => {
                    let thinking = item["thinking"].as_str().unwrap_or_default();
                    let signature = item["signature"].as_str().unwrap_or_default();
                    if !thinking.is_empty() || !signature.is_empty() {
                        blocks.push(ContentBlock::Thinking {
                            thinking: thinking.to_string(),
                            signature: signature.to_string(),
                        });
                    }
                }
                Some("redacted_thinking") => {
                    if let Some(data) = item["data"].as_str() {
                        blocks.push(ContentBlock::RedactedThinking {
                            data: data.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    if blocks.is_empty() {
        blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
    }
    let usage = &value["usage"];
    let input_tokens = usage["input_tokens"].as_u64();
    let output_tokens = usage["output_tokens"].as_u64();
    // Anthropic reports cache read/write tokens separately from `input_tokens`.
    let cache_read_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
    let cache_write_tokens = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
    Ok(ProtocolResponse {
        blocks,
        input_tokens: input_tokens.unwrap_or(0),
        output_tokens: output_tokens.unwrap_or(0),
        cache_read_tokens,
        cache_write_tokens,
        invalid_tool_calls,
        finish_reason: value["stop_reason"]
            .as_str()
            .unwrap_or("end_turn")
            .to_string(),
        input_tokens_known: input_tokens.is_some(),
        output_tokens_known: output_tokens.is_some(),
        saw_terminal: true,
        estimated_usage: input_tokens.is_none() || output_tokens.is_none(),
    })
}

fn parse_chat(value: &Value) -> Result<ProtocolResponse, ModelError> {
    let choice = value["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .ok_or_else(|| model_error(i18n::t("Model response is missing choices")))?;
    let message = &choice["message"];
    let mut blocks = Vec::new();
    let mut invalid_tool_calls = Vec::new();

    if let Some(content) = message.get("content") {
        if let Some(text) = content.as_str() {
            if !text.is_empty() {
                blocks.push(ContentBlock::text(text));
            }
        } else if let Some(parts) = content.as_array() {
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::text(text));
                    }
                }
            }
        }
    }

    if let Some(calls) = message["tool_calls"].as_array() {
        for call in calls {
            let arguments = &call["function"]["arguments"];
            let id = call["id"].as_str().unwrap_or_default().to_string();
            let name = call["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let (input, error) = parse_arguments(arguments);
            if let Some(error) = error {
                invalid_tool_calls.push(InvalidToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    block_index: blocks.len(),
                    raw_arguments: arguments.as_str().unwrap_or_default().to_string(),
                    error,
                });
            }
            blocks.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
    }

    let usage = &value["usage"];
    let prompt_tokens = usage["prompt_tokens"].as_u64();
    // OpenAI includes cached tokens in `prompt_tokens`; DeepSeek exposes them
    // as `prompt_cache_hit_tokens`. Normalize to non-cached input tokens.
    let cache_read_tokens = usage["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
        .unwrap_or(0);
    let input_tokens = prompt_tokens.map(|total| total.saturating_sub(cache_read_tokens));
    let output_tokens = usage["completion_tokens"].as_u64();
    Ok(ProtocolResponse {
        blocks,
        input_tokens: input_tokens.unwrap_or(0),
        output_tokens: output_tokens.unwrap_or(0),
        cache_read_tokens,
        cache_write_tokens: 0,
        invalid_tool_calls,
        finish_reason: choice["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .to_string(),
        input_tokens_known: input_tokens.is_some(),
        output_tokens_known: output_tokens.is_some(),
        saw_terminal: true,
        estimated_usage: input_tokens.is_none() || output_tokens.is_none(),
    })
}

fn parse_responses(value: &Value) -> Result<ProtocolResponse, ModelError> {
    if value["status"].as_str() == Some("failed") || value.get("error").is_some() {
        let detail = value["error"]["message"]
            .as_str()
            .or_else(|| value["error"].as_str())
            .unwrap_or("the provider reported a failed response");
        return Err(model_error(i18n::t_args(
            "Model response failed: {error}",
            &[("error", truncate(detail, 500))],
        )));
    }
    let mut blocks = Vec::new();
    let mut invalid_tool_calls = Vec::new();
    if let Some(output) = value["output"].as_array() {
        for item in output {
            match item["type"].as_str() {
                Some("message") => {
                    if let Some(parts) = item["content"].as_array() {
                        for part in parts {
                            match part["type"].as_str() {
                                Some("output_text") | Some("text") => {
                                    if let Some(text) = part["text"].as_str() {
                                        if !text.is_empty() {
                                            blocks.push(ContentBlock::text(text));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    if let Some(text) = item["content"].as_str() {
                        if !text.is_empty() {
                            blocks.push(ContentBlock::text(text));
                        }
                    }
                }
                Some("function_call") => {
                    let id = item["call_id"]
                        .as_str()
                        .or_else(|| item["id"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = item["name"].as_str().unwrap_or_default().to_string();
                    let arguments = &item["arguments"];
                    let (input, error) = parse_arguments(arguments);
                    if let Some(error) = error {
                        invalid_tool_calls.push(InvalidToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            block_index: blocks.len(),
                            raw_arguments: arguments.as_str().unwrap_or_default().to_string(),
                            error,
                        });
                    }
                    blocks.push(ContentBlock::ToolUse { id, name, input });
                }
                _ => {}
            }
        }
    }
    if blocks.is_empty() {
        blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
    }
    let usage = &value["usage"];
    let prompt_tokens = usage["input_tokens"].as_u64();
    let cache_read_tokens = usage["input_tokens_details"]["cached_tokens"]
        .as_u64()
        .unwrap_or(0);
    let input_tokens = prompt_tokens.map(|total| total.saturating_sub(cache_read_tokens));
    let output_tokens = usage["output_tokens"].as_u64();
    let finish_reason = match value["status"].as_str() {
        Some("incomplete") => {
            // OpenAI names the output cap differently from Chat Completions;
            // normalize it so the loop's truncation recovery sees it.
            match value["incomplete_details"]["reason"]
                .as_str()
                .unwrap_or("length")
            {
                "max_output_tokens" => "length".to_string(),
                other => other.to_string(),
            }
        }
        _ => "end_turn".to_string(),
    };
    Ok(ProtocolResponse {
        blocks,
        input_tokens: input_tokens.unwrap_or(0),
        output_tokens: output_tokens.unwrap_or(0),
        cache_read_tokens,
        cache_write_tokens: 0,
        invalid_tool_calls,
        finish_reason,
        input_tokens_known: input_tokens.is_some(),
        output_tokens_known: output_tokens.is_some(),
        saw_terminal: true,
        estimated_usage: input_tokens.is_none() || output_tokens.is_none(),
    })
}

/// Parse tool arguments without ever failing. Returns the best-effort value
/// and an error string when recovery did not produce valid JSON.
///
/// A malformed call must not abort the whole model response: the assistant
/// text and the tool block are still returned, and the loop sends a paired
/// tool_result error instead of executing the tool.
fn parse_arguments(value: &Value) -> (Value, Option<String>) {
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

fn truncate(text: &str, max_chars: usize) -> String {
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
            };
        }
    }
    for tool in &req.tools {
        bytes += 64 + tool.description.len() + tool.input_schema.to_string().len();
    }
    (bytes as u64 + 256, req.max_output_tokens as u64)
}

// ─────────────────────────── SSE decoders ───────────────────────────

#[derive(Default)]
struct AnthropicDecoder {
    blocks: BTreeMap<u32, BlockAccum>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    stop_reason: Option<String>,
    saw_terminal: bool,
}

enum BlockAccum {
    Text(String),
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking(String),
    Tool {
        id: String,
        name: String,
        args: String,
    },
}

impl StreamDecoder for AnthropicDecoder {
    fn feed(&mut self, event: &str, data: &str) -> Result<Vec<StreamDelta>, ModelError> {
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            model_error(i18n::t_args(
                "SSE data is not valid JSON: {error}",
                &[("error", error.to_string())],
            ))
        })?;
        let kind = value["type"].as_str().unwrap_or(event);
        let mut deltas = Vec::new();
        match kind {
            "message_start" => {
                let usage = &value["message"]["usage"];
                self.input_tokens = usage["input_tokens"].as_u64();
                self.output_tokens = usage["output_tokens"].as_u64();
                self.cache_read_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                self.cache_write_tokens =
                    usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let index = value["index"].as_u64().unwrap_or(0) as u32;
                match value["content_block"]["type"].as_str() {
                    Some("text") => {
                        self.blocks.insert(index, BlockAccum::Text(String::new()));
                    }
                    Some("thinking") => {
                        self.blocks.insert(
                            index,
                            BlockAccum::Thinking {
                                thinking: String::new(),
                                signature: String::new(),
                            },
                        );
                    }
                    Some("redacted_thinking") => {
                        let data = value["content_block"]["data"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        self.blocks
                            .insert(index, BlockAccum::RedactedThinking(data));
                    }
                    Some("tool_use") => {
                        let id = value["content_block"]["id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let name = value["content_block"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        self.blocks.insert(
                            index,
                            BlockAccum::Tool {
                                id: id.clone(),
                                name: name.clone(),
                                args: String::new(),
                            },
                        );
                        deltas.push(StreamDelta::ToolStart { index, id, name });
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = value["index"].as_u64().unwrap_or(0) as u32;
                let delta_type = value["delta"]["type"].as_str().unwrap_or_default();
                match delta_type {
                    "text_delta" => {
                        if let Some(text) = value["delta"]["text"].as_str() {
                            if let Some(BlockAccum::Text(buffer)) = self.blocks.get_mut(&index) {
                                buffer.push_str(text);
                            }
                            if !text.is_empty() {
                                deltas.push(StreamDelta::Text {
                                    index,
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    "input_json_delta" => {
                        if let Some(partial) = value["delta"]["partial_json"].as_str() {
                            if let Some(BlockAccum::Tool { args, .. }) = self.blocks.get_mut(&index)
                            {
                                args.push_str(partial);
                            }
                            if !partial.is_empty() {
                                deltas.push(StreamDelta::ToolArgs {
                                    index,
                                    delta: partial.to_string(),
                                });
                            }
                        }
                    }
                    "thinking_delta" => {
                        if let Some(thinking) = value["delta"]["thinking"].as_str() {
                            if let Some(BlockAccum::Thinking {
                                thinking: buffer, ..
                            }) = self.blocks.get_mut(&index)
                            {
                                buffer.push_str(thinking);
                            }
                        }
                    }
                    "signature_delta" => {
                        if let Some(signature) = value["delta"]["signature"].as_str() {
                            if let Some(BlockAccum::Thinking {
                                signature: buffer, ..
                            }) = self.blocks.get_mut(&index)
                            {
                                buffer.push_str(signature);
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = value["index"].as_u64().unwrap_or(0) as u32;
                if matches!(self.blocks.get(&index), Some(BlockAccum::Tool { .. })) {
                    deltas.push(StreamDelta::ToolEnd { index });
                }
            }
            "message_delta" => {
                if let Some(reason) = value["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                    self.saw_terminal = true;
                }
                if let Some(output) = value["usage"]["output_tokens"].as_u64() {
                    self.output_tokens = Some(output);
                }
            }
            "message_stop" => {
                self.saw_terminal = true;
                deltas.push(StreamDelta::Done);
            }
            "error" => {
                return Err(model_error(i18n::t_args(
                    "SSE error: {message}",
                    &[(
                        "message",
                        value["error"]["message"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                    )],
                )));
            }
            _ => {}
        }
        Ok(deltas)
    }

    fn finish(self: Box<Self>) -> Result<ProtocolResponse, ModelError> {
        let this = *self;
        let mut blocks = Vec::new();
        let mut invalid_tool_calls = Vec::new();
        for (_, block) in this.blocks {
            match block {
                BlockAccum::Text(text) => {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::text(text));
                    }
                }
                BlockAccum::Thinking {
                    thinking,
                    signature,
                } => {
                    if !thinking.is_empty() || !signature.is_empty() {
                        blocks.push(ContentBlock::Thinking {
                            thinking,
                            signature,
                        });
                    }
                }
                BlockAccum::RedactedThinking(data) => {
                    if !data.is_empty() {
                        blocks.push(ContentBlock::RedactedThinking { data });
                    }
                }
                BlockAccum::Tool { id, name, args } => {
                    let (input, error) = parse_arguments(&Value::String(args.clone()));
                    if let Some(error) = error {
                        invalid_tool_calls.push(InvalidToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            block_index: blocks.len(),
                            raw_arguments: args,
                            error,
                        });
                    }
                    blocks.push(ContentBlock::ToolUse { id, name, input });
                }
            }
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
        }
        Ok(ProtocolResponse {
            blocks,
            input_tokens: this.input_tokens.unwrap_or(0),
            output_tokens: this.output_tokens.unwrap_or(0),
            cache_read_tokens: this.cache_read_tokens,
            cache_write_tokens: this.cache_write_tokens,
            invalid_tool_calls,
            finish_reason: this.stop_reason.unwrap_or_else(|| "end_turn".to_string()),
            input_tokens_known: this.input_tokens.is_some(),
            output_tokens_known: this.output_tokens.is_some(),
            saw_terminal: this.saw_terminal,
            estimated_usage: this.input_tokens.is_none() || this.output_tokens.is_none(),
        })
    }
}

#[derive(Default)]
struct ChatDecoder {
    text: String,
    tools: BTreeMap<u32, ToolAccum>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: u64,
    finish_reason: Option<String>,
    saw_terminal: bool,
}

#[derive(Default)]
struct ToolAccum {
    id: String,
    name: String,
    args: String,
    started: bool,
}

impl StreamDecoder for ChatDecoder {
    fn feed(&mut self, _event: &str, data: &str) -> Result<Vec<StreamDelta>, ModelError> {
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if data.trim() == "[DONE]" {
            self.saw_terminal = true;
            return Ok(vec![StreamDelta::Done]);
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            model_error(i18n::t_args(
                "SSE data is not valid JSON: {error}",
                &[("error", error.to_string())],
            ))
        })?;
        if value.get("error").is_some() {
            return Err(model_error(i18n::t_args(
                "SSE error: {message}",
                &[(
                    "message",
                    value["error"]["message"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                )],
            )));
        }
        if let Some(usage) = value.get("usage") {
            let cache_read = usage["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
                .unwrap_or(0);
            self.cache_read_tokens = cache_read;
            if let Some(input) = usage["prompt_tokens"].as_u64() {
                self.input_tokens = Some(input.saturating_sub(cache_read));
            }
            if let Some(output) = usage["completion_tokens"].as_u64() {
                self.output_tokens = Some(output);
            }
        }
        let mut deltas = Vec::new();
        if let Some(choices) = value["choices"].as_array() {
            for choice in choices {
                if let Some(finish) = choice["finish_reason"].as_str() {
                    if !finish.is_empty() {
                        self.finish_reason = Some(finish.to_string());
                        self.saw_terminal = true;
                    }
                }
                let delta = &choice["delta"];
                if let Some(text) = delta["content"].as_str() {
                    if !text.is_empty() {
                        self.text.push_str(text);
                        deltas.push(StreamDelta::Text {
                            index: 0,
                            text: text.to_string(),
                        });
                    }
                }
                if let Some(calls) = delta["tool_calls"].as_array() {
                    for call in calls {
                        let index = call["index"].as_u64().unwrap_or(0) as u32;
                        let entry = self.tools.entry(index).or_default();
                        if let Some(id) = call["id"].as_str() {
                            if !id.is_empty() {
                                entry.id = id.to_string();
                            }
                        }
                        if let Some(name) = call["function"]["name"].as_str() {
                            if !name.is_empty() {
                                entry.name = name.to_string();
                            }
                        }
                        let partial = call["function"]["arguments"].as_str().unwrap_or("");
                        if !entry.started && !entry.name.is_empty() {
                            entry.started = true;
                            deltas.push(StreamDelta::ToolStart {
                                index,
                                id: entry.id.clone(),
                                name: entry.name.clone(),
                            });
                        }
                        if !partial.is_empty() {
                            entry.args.push_str(partial);
                            deltas.push(StreamDelta::ToolArgs {
                                index,
                                delta: partial.to_string(),
                            });
                        }
                    }
                }
            }
        }
        Ok(deltas)
    }

    fn finish(self: Box<Self>) -> Result<ProtocolResponse, ModelError> {
        let this = *self;
        let mut blocks = Vec::new();
        let mut invalid_tool_calls = Vec::new();
        if !this.text.is_empty() {
            blocks.push(ContentBlock::text(this.text));
        }
        for (_, tool) in this.tools {
            let raw_arguments = tool.args;
            let (input, error) = parse_arguments(&Value::String(raw_arguments.clone()));
            if let Some(error) = error {
                invalid_tool_calls.push(InvalidToolCall {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    block_index: blocks.len(),
                    raw_arguments,
                    error,
                });
            }
            blocks.push(ContentBlock::ToolUse {
                id: tool.id,
                name: tool.name,
                input,
            });
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
        }
        Ok(ProtocolResponse {
            blocks,
            input_tokens: this.input_tokens.unwrap_or(0),
            output_tokens: this.output_tokens.unwrap_or(0),
            cache_read_tokens: this.cache_read_tokens,
            cache_write_tokens: 0,
            invalid_tool_calls,
            finish_reason: this.finish_reason.unwrap_or_else(|| "stop".to_string()),
            input_tokens_known: this.input_tokens.is_some(),
            output_tokens_known: this.output_tokens.is_some(),
            saw_terminal: this.saw_terminal,
            estimated_usage: this.input_tokens.is_none() || this.output_tokens.is_none(),
        })
    }
}

#[derive(Default)]
struct ResponsesDecoder {
    text: String,
    tools: BTreeMap<u32, ToolAccum>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: u64,
    final_response: Option<Value>,
    finish_reason: Option<String>,
    saw_terminal: bool,
}

impl StreamDecoder for ResponsesDecoder {
    fn feed(&mut self, event: &str, data: &str) -> Result<Vec<StreamDelta>, ModelError> {
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if data.trim() == "[DONE]" {
            self.saw_terminal = true;
            return Ok(vec![StreamDelta::Done]);
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            model_error(i18n::t_args(
                "SSE data is not valid JSON: {error}",
                &[("error", error.to_string())],
            ))
        })?;
        let kind = value["type"].as_str().unwrap_or(event);
        let mut deltas = Vec::new();
        match kind {
            "response.output_text.delta" => {
                if let Some(text) = value["delta"].as_str() {
                    if !text.is_empty() {
                        self.text.push_str(text);
                        deltas.push(StreamDelta::Text {
                            index: value["output_index"].as_u64().unwrap_or(0) as u32,
                            text: text.to_string(),
                        });
                    }
                }
            }
            "response.output_item.added" => {
                let item = &value["item"];
                if item["type"].as_str() == Some("function_call") {
                    let index = value["output_index"].as_u64().unwrap_or(0) as u32;
                    let id = item["call_id"]
                        .as_str()
                        .or_else(|| item["id"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = item["name"].as_str().unwrap_or_default().to_string();
                    let entry = self.tools.entry(index).or_default();
                    entry.id = id.clone();
                    entry.name = name.clone();
                    entry.started = true;
                    deltas.push(StreamDelta::ToolStart { index, id, name });
                }
            }
            "response.function_call_arguments.delta" => {
                let index = value["output_index"].as_u64().unwrap_or(0) as u32;
                if let Some(partial) = value["delta"].as_str() {
                    if !partial.is_empty() {
                        let entry = self.tools.entry(index).or_default();
                        entry.args.push_str(partial);
                        deltas.push(StreamDelta::ToolArgs {
                            index,
                            delta: partial.to_string(),
                        });
                    }
                }
            }
            "response.output_item.done" => {
                let item = &value["item"];
                if item["type"].as_str() == Some("function_call") {
                    let index = value["output_index"].as_u64().unwrap_or(0) as u32;
                    let entry = self.tools.entry(index).or_default();
                    if let Some(args) = item["arguments"].as_str() {
                        if entry.args.is_empty() {
                            entry.args = args.to_string();
                        }
                    }
                    deltas.push(StreamDelta::ToolEnd { index });
                }
            }
            "response.completed" => {
                self.final_response = Some(value["response"].clone());
                self.saw_terminal = true;
                if let Some(usage) = value["response"].get("usage") {
                    let cache_read = usage["input_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .unwrap_or(0);
                    self.cache_read_tokens = cache_read;
                    self.input_tokens = usage["input_tokens"]
                        .as_u64()
                        .map(|total| total.saturating_sub(cache_read));
                    self.output_tokens = usage["output_tokens"].as_u64();
                }
                deltas.push(StreamDelta::Done);
            }
            "response.incomplete" => {
                self.final_response = Some(value["response"].clone());
                self.finish_reason = Some("length".to_string());
                self.saw_terminal = true;
                deltas.push(StreamDelta::Done);
            }
            "response.failed" | "error" => {
                let message = value["response"]["error"]["message"]
                    .as_str()
                    .or_else(|| value["error"]["message"].as_str())
                    .or_else(|| value["message"].as_str())
                    .unwrap_or("unknown");
                return Err(model_error(i18n::t_args(
                    "SSE error: {message}",
                    &[("message", message.to_string())],
                )));
            }
            _ => {}
        }
        Ok(deltas)
    }

    fn finish(self: Box<Self>) -> Result<ProtocolResponse, ModelError> {
        let this = *self;
        if let Some(response) = this.final_response {
            return parse_responses(&response);
        }
        let mut blocks = Vec::new();
        let mut invalid_tool_calls = Vec::new();
        if !this.text.is_empty() {
            blocks.push(ContentBlock::text(this.text));
        }
        for (_, tool) in this.tools {
            let raw_arguments = tool.args;
            let (input, error) = parse_arguments(&Value::String(raw_arguments.clone()));
            if let Some(error) = error {
                invalid_tool_calls.push(InvalidToolCall {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    block_index: blocks.len(),
                    raw_arguments,
                    error,
                });
            }
            blocks.push(ContentBlock::ToolUse {
                id: tool.id,
                name: tool.name,
                input,
            });
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::text(i18n::t("(model returned no content)")));
        }
        Ok(ProtocolResponse {
            blocks,
            input_tokens: this.input_tokens.unwrap_or(0),
            output_tokens: this.output_tokens.unwrap_or(0),
            cache_read_tokens: this.cache_read_tokens,
            cache_write_tokens: 0,
            invalid_tool_calls,
            finish_reason: this.finish_reason.unwrap_or_else(|| "end_turn".to_string()),
            input_tokens_known: this.input_tokens.is_some(),
            output_tokens_known: this.output_tokens.is_some(),
            saw_terminal: this.saw_terminal,
            estimated_usage: this.input_tokens.is_none() || this.output_tokens.is_none(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn profile(protocol: Protocol) -> ProviderProfile {
        ProviderProfile {
            id: "p".into(),
            name: "test".into(),
            protocol,
            base_url: "https://api.example.com".into(),
            api_key: Some(phi_contracts::provider::SecretRef::literal("sk-test")),
            model: "test-model".into(),
            models: vec![],
            model_map: HashMap::new(),
            auth: None,
            headers: HashMap::new(),
            max_output_tokens: None,
            temperature: None,
            enabled: true,
            sort_index: 0,
            icon: None,
            icon_color: None,
            website_url: None,
            notes: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn request() -> ModelRequest {
        ModelRequest {
            model: "default".into(),
            system: "be helpful".into(),
            messages: vec![
                ChatMessage::user_text("hello"),
                ChatMessage {
                    role: Role::Assistant,
                    blocks: vec![ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "fs_list".into(),
                        input: json!({"path": "."}),
                    }],
                },
                ChatMessage {
                    role: Role::User,
                    blocks: vec![ContentBlock::tool_result("call_1", "files", false)],
                },
            ],
            tools: vec![ToolDefinition {
                id: "fs_list".into(),
                name: "fs_list".into(),
                description: "list files".into(),
                input_schema: json!({"type": "object"}),
                side_effect: phi_contracts::ports::SideEffect::None,
            }],
            max_output_tokens: 100,
            thinking: None,
            call_id: "call".into(),
            run_id: "run".into(),
            stream: true,
            workbench: false,
        }
    }

    #[test]
    fn endpoint_url_handles_v1_and_full_endpoints() {
        let mut p = profile(Protocol::OpenAiChat);
        assert_eq!(
            endpoint_url(&p),
            "https://api.example.com/v1/chat/completions"
        );
        p.base_url = "https://api.example.com/v1".into();
        assert_eq!(
            endpoint_url(&p),
            "https://api.example.com/v1/chat/completions"
        );
        p.base_url = "https://api.example.com/v1/chat/completions".into();
        assert_eq!(
            endpoint_url(&p),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(models_url(&p), "https://api.example.com/v1/models");
    }

    #[test]
    fn anthropic_request_keeps_system_and_tool_results() {
        let plan = build_http(&profile(Protocol::AnthropicMessages), &request(), false).unwrap();
        assert_eq!(plan.url, "https://api.example.com/v1/messages");
        assert!(
            plan.headers
                .iter()
                .any(|(key, _)| key == "anthropic-version")
        );
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(plan.body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(plan.body["system"][0]["text"], "be helpful");
        assert_eq!(plan.body["system"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn chat_request_uses_tool_messages_and_function_tools() {
        let plan = build_http(&profile(Protocol::OpenAiChat), &request(), true).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        let tool_message = messages
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("tool result message");
        assert_eq!(tool_message["tool_call_id"], "call_1");
        assert_eq!(plan.body["tools"][0]["type"], "function");
        assert_eq!(plan.body["stream"], true);
    }

    #[test]
    fn reasoning_models_use_max_completion_tokens_and_skip_temperature() {
        let mut p = profile(Protocol::OpenAiChat);
        p.model = "gpt-6-astra".into();
        p.temperature = Some(0.2);
        let plan = build_http(&p, &request(), false).unwrap();
        assert_eq!(plan.body["max_completion_tokens"], 100);
        assert!(plan.body.get("max_tokens").is_none());
        assert!(plan.body.get("temperature").is_none());
    }

    #[test]
    fn ordinary_chat_models_keep_max_tokens_and_temperature() {
        let mut p = profile(Protocol::OpenAiChat);
        p.model = "gpt-4o".into();
        p.temperature = Some(0.2);
        let plan = build_http(&p, &request(), false).unwrap();
        assert_eq!(plan.body["max_tokens"], 100);
        assert!(plan.body.get("max_completion_tokens").is_none());
        let temperature = plan.body["temperature"].as_f64().unwrap();
        assert!((temperature - 0.2).abs() < 1e-6);
    }

    #[test]
    fn responses_request_uses_function_call_items() {
        let plan = build_http(&profile(Protocol::OpenAiResponses), &request(), false).unwrap();
        let input = plan.body["input"].as_array().unwrap();
        assert!(input.iter().any(|item| item["type"] == "function_call"));
        assert!(
            input
                .iter()
                .any(|item| item["type"] == "function_call_output")
        );
        assert_eq!(plan.body["instructions"], "be helpful");
        assert_eq!(plan.body["tools"][0]["name"], "fs_list");
    }

    #[test]
    fn parses_anthropic_cache_usage() {
        let value = json!({
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 100,
                "cache_creation_input_tokens": 20
            }
        });
        let response = parse_response(Protocol::AnthropicMessages, &value).unwrap();
        assert_eq!(response.input_tokens, 10);
        assert_eq!(response.output_tokens, 5);
        assert_eq!(response.cache_read_tokens, 100);
        assert_eq!(response.cache_write_tokens, 20);
    }

    #[test]
    fn normalizes_openai_cached_prompt_tokens() {
        let value = json!({
            "choices": [{
                "message": {"role": "assistant", "content": "hi"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 5,
                "prompt_tokens_details": {"cached_tokens": 40}
            }
        });
        let response = parse_response(Protocol::OpenAiChat, &value).unwrap();
        assert_eq!(response.input_tokens, 60);
        assert_eq!(response.cache_read_tokens, 40);
        assert_eq!(response.output_tokens, 5);
    }

    #[test]
    fn normalizes_responses_cached_prompt_tokens() {
        let value = json!({
            "status": "completed",
            "output": [{"type": "message", "content": [{"type": "output_text", "text": "hi"}]}],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 5,
                "input_tokens_details": {"cached_tokens": 70}
            }
        });
        let response = parse_response(Protocol::OpenAiResponses, &value).unwrap();
        assert_eq!(response.input_tokens, 30);
        assert_eq!(response.cache_read_tokens, 70);
        assert_eq!(response.output_tokens, 5);
    }

    #[test]
    fn parses_chat_tool_calls() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "thinking",
                    "tool_calls": [{
                        "id": "call_9",
                        "type": "function",
                        "function": {"name": "fs_list", "arguments": "{\"path\":\".\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        });
        let response = parse_response(Protocol::OpenAiChat, &value).unwrap();
        assert_eq!(response.blocks.len(), 2);
        match &response.blocks[1] {
            ContentBlock::ToolUse { id, input, .. } => {
                assert_eq!(id, "call_9");
                assert_eq!(input["path"], ".");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(response.input_tokens, 10);
        assert!(!response.estimated_usage);
        assert!(response.invalid_tool_calls.is_empty());
    }

    #[test]
    fn responses_incomplete_output_cap_is_truncation() {
        let value = json!({
            "status": "incomplete",
            "incomplete_details": { "reason": "max_output_tokens" },
            "output": [{
                "type": "function_call",
                "call_id": "call_truncated",
                "name": "fs_write",
                "arguments": "{\"path\":"
            }],
            "usage": { "input_tokens": 5, "output_tokens": 2 }
        });
        let response = parse_response(Protocol::OpenAiResponses, &value).unwrap();
        assert_eq!(response.finish_reason, "length");
        assert_eq!(response.invalid_tool_calls.len(), 1);
    }

    #[test]
    fn responses_failed_status_is_an_error() {
        let value = json!({
            "status": "failed",
            "error": { "message": "upstream exploded" }
        });
        let error = parse_response(Protocol::OpenAiResponses, &value).unwrap_err();
        assert!(error.message.contains("upstream exploded"));
    }

    #[test]
    fn parses_responses_function_call() {
        let value = json!({
            "status": "completed",
            "output": [
                {"type": "message", "content": [{"type": "output_text", "text": "hi"}]},
                {"type": "function_call", "call_id": "call_2", "name": "fs_read", "arguments": "{\"path\":\"a.txt\"}"}
            ],
            "usage": {"input_tokens": 7, "output_tokens": 3}
        });
        let response = parse_response(Protocol::OpenAiResponses, &value).unwrap();
        assert_eq!(response.blocks.len(), 2);
        match &response.blocks[1] {
            ContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "call_2");
                assert_eq!(name, "fs_read");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn anthropic_sse_collects_text_and_tool_json() {
        let mut decoder = decoder(Protocol::AnthropicMessages);
        let events = vec![
            (
                "message_start",
                json!({"type":"message_start","message":{"usage":{"input_tokens":4,"output_tokens":1}}}),
            ),
            (
                "content_block_start",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            ),
            (
                "content_block_delta",
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}),
            ),
            (
                "content_block_delta",
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}),
            ),
            (
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ),
            (
                "content_block_start",
                json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_1","name":"fs_list"}}),
            ),
            (
                "content_block_delta",
                json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\".\"}"}}),
            ),
            (
                "content_block_stop",
                json!({"type":"content_block_stop","index":1}),
            ),
            (
                "message_delta",
                json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
            ),
            ("message_stop", json!({"type":"message_stop"})),
        ];
        for (event, value) in events {
            decoder.feed(event, &value.to_string()).unwrap();
        }
        let response = decoder.finish().unwrap();
        assert_eq!(response.blocks.len(), 2);
        assert_eq!(response.input_tokens, 4);
        assert_eq!(response.output_tokens, 9);
        assert_eq!(response.finish_reason, "tool_use");
        assert!(response.invalid_tool_calls.is_empty());
        match &response.blocks[1] {
            ContentBlock::ToolUse { id, input, .. } => {
                assert_eq!(id, "call_1");
                assert_eq!(input["path"], ".");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn anthropic_thinking_blocks_are_preserved_and_replayed() {
        // Non-streaming parse keeps the thinking block and its signature.
        let value = json!({
            "content": [
                {"type": "thinking", "thinking": "consider the file", "signature": "sig-1"},
                {"type": "text", "text": "I will read it."}
            ],
            "usage": {"input_tokens": 5, "output_tokens": 7},
            "stop_reason": "end_turn"
        });
        let response = parse_response(Protocol::AnthropicMessages, &value).unwrap();
        match &response.blocks[0] {
            ContentBlock::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "consider the file");
                assert_eq!(signature, "sig-1");
            }
            other => panic!("unexpected {other:?}"),
        }

        // The assistant message is replayed to Anthropic with the block intact.
        let message = ChatMessage {
            role: Role::Assistant,
            blocks: response.blocks,
        };
        let wire = message_to_anthropic(&message, true);
        assert_eq!(wire["content"][0]["type"], "thinking");
        assert_eq!(wire["content"][0]["thinking"], "consider the file");
        assert_eq!(wire["content"][0]["signature"], "sig-1");
    }

    #[test]
    fn anthropic_sse_collects_thinking_and_redacted_blocks() {
        let mut decoder = decoder(Protocol::AnthropicMessages);
        let events = vec![
            (
                "content_block_start",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
            ),
            (
                "content_block_delta",
                json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"let me think"}}),
            ),
            (
                "content_block_delta",
                json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-2"}}),
            ),
            (
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ),
            (
                "content_block_start",
                json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"opaque"}}),
            ),
            (
                "content_block_stop",
                json!({"type":"content_block_stop","index":1}),
            ),
            ("message_stop", json!({"type":"message_stop"})),
        ];
        for (event, value) in events {
            decoder.feed(event, &value.to_string()).unwrap();
        }
        let response = decoder.finish().unwrap();
        assert!(matches!(
            &response.blocks[0],
            ContentBlock::Thinking { thinking, signature }
                if thinking == "let me think" && signature == "sig-2"
        ));
        assert!(matches!(
            &response.blocks[1],
            ContentBlock::RedactedThinking { data } if data == "opaque"
        ));
    }

    #[test]
    fn chat_sse_collects_text_and_tool_call() {
        let mut decoder = decoder(Protocol::OpenAiChat);
        let chunks = vec![
            json!({"choices":[{"delta":{"content":"Hel"}}]}),
            json!({"choices":[{"delta":{"content":"lo"}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_3","function":{"name":"fs_list","arguments":"{\"path\""}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\".\"}"}}]},"finish_reason":"tool_calls"}]}),
            json!({"usage":{"prompt_tokens":3,"completion_tokens":4}}),
        ];
        for chunk in chunks {
            decoder.feed("", &chunk.to_string()).unwrap();
        }
        decoder.feed("", "[DONE]").unwrap();
        let response = decoder.finish().unwrap();
        assert_eq!(response.blocks.len(), 2);
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.input_tokens, 3);
        match &response.blocks[1] {
            ContentBlock::ToolUse { id, input, .. } => {
                assert_eq!(id, "call_3");
                assert_eq!(input["path"], ".");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn responses_sse_uses_completed_response() {
        let mut decoder = decoder(Protocol::OpenAiResponses);
        let completed = json!({
            "type": "response.completed",
            "response": {
                "status": "completed",
                "output": [{"type":"message","content":[{"type":"output_text","text":"done"}]}],
                "usage": {"input_tokens": 5, "output_tokens": 2}
            }
        });
        decoder
            .feed(
                "response.output_text.delta",
                &json!({"type":"response.output_text.delta","output_index":0,"delta":"do"})
                    .to_string(),
            )
            .unwrap();
        decoder
            .feed("response.completed", &completed.to_string())
            .unwrap();
        let response = decoder.finish().unwrap();
        assert_eq!(response.blocks.len(), 1);
        match &response.blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "done"),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(response.input_tokens, 5);
    }

    #[test]
    fn anthropic_adaptive_thinking_uses_effort_and_omits_temperature() {
        let mut p = profile(Protocol::AnthropicMessages);
        p.model = "claude-sonnet-5-5".into();
        p.temperature = Some(0.3);
        p.max_output_tokens = Some(4_096);
        let mut req = request();
        req.max_output_tokens = 4_096;
        req.thinking = Some(ThinkingEffort::High);
        let plan = build_http(&p, &req, false).unwrap();
        assert_eq!(plan.body["thinking"]["type"], "adaptive");
        assert_eq!(plan.body["thinking"]["display"], "summarized");
        assert_eq!(plan.body["output_config"]["effort"], "high");
        assert_eq!(
            plan.body["max_tokens"].as_u64().unwrap(),
            4_096,
            "adaptive thinking must not exceed the model output cap"
        );
        assert!(
            plan.body.get("temperature").is_none(),
            "Anthropic rejects temperature on current models"
        );
    }

    #[test]
    fn off_thinking_omits_protocol_fields() {
        let mut p = profile(Protocol::AnthropicMessages);
        p.model = "claude-sonnet-5-5".into();
        p.temperature = Some(0.3);
        let mut req = request();
        req.thinking = Some(ThinkingEffort::Off);
        let plan = build_http(&p, &req, false).unwrap();
        assert!(plan.body.get("thinking").is_none());
        assert!(plan.body.get("output_config").is_none());
        assert!(
            plan.body.get("temperature").is_none(),
            "temperature is never valid on current Anthropic models"
        );

        let p = profile(Protocol::OpenAiChat);
        let mut req = request();
        req.thinking = Some(ThinkingEffort::Off);
        let plan = build_http(&p, &req, false).unwrap();
        assert!(plan.body.get("reasoning_effort").is_none());

        let mut p = profile(Protocol::OpenAiResponses);
        p.model = "gpt-6-sol".into();
        let plan = build_http(&p, &req, false).unwrap();
        assert!(plan.body.get("reasoning").is_none());
    }

    #[test]
    fn chat_thinking_sets_reasoning_effort() {
        let mut p = profile(Protocol::OpenAiChat);
        p.model = "gpt-6-astra".into();
        let mut req = request();
        req.thinking = Some(ThinkingEffort::Low);
        let plan = build_http(&p, &req, false).unwrap();
        assert_eq!(plan.body["reasoning_effort"], "low");

        // Compatibility providers serving custom ids receive the same effort
        // field; APIs that do not know it ignore it.
        p.model = "custom-reasoner".into();
        let plan = build_http(&p, &req, false).unwrap();
        assert_eq!(plan.body["reasoning_effort"], "low");
    }

    #[test]
    fn responses_thinking_sets_the_reasoning_object() {
        let mut p = profile(Protocol::OpenAiResponses);
        p.model = "gpt-6-sol".into();
        let mut req = request();
        req.thinking = Some(ThinkingEffort::Medium);
        let plan = build_http(&p, &req, false).unwrap();
        assert_eq!(plan.body["reasoning"]["effort"], "medium");
    }

    #[test]
    fn malformed_chat_arguments_keep_text_and_emit_a_block() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "I will write the file.",
                    "tool_calls": [{
                        "id": "call_bad",
                        "type": "function",
                        "function": {"name": "fs_write", "arguments": "{\"path\": src/main.rs}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        });
        let response = parse_response(Protocol::OpenAiChat, &value).unwrap();
        match &response.blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "I will write the file."),
            other => panic!("unexpected {other:?}"),
        }
        match &response.blocks[1] {
            ContentBlock::ToolUse { id, input, .. } => {
                assert_eq!(id, "call_bad");
                assert_eq!(input, &json!({}));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(response.invalid_tool_calls.len(), 1);
        assert_eq!(response.invalid_tool_calls[0].id, "call_bad");
        assert_eq!(response.invalid_tool_calls[0].name, "fs_write");
        assert!(!response.invalid_tool_calls[0].error.is_empty());
    }

    #[test]
    fn fenced_and_wrapped_json_arguments_are_recovered() {
        for arguments in [
            "```json\n{\"path\":\".\"}\n```",
            "Here is the call: {\"path\":\".\"} thanks",
            "{\"path\":\".\"}",
        ] {
            let value = json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "tool_calls": [{
                            "id": "call_ok",
                            "type": "function",
                            "function": {"name": "fs_list", "arguments": arguments}
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            });
            let response = parse_response(Protocol::OpenAiChat, &value).unwrap();
            assert!(
                response.invalid_tool_calls.is_empty(),
                "should recover {arguments}"
            );
            match &response.blocks[0] {
                ContentBlock::ToolUse { input, .. } => assert_eq!(input["path"], "."),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn chat_sse_malformed_arguments_do_not_fail_the_response() {
        let mut decoder = decoder(Protocol::OpenAiChat);
        decoder
            .feed(
                "",
                &json!({"choices":[{"delta":{"content":"Looking."}}]}).to_string(),
            )
            .unwrap();
        decoder
            .feed(
                "",
                &json!({"choices":[{
                    "delta":{"tool_calls":[{
                        "index":0,
                        "id":"call_bad",
                        "function":{"name":"fs_write","arguments":"{\"path\": src/main.rs}"}
                    }]},
                    "finish_reason":"tool_calls"
                }]})
                .to_string(),
            )
            .unwrap();
        let response = decoder.finish().unwrap();
        assert_eq!(response.blocks.len(), 2);
        assert_eq!(response.invalid_tool_calls.len(), 1);
        match &response.blocks[1] {
            ContentBlock::ToolUse { input, .. } => assert_eq!(input, &json!({})),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn anthropic_sse_malformed_arguments_do_not_fail_the_response() {
        let mut decoder = decoder(Protocol::AnthropicMessages);
        let events = vec![
            json!({"type":"message_start","message":{"usage":{"input_tokens":4,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Working."}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_bad","name":"fs_write"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\": src/main.rs}"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_stop"}),
        ];
        for event in events {
            decoder.feed("", &event.to_string()).unwrap();
        }
        let response = decoder.finish().unwrap();
        assert_eq!(response.blocks.len(), 2);
        assert_eq!(response.invalid_tool_calls.len(), 1);
        assert_eq!(response.invalid_tool_calls[0].id, "call_bad");
        match &response.blocks[1] {
            ContentBlock::ToolUse { input, .. } => assert_eq!(input, &json!({})),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn http_errors_are_classified() {
        let error = parse_http_error(401, r#"{"error":{"message":"bad key"}}"#);
        assert!(error.message.starts_with("auth:"));
        assert!(!error.retryable);
        let error = parse_http_error(429, "");
        assert!(error.message.starts_with("rate_limit:"));
        assert!(error.retryable);
        let error = parse_http_error(503, "");
        assert!(error.message.starts_with("server:"));
        assert!(error.retryable);
    }
}
