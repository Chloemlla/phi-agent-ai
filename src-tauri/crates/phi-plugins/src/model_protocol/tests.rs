use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use phi_contracts::ports::{ModelRequest, ThinkingEffort, ToolDefinition};
use phi_contracts::provider::{Protocol, ProviderProfile};
use phi_domain::message::{ChatMessage, ContentBlock, Role};

use super::anthropic::message_to_anthropic;
use super::*;

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
            capabilities: vec![],
        }],
        max_output_tokens: 100,
        thinking: None,
        call_id: "call".into(),
        run_id: "run".into(),
        stream: true,
        images: Arc::new(HashMap::new()),
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
    let wire = message_to_anthropic(&message, true, &HashMap::new());
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
            &json!({"type":"response.output_text.delta","output_index":0,"delta":"do"}).to_string(),
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
fn xhigh_and_max_pass_through_each_protocol() {
    for effort in [ThinkingEffort::XHigh, ThinkingEffort::Max] {
        let expected = effort.as_str();

        let mut anthropic = profile(Protocol::AnthropicMessages);
        anthropic.model = "claude-sonnet-5-5".into();
        let mut req = request();
        req.thinking = Some(effort);
        let plan = build_http(&anthropic, &req, false).unwrap();
        assert_eq!(plan.body["output_config"]["effort"], expected);

        let mut chat = profile(Protocol::OpenAiChat);
        chat.model = "gpt-6-astra".into();
        let plan = build_http(&chat, &req, false).unwrap();
        assert_eq!(plan.body["reasoning_effort"], expected);

        let mut responses = profile(Protocol::OpenAiResponses);
        responses.model = "gpt-6-sol".into();
        let plan = build_http(&responses, &req, false).unwrap();
        assert_eq!(plan.body["reasoning"]["effort"], expected);
    }
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
