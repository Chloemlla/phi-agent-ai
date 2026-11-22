//! Shared builder for tool definitions announced to the model.

use serde_json::{Value, json};

use phi_contracts::ports::{SideEffect, ToolCapability, ToolDefinition};

/// Build a tool definition whose protocol name equals its id.
pub(crate) fn tool(
    id: &str,
    description: &str,
    properties: Value,
    required: Vec<&str>,
    side_effect: SideEffect,
    capabilities: Vec<ToolCapability>,
) -> ToolDefinition {
    ToolDefinition {
        id: id.to_string(),
        name: id.to_string(),
        description: description.to_string(),
        input_schema: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
        side_effect,
        capabilities,
    }
}
