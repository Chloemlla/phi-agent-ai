use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    },

    // ── task ──────────────────────────────────────────────────────────
    TaskCreated {
        task_id: String,
        title: String,
        implicit: bool,
    },

    // ── run / step ────────────────────────────────────────────────────
    RunScheduled {
        run_id: String,
        session_id: String,
        turn_id: String,
        task_id: String,
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
        risk: String,
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
