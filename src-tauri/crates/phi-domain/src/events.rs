use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::agent::{AgentRole, AgentStatus, MessageKind, WorkMode};
use super::memory::{MemoryScope, SourceTrust};
use super::message::Role;

/// Domain event catalog (MVP subset).
///
/// Events are serialized into the journal envelope's `payload` with a `type`
/// discriminator. All events on the session line use the session id as the
/// aggregate id, so a single write order gives a restorable session stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum DomainEvent {
    // ── session / turn ────────────────────────────────────────────────
    SessionStarted {
        session_id: String,
        owner_node_id: String,
        title: String,
    },
    SessionArchived {
        session_id: String,
    },
    SessionRenamed {
        session_id: String,
        title: String,
        /// `user`, `auxiliary` or `import`.
        source: String,
    },
    TurnStarted {
        session_id: String,
        turn_id: String,
        index: u64,
    },
    TurnClosed {
        turn_id: String,
        terminal_run_id: String,
    },
    MessageAppended {
        session_id: String,
        turn_id: Option<String>,
        message_id: String,
        role: Role,
        content_ref: String,
        /// Message thread: `main` for the user/lead conversation,
        /// `agent:<agentId>` for a subagent. Missing on old journals.
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        to_agent_id: Option<String>,
        #[serde(default)]
        kind: Option<MessageKind>,
    },

    // ── agents ────────────────────────────────────────────────────────
    AgentCreated {
        agent_id: String,
        session_id: String,
        turn_id: String,
        role: AgentRole,
        parent_agent_id: Option<String>,
        root_agent_id: String,
        model: String,
        thinking: Option<String>,
        thread_id: String,
        work_mode: WorkMode,
        report_to: Option<String>,
        spawned_by_run_id: String,
    },
    AgentStatusChanged {
        agent_id: String,
        session_id: String,
        status: AgentStatus,
        note: Option<String>,
    },
    AgentReported {
        agent_id: String,
        session_id: String,
        run_id: String,
        status: AgentStatus,
        report_message_id: String,
        delivered_to: Option<String>,
    },

    // ── optional model surfaces ───────────────────────────────────────
    ImageGenerated {
        asset_id: String,
        session_id: Option<String>,
        model: String,
        mime: String,
        width: Option<u32>,
        height: Option<u32>,
        prompt_preview: String,
    },
    ImageDeleted {
        asset_id: String,
    },
    DecisionRequested {
        call_id: String,
        point: String,
        model: String,
        state_chars: u64,
        session_id: Option<String>,
        run_id: Option<String>,
    },
    DecisionReturned {
        call_id: String,
        point: String,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
    },
    DecisionFailed {
        call_id: String,
        point: String,
        error: String,
    },
    DecisionAutoApproved {
        call_id: String,
        tool_id: String,
        action_digest: String,
        threshold: f32,
        probability: f32,
    },
    ToolOutputCompacted {
        invocation_id: String,
        run_id: String,
        strategy: String,
        before_chars: u64,
        after_chars: u64,
        blob_ref: String,
    },
    AuxiliaryCallIssued {
        call_id: String,
        role: String,
        provider_id: Option<String>,
        model: String,
        purpose: String,
        session_id: Option<String>,
    },
    AuxiliaryCallReturned {
        call_id: String,
        role: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    AuxiliaryCallFailed {
        call_id: String,
        role: String,
        error: String,
    },
    MemoryMaintenanceStarted {
        trigger: String,
        batch_size: u64,
        dry_run: bool,
    },
    MemoryMaintenanceProposed {
        proposal_count: u64,
        kinds: Vec<String>,
    },
    MemoryMaintenanceApplied {
        applied: u64,
        pending: u64,
        reason: String,
    },

    // ── run / step ────────────────────────────────────────────────────
    RunScheduled {
        run_id: String,
        session_id: String,
        turn_id: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        parent_run_id: Option<String>,
    },
    RunStarted {
        run_id: String,
    },
    StepStarted {
        run_id: String,
        step_id: String,
        index: u64,
        phase: String,
    },
    StepCompleted {
        run_id: String,
        step_id: String,
    },
    RunWaitingApproval {
        run_id: String,
        approval_id: String,
    },
    RunResumed {
        run_id: String,
        approval_id: String,
    },
    RunSucceeded {
        run_id: String,
    },
    RunFailed {
        run_id: String,
        error: String,
    },
    RunAborted {
        run_id: String,
        reason: String,
        stop_confirmation: String,
    },
    RunAwaitingReconciliation {
        run_id: String,
        note: String,
    },
    RunInterrupted {
        run_id: String,
        session_id: String,
        note: String,
    },

    // ── model calls ───────────────────────────────────────────────────
    ModelCallIssued {
        call_id: String,
        run_id: String,
        step_id: String,
        model: String,
        max_output_tokens: u32,
        /// Character counts of the assembled prompt segments. Tokens are not
        /// counted locally; the UI labels the resulting split as estimated.
        #[serde(default)]
        system_chars: u64,
        #[serde(default)]
        tools_chars: u64,
        #[serde(default)]
        history_chars: u64,
        #[serde(default)]
        context_window: Option<u64>,
    },
    ModelCallReturned {
        call_id: String,
        run_id: String,
        input_tokens: u64,
        output_tokens: u64,
        #[serde(default)]
        cache_read_tokens: u64,
        #[serde(default)]
        cache_write_tokens: u64,
        #[serde(default)]
        duration_ms: u64,
        finish_reason: String,
    },
    ModelCallFailed {
        call_id: String,
        run_id: String,
        error: String,
    },

    // ── tools ─────────────────────────────────────────────────────────
    ToolInvokeRequested {
        invocation_id: String,
        run_id: String,
        step_id: String,
        tool_id: String,
        args_preview: Value,
        action_digest: String,
    },
    ToolInvokeDecided {
        invocation_id: String,
        run_id: String,
        decision: String,
        rationale: String,
    },
    ToolInvokeStarted {
        invocation_id: String,
        run_id: String,
    },
    ToolInvokeCompleted {
        invocation_id: String,
        run_id: String,
        result_ref: String,
        is_error: bool,
        duration_ms: u64,
    },
    ToolInvokeFailed {
        invocation_id: String,
        run_id: String,
        error: String,
    },
    ToolInvokeOutcomeUnknown {
        invocation_id: String,
        run_id: String,
        evidence: String,
    },

    // ── retired ───────────────────────────────────────────────────────
    /// Retired workbench protocol. Kept only so journals written before
    /// 2026-11-20 still replay; nothing publishes it and projections ignore
    /// the stored view.
    UiViewUpdated {
        run_id: String,
        view: Value,
    },

    // ── approval ──────────────────────────────────────────────────────
    ApprovalRequested {
        approval_id: String,
        invocation_id: String,
        run_id: String,
        tool_id: String,
        action_digest: String,
        /// Localized human-readable risk sentence.
        risk: String,
        /// Structured risk tier (`low`, `medium`, `high`). Defaulted so
        /// journals written before this field existed still deserialize.
        #[serde(default)]
        risk_level: String,
        args_preview: Value,
        expires_at: String,
    },
    ApprovalResolved {
        approval_id: String,
        status: String,
        by: String,
    },
    ApprovalExpired {
        approval_id: String,
    },

    // ── memory ────────────────────────────────────────────────────────
    // Memory events are metadata only. The fact text lives in the memory
    // store so `forget` can erase it completely; the journal keeps an
    // audit trail that survives the deletion without retaining content.
    MemoryAdmitted {
        memory_id: String,
        scope: MemoryScope,
        source_trust: SourceTrust,
        confidence: f64,
        fact_chars: u64,
    },
    MemoryRejected {
        candidate_id: String,
        scope: MemoryScope,
        reason: String,
    },
    MemoryInvalidated {
        memory_id: String,
        reason: String,
    },
    MemorySuperseded {
        old_id: String,
        new_id: String,
        reason: String,
    },
    MemoryForgotten {
        /// Selector summary (for example `all`, `ids:3` or `run:run_x`).
        scope: String,
        records: u64,
        episodes: u64,
        reason: String,
    },
}
