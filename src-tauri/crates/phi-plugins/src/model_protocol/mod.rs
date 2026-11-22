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

mod anthropic;
mod common;
mod openai_chat;
mod openai_responses;

#[cfg(test)]
mod tests;

use phi_contracts::ports::{ModelError, ModelRequest};
use phi_contracts::provider::{Protocol, ProviderProfile};
use serde_json::Value;

pub use common::{
    HttpPlan, ProtocolResponse, StreamDecoder, StreamDelta, conservative_request_tokens,
    endpoint_url, model_error, models_url, parse_http_error,
};

pub fn decoder(protocol: Protocol) -> Box<dyn StreamDecoder> {
    match protocol {
        Protocol::AnthropicMessages => Box::new(anthropic::AnthropicDecoder::default()),
        Protocol::OpenAiChat => Box::new(openai_chat::ChatDecoder::default()),
        Protocol::OpenAiResponses => Box::new(openai_responses::ResponsesDecoder::default()),
    }
}

pub fn build_http(
    profile: &ProviderProfile,
    req: &ModelRequest,
    stream: bool,
) -> Result<HttpPlan, ModelError> {
    let url = endpoint_url(profile);
    let headers = common::request_headers(profile, req)?;
    let prepared = common::prepare_request(profile, req)?;

    let body = match profile.protocol {
        Protocol::AnthropicMessages => anthropic::build_body(req, &prepared, stream),
        Protocol::OpenAiChat => openai_chat::build_body(profile, req, &prepared, stream),
        Protocol::OpenAiResponses => openai_responses::build_body(profile, req, &prepared, stream),
    };

    Ok(HttpPlan { url, headers, body })
}

pub fn parse_response(protocol: Protocol, value: &Value) -> Result<ProtocolResponse, ModelError> {
    match protocol {
        Protocol::AnthropicMessages => anthropic::parse(value),
        Protocol::OpenAiChat => openai_chat::parse(value),
        Protocol::OpenAiResponses => openai_responses::parse(value),
    }
}
