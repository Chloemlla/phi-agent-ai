//! Anthropic Messages (`/v1/messages`) request, response, and stream handling.

use std::collections::{BTreeMap, HashMap};

use base64::Engine;
use serde_json::{Value, json};

use phi_contracts::ports::{InvalidToolCall, ModelError, ModelRequest, ToolDefinition};
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_i18n as i18n;

use super::common::{
    PreparedRequest, ProtocolResponse, StreamDecoder, StreamDelta, model_error, parse_arguments,
};

pub(super) fn build_body(req: &ModelRequest, prepared: &PreparedRequest, stream: bool) -> Value {
    let last_assistant = req
        .messages
        .iter()
        .rposition(|message| message.role == Role::Assistant);
    let mut body = json!({
        "model": &prepared.model,
        "max_tokens": prepared.max_tokens,
        "messages": req.messages.iter().enumerate()
            .map(|(index, message)| {
                // Only the most recent assistant turn's thinking blocks
                // are needed for tool-use continuation. Older ones are
                // dropped so history edits do not invalidate the request.
                message_to_anthropic(message, Some(index) == last_assistant, &req.images)
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
    if let Some(effort) = prepared.effort {
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

pub(super) fn message_to_anthropic(
    message: &ChatMessage,
    keep_thinking: bool,
    images: &HashMap<String, Vec<u8>>,
) -> Value {
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
            ContentBlock::Image { asset_id, mime } => images.get(asset_id).map(|bytes| {
                json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime,
                        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    }
                })
            }),
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

fn tool_to_anthropic(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

pub(super) fn parse(value: &Value) -> Result<ProtocolResponse, ModelError> {
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

#[derive(Default)]
pub(super) struct AnthropicDecoder {
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
