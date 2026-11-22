use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::agent::{AgentView, MessageKind};
use super::message::{ContentBlock, Role};

/// Lifecycle state of a run. The wire representation is the PascalCase name
/// used by the journal, headless JSON and the desktop shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "PascalCase")]
pub enum RunState {
    Scheduled,
    Running,
    WaitingApproval,
    Succeeded,
    Failed,
    Aborted,
    AwaitingReconciliation,
}

impl RunState {
    pub const fn as_str(self) -> &'static str {
        match self {
            RunState::Scheduled => "Scheduled",
            RunState::Running => "Running",
            RunState::WaitingApproval => "WaitingApproval",
            RunState::Succeeded => "Succeeded",
            RunState::Failed => "Failed",
            RunState::Aborted => "Aborted",
            RunState::AwaitingReconciliation => "AwaitingReconciliation",
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(
            self,
            RunState::Scheduled | RunState::Running | RunState::WaitingApproval
        )
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Succeeded
                | RunState::Failed
                | RunState::Aborted
                | RunState::AwaitingReconciliation
        )
    }
}

impl std::fmt::Display for RunState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub last_run_state: Option<RunState>,
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
    pub state: RunState,
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
        self.state.is_active()
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
    /// Prompt size and session throughput for the composer's context meter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsageView>,
}

/// Context and throughput readings for the current session.
///
/// `used_tokens` and `context_window` are real provider readings. The
/// per-segment token split is estimated by the UI from the character counts,
/// because no tokenizer runs locally; the DTO therefore exposes characters,
/// not fabricated token counts.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageView {
    #[serde(default)]
    pub context_window: Option<u64>,
    pub used_tokens: u64,
    pub system_chars: u64,
    pub tools_chars: u64,
    pub history_chars: u64,
    pub turns: u64,
    pub steps: u64,
    pub total_tokens: u64,
    pub average_tps: f64,
    pub cache_hit: f64,
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
