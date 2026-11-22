//! OpenAI Chat Completions (`/v1/chat/completions`) request, response, and
//! stream handling.

use std::collections::{BTreeMap, HashMap};

use base64::Engine;
use serde_json::{Value, json};

use phi_contracts::ports::{InvalidToolCall, ModelError, ModelRequest, ToolDefinition};
use phi_contracts::provider::ProviderProfile;
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_i18n as i18n;

use super::common::{
    PreparedRequest, ProtocolResponse, StreamDecoder, StreamDelta, ToolAccum, is_reasoning_model,
    model_error, parse_arguments,
};

pub(super) fn build_body(
    profile: &ProviderProfile,
    req: &ModelRequest,
    prepared: &PreparedRequest,
    stream: bool,
) -> Value {
    let reasoning = is_reasoning_model(&prepared.model);
    let mut body = json!({
        "model": &prepared.model,
        "messages": messages_to_chat(&req.messages, &req.images),
        "stream": stream,
    });
    if stream {
        // Ask for the final usage chunk; compatible providers ignore it.
        body["stream_options"] = json!({ "include_usage": true });
    }
    // Reasoning models reject `max_tokens`; newer Chat Completions
    // models use `max_completion_tokens`.
    if reasoning {
        body["max_completion_tokens"] = json!(prepared.max_tokens);
    } else {
        body["max_tokens"] = json!(prepared.max_tokens);
    }
    if !reasoning {
        if let Some(temperature) = profile.temperature {
            body["temperature"] = json!(temperature);
        }
    }
    if let Some(effort) = prepared.effort {
        // Every current model accepts a reasoning effort; providers
        // that do not know the field ignore it.
        body["reasoning_effort"] = json!(effort.as_str());
    }
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(req.tools.iter().map(tool_to_chat).collect());
    }
    body
}

fn messages_to_chat(messages: &[ChatMessage], images: &HashMap<String, Vec<u8>>) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            Role::User => {
                let mut texts = Vec::new();
                let mut parts = Vec::new();
                let mut has_image = false;
                let mut tool_results = Vec::new();
                for block in &message.blocks {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            texts.push(text.clone());
                            parts.push(json!({ "type": "text", "text": text }));
                        }
                        ContentBlock::Image { asset_id, mime } => {
                            if let Some(bytes) = images.get(asset_id) {
                                has_image = true;
                                parts.push(json!({
                                    "type": "image_url",
                                    "image_url": {
                                        "url": format!(
                                            "data:{mime};base64,{}",
                                            base64::engine::general_purpose::STANDARD.encode(bytes)
                                        )
                                    }
                                }));
                            }
                        }
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
                if has_image {
                    out.push(json!({ "role": "user", "content": parts }));
                } else if !texts.is_empty() {
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

pub(super) fn parse(value: &Value) -> Result<ProtocolResponse, ModelError> {
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

#[derive(Default)]
pub(super) struct ChatDecoder {
    text: String,
    tools: BTreeMap<u32, ToolAccum>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: u64,
    finish_reason: Option<String>,
    saw_terminal: bool,
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
