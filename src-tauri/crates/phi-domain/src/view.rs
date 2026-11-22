use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::agent::{AgentView, MessageKind};
use super::message::{ContentBlock, Role};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub last_run_state: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub turn_id: Option<String>,
    pub role: Role,
    pub ts: String,
    pub blocks: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MessageKind>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub id: String,
    pub turn_id: String,
    pub state: String,
    pub steps: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    pub pending_approval_id: Option<String>,
    pub error: Option<String>,
    pub stop_confirmation: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
}

impl RunView {
    pub fn is_active(&self) -> bool {
        matches!(
            self.state.as_str(),
            "Scheduled" | "Running" | "WaitingApproval"
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub owner_node_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<MessageView>,
    pub runs: Vec<RunView>,
    #[serde(default)]
    pub agents: Vec<AgentView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PendingApprovalView {
    pub id: String,
    pub session_id: String,
    pub run_id: String,
    pub invocation_id: String,
    pub tool_id: String,
    pub args_preview: Value,
    pub action_digest: String,
    /// Localized human-readable risk sentence.
    pub risk: String,
    /// Structured risk tier (`low`, `medium`, `high`).
    pub risk_level: String,
    pub requested_at: String,
    pub expires_at: String,
}
