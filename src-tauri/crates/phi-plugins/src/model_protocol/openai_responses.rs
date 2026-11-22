//! OpenAI Responses (`/v1/responses`) request, response, and stream handling.

use std::collections::{BTreeMap, HashMap};

use base64::Engine;
use serde_json::{Value, json};

use phi_contracts::ports::{InvalidToolCall, ModelError, ModelRequest, ToolDefinition};
use phi_contracts::provider::ProviderProfile;
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_i18n as i18n;

use super::common::{
    PreparedRequest, ProtocolResponse, StreamDecoder, StreamDelta, ToolAccum, is_reasoning_model,
    model_error, parse_arguments, truncate,
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
        "input": messages_to_responses(&req.messages, &req.images),
        "max_output_tokens": prepared.max_tokens,
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
    if let Some(effort) = prepared.effort {
        body["reasoning"] = json!({ "effort": effort.as_str() });
    }
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(req.tools.iter().map(tool_to_responses).collect());
    }
    body
}

fn messages_to_responses(
    messages: &[ChatMessage],
    images: &HashMap<String, Vec<u8>>,
) -> Vec<Value> {
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
                            parts.push(json!({ "type": "input_text", "text": text }));
                        }
                        ContentBlock::Image { asset_id, mime } => {
                            if let Some(bytes) = images.get(asset_id) {
                                has_image = true;
                                parts.push(json!({
                                    "type": "input_image",
                                    "image_url": format!(
                                        "data:{mime};base64,{}",
                                        base64::engine::general_purpose::STANDARD.encode(bytes)
                                    )
                                }));
                            }
                        }
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
                if has_image {
                    out.push(json!({ "role": "user", "content": parts }));
                } else if !texts.is_empty() {
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

fn tool_to_responses(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

pub(super) fn parse(value: &Value) -> Result<ProtocolResponse, ModelError> {
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

#[derive(Default)]
pub(super) struct ResponsesDecoder {
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
            return parse(&response);
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
