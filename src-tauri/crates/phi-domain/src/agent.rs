use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which side of a delegation an agent instance is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Lead,
    Subagent,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Lead => "lead",
            AgentRole::Subagent => "subagent",
        }
    }
}

/// Current lifecycle of an agent instance. The instance can outlive a single
/// run, so this is derived from the latest run plus explicit status events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Created,
    Running,
    WaitingApproval,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl AgentStatus {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentStatus::Created | AgentStatus::Running | AgentStatus::WaitingApproval
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentStatus::Succeeded
                | AgentStatus::Failed
                | AgentStatus::Cancelled
                | AgentStatus::Interrupted
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AgentStatus::Created => "created",
            AgentStatus::Running => "running",
            AgentStatus::WaitingApproval => "waiting_approval",
            AgentStatus::Succeeded => "succeeded",
            AgentStatus::Failed => "failed",
            AgentStatus::Cancelled => "cancelled",
            AgentStatus::Interrupted => "interrupted",
        }
    }
}

/// How a subagent may touch the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkMode {
    Shared,
    ReadOnly,
}

impl WorkMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "shared" | "write" => Some(WorkMode::Shared),
            "read_only" | "readonly" | "read" => Some(WorkMode::ReadOnly),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkMode::Shared => "shared",
            WorkMode::ReadOnly => "read_only",
        }
    }
}

/// Product-level message category. The provider still only sees user /
/// assistant roles; `kind` explains where a message came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    User,
    Assistant,
    Tool,
    Brief,
    Message,
    Report,
    System,
}

/// Read model for one agent instance.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub role: AgentRole,
    pub parent_agent_id: Option<String>,
    pub root_agent_id: String,
    pub model: String,
    pub thinking: Option<String>,
    pub thread_id: String,
    pub work_mode: WorkMode,
    pub status: AgentStatus,
    pub report_to: Option<String>,
    pub spawned_by_run_id: String,
    pub report_message_id: Option<String>,
    pub steps: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub created_at: String,
    pub updated_at: String,
}
