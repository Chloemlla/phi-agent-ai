use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::Value;

use phi_contracts::auxiliary::{DecisionAnswer, DecisionQuestion, DecisionRequest};
use phi_contracts::ports::{
    ApprovalStatus, InvalidToolCall, PolicyEffect, PolicyInput, SideEffect, ToolCall,
    ToolCapability, ToolContext,
};
use phi_contracts::util::{
    ResourceIdentity, action_digest, args_preview, resource_identity, truncate_chars,
};
use phi_domain::agent::AgentStatus;
use phi_domain::events::DomainEvent;
use phi_domain::message::ContentBlock;
use phi_domain::view::RunState;
use phi_i18n as i18n;

use crate::memory::redact_secrets;

use super::RunContext;
use super::support::{env_u64, publish_agent_status, set_status};

pub(super) struct ToolExecOutcome {
    pub(super) block: ContentBlock,
    pub(super) cancelled: bool,
    pub(super) unknown_side_effect: bool,
}

pub(super) async fn maybe_auto_approve(
    ctx: &RunContext,
    call: &ToolCall,
    digest: &str,
    risk_level: &str,
) -> bool {
    let Some(decision) = ctx.services.decision() else {
        return false;
    };
    let config = decision.triage();
    // The configured tool list is the user's pre-authorization. The host risk
    // level is shown to the judge but does not veto triage; a tool that is not
    // listed is never sent to the decision model.
    if !config.enabled || !config.tools.iter().any(|tool| tool == &call.tool_id) {
        return false;
    }
    let args = serde_json::to_string(&journal_args_preview(&call.tool_id, &call.input))
        .unwrap_or_default();
    let args = redact_secrets(&args).text;
    let state = truncate_chars(
        &format!(
            "Tool: {}\nHost risk level: {}\nWorkspace: {}\nArguments: {}",
            call.tool_id,
            risk_level,
            ctx.workspace.display(),
            args
        ),
        4_000,
    );
    let mut questions = BTreeMap::new();
    questions.insert(
        "approve".to_string(),
        DecisionQuestion::Noul {
            instructions: "Is this action within the low-risk category the user pre-authorized for automatic approval? Consider scope, reversibility and whether the arguments look normal.".to_string(),
        },
    );
    let response = match decision
        .evaluate(
            DecisionRequest {
                point: "approval_triage".to_string(),
                state,
                questions,
                session_id: Some(ctx.session_id.clone()),
                run_id: Some(ctx.run_id.clone()),
            },
            ctx.cancel.clone(),
        )
        .await
    {
        Ok(response) => response,
        Err(_) => return false,
    };
    let Some(DecisionAnswer::Noul { probability }) = response.answers.get("approve") else {
        return false;
    };
    if *probability < config.threshold {
        return false;
    }
    let rationale = i18n::t_args(
        "Auto-approved by the decision model (probability {probability}, threshold {threshold})",
        &[
            ("probability", format!("{probability:.2}")),
            ("threshold", format!("{:.2}", config.threshold)),
        ],
    );
    if decide(ctx, call, "allow", &rationale).is_err() {
        return false;
    }
    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::DecisionAutoApproved {
            call_id: response.call_id,
            tool_id: call.tool_id.clone(),
            action_digest: digest.to_string(),
            threshold: config.threshold,
            probability: *probability,
        },
    );
    true
}

pub(super) async fn execute_tool(
    ctx: &RunContext,
    step_id: &str,
    call: ToolCall,
) -> ToolExecOutcome {
    let digest = action_digest_for(ctx, &call);
    let definition = ctx.services.tool_definition(&call.tool_id);
    let capabilities = definition
        .as_ref()
        .map(|definition| definition.capabilities.clone())
        .unwrap_or_default();
    let side_effect = definition
        .as_ref()
        .map(|definition| definition.side_effect)
        .unwrap_or(SideEffect::Irreversible);
    let has = |capability| capabilities.contains(&capability);
    // The request record must be durable before anything else happens; if the
    // journal is unwritable we must not proceed to a side effect.
    if let Err(error) = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::ToolInvokeRequested {
            invocation_id: call.id.clone(),
            run_id: ctx.run_id.clone(),
            step_id: step_id.to_string(),
            tool_id: call.tool_id.clone(),
            args_preview: journal_args_preview(&call.tool_id, &call.input),
            action_digest: digest.clone(),
        },
    ) {
        return ToolExecOutcome {
            block: ContentBlock::tool_result(
                call.id.clone(),
                i18n::t_args(
                    "Action not executed: journal write failed ({error}).",
                    &[("error", error.to_string())],
                ),
                true,
            ),
            cancelled: false,
            unknown_side_effect: false,
        };
    }

    // A shared subagent owns the workspace writer lease; the lead and other
    // subagents must not write until it finishes or is cancelled.
    if has(ToolCapability::WorkspaceWrite) || has(ToolCapability::Shell) {
        if let Some(holder) = ctx
            .services
            .session()
            .and_then(|session| session.write_lease_holder(&ctx.session_id))
        {
            if holder != ctx.agent_id {
                let rationale = i18n::t_args(
                    "Another agent holds the workspace write lease ({agent}); the action was denied.",
                    &[("agent", holder)],
                );
                let _ = decide(ctx, &call, "deny", &rationale);
                return deny_result(&call, &rationale);
            }
        }
    }

    let policy_input = PolicyInput {
        tool_id: call.tool_id.clone(),
        args: call.input.clone(),
        workspace: ctx.workspace.clone(),
        capabilities: capabilities.clone(),
        side_effect,
    };

    let Some(policy) = ctx.services.policy() else {
        let rationale = i18n::t("Policy component is unavailable; denied by default");
        let _ = decide(ctx, &call, "deny", &rationale);
        return deny_result(&call, &rationale);
    };
    let decision = policy.evaluate(&policy_input);
    let risk_level = if has(ToolCapability::Shell) {
        "high"
    } else {
        "medium"
    };
    let triaged = if decision.effect == PolicyEffect::Ask {
        maybe_auto_approve(ctx, &call, &digest, risk_level).await
    } else {
        false
    };

    match decision.effect {
        PolicyEffect::Deny => {
            let _ = decide(ctx, &call, "deny", &decision.rationale);
            return deny_result(&call, &decision.rationale);
        }
        PolicyEffect::Ask if triaged => {}
        PolicyEffect::Ask => {
            if let Err(error) = decide(ctx, &call, "ask", &decision.rationale) {
                return tool_not_executed(
                    &call,
                    &i18n::t_args(
                        "Action not executed: journal write failed ({error}).",
                        &[("error", error.to_string())],
                    ),
                );
            }
            let Some(approver) = ctx.services.approver() else {
                let rationale = i18n::t("Approval component is unavailable; denied by default");
                let _ = decide(ctx, &call, "deny", &rationale);
                return deny_result(&call, &rationale);
            };
            let risk = if has(ToolCapability::Shell) {
                i18n::t("Irreversible: code execution and permissions")
            } else {
                i18n::t("Reversible: data modification")
            };
            // Approval has its own timeout, independent of any run budget.
            let expires_in_secs = env_u64("PHI_APPROVAL_TIMEOUT_SECS", 300);
            let spec = phi_contracts::ports::ApprovalSpec {
                session_id: ctx.session_id.clone(),
                run_id: ctx.run_id.clone(),
                invocation_id: call.id.clone(),
                tool_id: call.tool_id.clone(),
                args_preview: journal_args_preview(&call.tool_id, &call.input),
                action_digest: digest.clone(),
                risk,
                risk_level: risk_level.to_string(),
                expires_in_secs,
            };
            let pending = match approver.request(spec) {
                Ok(pending) => pending,
                Err(error) => {
                    let rationale =
                        i18n::t_args("Failed to create approval: {error}", &[("error", error)]);
                    let _ = decide(ctx, &call, "deny", &rationale);
                    return deny_result(&call, &rationale);
                }
            };
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::RunWaitingApproval {
                    run_id: ctx.run_id.clone(),
                    approval_id: pending.id.clone(),
                },
            ) {
                let _ = approver.resolve(&pending.id, false, Some(i18n::t("Journal unavailable")));
                return tool_not_executed(
                    &call,
                    &i18n::t_args(
                        "Action not executed: journal write failed ({error}).",
                        &[("error", error.to_string())],
                    ),
                );
            }
            set_status(ctx, RunState::WaitingApproval);
            publish_agent_status(ctx, AgentStatus::WaitingApproval, None);
            let outcome = approver.wait(&pending.id, ctx.cancel.clone()).await;
            if outcome.status != ApprovalStatus::Approved {
                let status = outcome.status.as_str().to_string();
                let rationale = outcome.note.unwrap_or_else(|| {
                    i18n::t_args("Approval outcome: {status}", &[("status", status)])
                });
                let _ = decide(ctx, &call, "deny", &rationale);
                if ctx.cancel.is_cancelled() {
                    return ToolExecOutcome {
                        block: ContentBlock::tool_result(
                            call.id.clone(),
                            i18n::t("Run cancelled; action was not executed."),
                            true,
                        ),
                        cancelled: true,
                        unknown_side_effect: false,
                    };
                }
                return deny_result(&call, &rationale);
            }
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::RunResumed {
                    run_id: ctx.run_id.clone(),
                    approval_id: pending.id.clone(),
                },
            ) {
                return tool_not_executed(
                    &call,
                    &i18n::t_args(
                        "Action not executed: journal write failed ({error}).",
                        &[("error", error.to_string())],
                    ),
                );
            }
            set_status(ctx, RunState::Running);
            publish_agent_status(ctx, AgentStatus::Running, None);
            // Approval is bound to the resolved resource identity and content,
            // not just the path string. Recompute and compare before running.
            let current = action_digest_for(ctx, &call);
            if current != pending.action_digest {
                let rationale =
                    i18n::t("The action changed after approval; the approval is no longer valid");
                let _ = decide(ctx, &call, "deny", &rationale);
                return deny_result(&call, &rationale);
            }
            if let Err(error) = decide(ctx, &call, "allow", &i18n::t("Human approval granted")) {
                return tool_not_executed(
                    &call,
                    &i18n::t_args(
                        "Action not executed: journal write failed ({error}).",
                        &[("error", error.to_string())],
                    ),
                );
            }
        }
        PolicyEffect::Allow => {
            if let Err(error) = decide(ctx, &call, "allow", &decision.rationale) {
                return tool_not_executed(
                    &call,
                    &i18n::t_args(
                        "Action not executed: journal write failed ({error}).",
                        &[("error", error.to_string())],
                    ),
                );
            }
        }
    }

    if ctx.services.sandbox().is_none() {
        let error = i18n::t(
            "Sandbox component is unavailable; execution denied (never run without isolation)",
        );
        publish_tool_failed(ctx, &call, &error);
        return ToolExecOutcome {
            block: ContentBlock::tool_result(call.id.clone(), error, true),
            cancelled: false,
            unknown_side_effect: false,
        };
    }
    // The tool provider owns path resolution/exec via the same port.

    let Some(tool) = ctx.services.find_tool(&call.tool_id) else {
        let error = i18n::t_args(
            "No tool implementation: {tool_id}",
            &[("tool_id", call.tool_id.clone())],
        );
        publish_tool_failed(ctx, &call, &error);
        return ToolExecOutcome {
            block: ContentBlock::tool_result(call.id.clone(), error, true),
            cancelled: false,
            unknown_side_effect: false,
        };
    };

    if ctx.cancel.is_cancelled() {
        return ToolExecOutcome {
            block: ContentBlock::tool_result(
                call.id.clone(),
                i18n::t("Run cancelled; action was not executed."),
                true,
            ),
            cancelled: true,
            unknown_side_effect: false,
        };
    }

    // The start record must be durable before the side effect begins.
    if let Err(error) = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::ToolInvokeStarted {
            invocation_id: call.id.clone(),
            run_id: ctx.run_id.clone(),
        },
    ) {
        return tool_not_executed(
            &call,
            &i18n::t_args(
                "Action not executed: journal write failed ({error}).",
                &[("error", error.to_string())],
            ),
        );
    }

    let tool_ctx = ToolContext {
        workspace: ctx.workspace.clone(),
        session_id: ctx.session_id.clone(),
        turn_id: ctx.turn_id.clone(),
        run_id: ctx.run_id.clone(),
        agent_id: ctx.agent_id.clone(),
        root_agent_id: ctx.root_agent_id.clone(),
        thread_id: ctx.thread_id.clone(),
        role: ctx.role,
        parent_run_id: ctx.parent_run_id.clone(),
        work_mode: ctx.work_mode,
        lead_scale: ctx.lead_scale,
    };
    let started = Instant::now();
    // Do not drop this future on cancellation: the execution layer (sandbox)
    // receives the token, kills the process tree and only then returns. Racing
    // it against the token here would abandon the termination confirmation.
    let result = tool.invoke(&call, &tool_ctx, ctx.cancel.clone()).await;

    match result {
        Ok(output) => {
            let duration_ms = started.elapsed().as_millis() as u64;
            let result_ref = ctx
                .blob
                .put(output.content.as_bytes())
                .unwrap_or_else(|_| "unavailable".to_string());
            // The side effect already happened. If its completion record
            // cannot be persisted, report that the audit trail is incomplete.
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::ToolInvokeCompleted {
                    invocation_id: call.id.clone(),
                    run_id: ctx.run_id.clone(),
                    result_ref: result_ref.clone(),
                    is_error: output.is_error,
                    duration_ms,
                },
            ) {
                return ToolExecOutcome {
                    block: ContentBlock::tool_result(
                        call.id.clone(),
                        i18n::t_args(
                            "Action executed, but its audit record could not be written: {error}",
                            &[("error", error)],
                        ),
                        true,
                    ),
                    cancelled: false,
                    unknown_side_effect: side_effect == SideEffect::Irreversible,
                };
            }
            let content =
                compact_tool_output(ctx, &call, output.content, output.is_error, &result_ref).await;
            ToolExecOutcome {
                block: ContentBlock::tool_result(call.id.clone(), content, output.is_error),
                cancelled: false,
                unknown_side_effect: false,
            }
        }
        Err(error) => {
            if error == "cancelled" {
                let evidence = i18n::t(
                    "The command was terminated and we cannot confirm whether side effects occurred; the run is awaiting reconciliation and will not retry automatically.",
                );
                let _ = ctx.bus.publish(
                    &ctx.session_id,
                    "plugin-loop",
                    &DomainEvent::ToolInvokeOutcomeUnknown {
                        invocation_id: call.id.clone(),
                        run_id: ctx.run_id.clone(),
                        evidence: evidence.clone(),
                    },
                );
                ToolExecOutcome {
                    block: ContentBlock::tool_result(call.id.clone(), evidence, true),
                    cancelled: true,
                    unknown_side_effect: side_effect == SideEffect::Irreversible,
                }
            } else {
                publish_tool_failed(ctx, &call, &error);
                ToolExecOutcome {
                    block: ContentBlock::tool_result(call.id.clone(), error, true),
                    cancelled: false,
                    unknown_side_effect: false,
                }
            }
        }
    }
}

pub(super) async fn compact_tool_output(
    ctx: &RunContext,
    call: &ToolCall,
    content: String,
    is_error: bool,
    blob_ref: &str,
) -> String {
    let Some(decision) = ctx.services.decision() else {
        return truncate_chars(&content, 20_000);
    };
    let config = decision.compaction();
    let before = content.chars().count();
    if !config.enabled || before < config.min_chars {
        return truncate_chars(&content, 20_000);
    }
    let preview = redact_secrets(&content.chars().take(2_000).collect::<String>()).text;
    let state = format!(
        "Tool: {}\nCharacters: {}\nIs error: {}\nFull output blob: {}\nPreview:\n{}",
        call.tool_id, before, is_error, blob_ref, preview
    );
    let mut criteria = BTreeMap::new();
    criteria.insert(
        "keep_full".to_string(),
        "The full output is necessary for the next step".to_string(),
    );
    criteria.insert(
        "summarize".to_string(),
        "A summary can replace the full text; details stay in the blob".to_string(),
    );
    criteria.insert(
        "head_tail".to_string(),
        "Keep only the beginning and end".to_string(),
    );
    criteria.insert(
        "reference_only".to_string(),
        "Only the fact that the output exists is needed".to_string(),
    );
    let mut questions = BTreeMap::new();
    questions.insert(
        "strategy".to_string(),
        DecisionQuestion::Choice {
            instructions: "How should this tool output enter the model context?".to_string(),
            criteria,
        },
    );
    let strategy = match decision
        .evaluate(
            DecisionRequest {
                point: "tool_output_compaction".to_string(),
                state,
                questions,
                session_id: Some(ctx.session_id.clone()),
                run_id: Some(ctx.run_id.clone()),
            },
            ctx.cancel.clone(),
        )
        .await
    {
        Ok(response) => match response.answers.get("strategy") {
            Some(DecisionAnswer::Choice {
                choice, confidence, ..
            }) if *confidence >= config.threshold => choice.clone(),
            _ => "head_tail".to_string(),
        },
        Err(_) => "head_tail".to_string(),
    };
    let mut applied_strategy = strategy.clone();
    let compacted = match strategy.as_str() {
        "keep_full" => return truncate_chars(&content, 20_000),
        "summarize" => {
            let summary = match ctx.services.auxiliary() {
                Some(auxiliary) if auxiliary.summarizer_ready() => {
                    match tokio::time::timeout(
                        Duration::from_secs(20),
                        auxiliary.summarize(&content, 4_000, ctx.cancel.clone()),
                    )
                    .await
                    {
                        Ok(Ok(Some(summary))) => Some(summary),
                        _ => None,
                    }
                }
                _ => None,
            };
            match summary {
                Some(summary) => {
                    format!("{summary}\n\n(full output saved as {blob_ref}; {before} chars)")
                }
                None => {
                    applied_strategy = "head_tail".to_string();
                    head_tail(&content, blob_ref)
                }
            }
        }
        "reference_only" => {
            format!("(full output saved as {blob_ref}; {before} chars)")
        }
        _ => {
            applied_strategy = "head_tail".to_string();
            head_tail(&content, blob_ref)
        }
    };
    let after = compacted.chars().count();
    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::ToolOutputCompacted {
            invocation_id: call.id.clone(),
            run_id: ctx.run_id.clone(),
            strategy: applied_strategy,
            before_chars: before as u64,
            after_chars: after as u64,
            blob_ref: blob_ref.to_string(),
        },
    );
    truncate_chars(&compacted, 20_000)
}

pub(super) fn head_tail(content: &str, blob_ref: &str) -> String {
    let total = content.chars().count();
    if total <= 6_000 {
        return content.to_string();
    }
    let head: String = content.chars().take(4_000).collect();
    let tail: String = content.chars().skip(total - 2_000).collect();
    format!("{head}\n… (truncated; full output: {blob_ref})\n{tail}")
}

/// Compute the approval-bound action digest, including resolved resource
/// identity and content hash so a changed file, a swapped junction or a
/// different target invalidates an earlier approval.
pub(super) fn action_digest_for(ctx: &RunContext, call: &ToolCall) -> String {
    let mut resources = Vec::new();
    if let Some(requested) = call.input.get("path").and_then(|value| value.as_str()) {
        let (resolved, kind, content_hash) = match ctx.services.sandbox() {
            Some(sandbox) => match sandbox.resolve_path(&ctx.workspace, requested) {
                Ok(path) => {
                    // Only writes bind file contents; reading a large file must
                    // not pay for a content hash the approval never uses.
                    let hash_contents = call.tool_id == "fs_write";
                    let (kind, content_hash) = resource_identity(&path, hash_contents);
                    (path.to_string_lossy().to_string(), kind, content_hash)
                }
                Err(_) => ("<unresolved>".to_string(), "denied", None),
            },
            None => ("<no-sandbox>".to_string(), "unknown", None),
        };
        resources.push(ResourceIdentity {
            requested: requested.to_string(),
            resolved,
            kind,
            content_hash,
        });
    }
    action_digest(&call.tool_id, &call.input, &ctx.workspace, &resources)
}

pub(super) fn decide(
    ctx: &RunContext,
    call: &ToolCall,
    decision: &str,
    rationale: &str,
) -> Result<(), String> {
    ctx.bus
        .publish(
            &ctx.session_id,
            "plugin-loop",
            &DomainEvent::ToolInvokeDecided {
                invocation_id: call.id.clone(),
                run_id: ctx.run_id.clone(),
                decision: decision.to_string(),
                rationale: rationale.to_string(),
            },
        )
        .map(|_| ())
        .map_err(|error| {
            i18n::t_args(
                "Failed to record the tool decision: {error}",
                &[("error", error.to_string())],
            )
        })
}

pub(super) fn tool_not_executed(call: &ToolCall, reason: &str) -> ToolExecOutcome {
    ToolExecOutcome {
        block: ContentBlock::tool_result(call.id.clone(), reason.to_string(), true),
        cancelled: false,
        unknown_side_effect: false,
    }
}

pub(super) fn publish_tool_failed(ctx: &RunContext, call: &ToolCall, error: &str) {
    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::ToolInvokeFailed {
            invocation_id: call.id.clone(),
            run_id: ctx.run_id.clone(),
            error: error.to_string(),
        },
    );
}

/// Match a tool block to its parsed invalid entry. Exact id first. Only a
/// tool call whose id was generated during normalization may consume an
/// entry whose id is empty; otherwise a valid call could be rejected merely
/// because another call to the same tool was malformed. Consumed entries are
/// removed so one malformed call cannot mark two tool calls.
pub(super) fn take_invalid_call(
    invalid: &mut Vec<InvalidToolCall>,
    call_id: &str,
    tool_id: &str,
    generated: bool,
) -> Option<InvalidToolCall> {
    if let Some(index) = invalid
        .iter()
        .position(|entry| !entry.id.trim().is_empty() && entry.id == call_id)
    {
        return Some(invalid.remove(index));
    }
    if generated {
        if let Some(index) = invalid
            .iter()
            .position(|entry| entry.id.trim().is_empty() && entry.name == tool_id)
        {
            return Some(invalid.remove(index));
        }
        if let Some(index) = invalid.iter().position(|entry| entry.id.trim().is_empty()) {
            return Some(invalid.remove(index));
        }
    }
    None
}

/// Human- and model-facing explanation for a tool call whose streamed
/// arguments could not be parsed. It names the tool, includes the parser error
/// and a bounded raw prefix, and asks for a corrected call.
pub(super) fn invalid_tool_message(call: &InvalidToolCall) -> String {
    // Malformed arguments can contain a partially written secret. Redact
    // before the text is persisted in the transcript and journal.
    let raw = redact_secrets(&call.raw_arguments).text;
    i18n::t_args(
        "Tool {tool} was not executed: the model sent arguments that are not valid JSON. Parser error: {error}. Raw arguments (truncated): {raw}. Re-issue the tool call with valid JSON arguments.",
        &[
            ("tool", call.name.clone()),
            ("error", call.error.clone()),
            ("raw", truncate_chars(&raw, 2000)),
        ],
    )
}

pub(super) fn deny_result(call: &ToolCall, rationale: &str) -> ToolExecOutcome {
    ToolExecOutcome {
        block: ContentBlock::tool_result(
            call.id.clone(),
            i18n::t_args(
                "Action denied: {rationale}",
                &[("rationale", rationale.to_string())],
            ),
            true,
        ),
        cancelled: false,
        unknown_side_effect: false,
    }
}

/// Audit preview for the journal. Memory facts are redacted at admission, so
/// their pre-admission copy must not be journaled in the clear; the action
/// digest still binds the full arguments. This builds the preview from the raw
/// input because `args_preview` may truncate large payloads into a string.
pub(super) fn journal_args_preview(tool_id: &str, input: &Value) -> Value {
    match tool_id {
        "memory_write" => {
            let mut object = serde_json::Map::new();
            if let Some(scope) = input.get("scope") {
                object.insert("scope".to_string(), scope.clone());
            }
            if let Some(confidence) = input.get("confidence") {
                object.insert("confidence".to_string(), confidence.clone());
            }
            let chars = input
                .get("fact")
                .and_then(Value::as_str)
                .map(|fact| fact.chars().count())
                .unwrap_or(0);
            object.insert("factChars".to_string(), Value::from(chars));
            Value::Object(object)
        }
        "memory_search" => {
            let mut object = serde_json::Map::new();
            let query = input
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default();
            object.insert(
                "query".to_string(),
                Value::String(truncate_chars(&redact_secrets(query).text, 2000)),
            );
            if let Some(limit) = input.get("limit") {
                object.insert("limit".to_string(), limit.clone());
            }
            Value::Object(object)
        }
        _ => args_preview(tool_id, input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_write_preview_never_contains_the_fact() {
        let fact = format!(
            "The staging key is sk-test0000000000000000 {}",
            "x".repeat(4000)
        );
        let preview = journal_args_preview(
            "memory_write",
            &json!({ "fact": fact, "scope": "project", "confidence": 0.7 }),
        );
        let text = preview.to_string();
        assert!(
            !text.contains("staging key"),
            "preview leaked the fact: {text}"
        );
        assert!(!text.contains("sk-test"), "preview leaked a secret: {text}");
        assert_eq!(preview["scope"], "project");
        assert_eq!(preview["confidence"], 0.7);
        assert_eq!(
            preview["factChars"].as_u64().unwrap(),
            fact.chars().count() as u64
        );
    }

    #[test]
    fn memory_search_preview_redacts_secrets_but_keeps_shape() {
        let preview = journal_args_preview(
            "memory_search",
            &json!({ "query": "key sk-test0000000000000000", "limit": 3 }),
        );
        assert!(!preview.to_string().contains("sk-test"));
        assert_eq!(preview["limit"], 3);
        assert!(
            preview["query"]
                .as_str()
                .unwrap()
                .contains("[REDACTED:api-key]")
        );
    }

    #[test]
    fn other_tool_previews_are_unchanged() {
        let preview = journal_args_preview("fs_read", &json!({ "path": "src/main.rs" }));
        assert_eq!(preview["path"], "src/main.rs");
    }

    #[test]
    fn invalid_call_matching_survives_a_generated_id() {
        let mut invalid = vec![InvalidToolCall {
            id: String::new(),
            name: "fs_write".to_string(),
            block_index: 0,
            raw_arguments: "{bad".to_string(),
            error: "expected value".to_string(),
        }];
        let matched = take_invalid_call(&mut invalid, "call_generated", "fs_write", true)
            .expect("a generated id must still match its malformed entry");
        assert_eq!(matched.name, "fs_write");
        assert!(invalid.is_empty());

        // A valid call must not consume an entry that belongs to another call.
        let mut invalid = vec![InvalidToolCall {
            id: String::new(),
            name: "fs_write".to_string(),
            block_index: 0,
            raw_arguments: String::new(),
            error: "bad".to_string(),
        }];
        assert!(take_invalid_call(&mut invalid, "call_valid", "fs_write", false).is_none());
        assert_eq!(invalid.len(), 1);
    }

    #[test]
    fn invalid_tool_message_redacts_secrets() {
        let call = InvalidToolCall {
            id: "call_bad".to_string(),
            name: "fs_write".to_string(),
            block_index: 0,
            raw_arguments: "{\"content\":\"sk-test0000000000000000\"".to_string(),
            error: "expected value".to_string(),
        };
        let message = invalid_tool_message(&call);
        assert!(
            !message.contains("sk-test0000000000000000"),
            "a malformed secret must not be persisted: {message}"
        );
        assert!(message.contains("REDACTED"));
    }

    #[test]
    fn invalid_tool_message_bounds_the_raw_arguments() {
        let call = InvalidToolCall {
            id: "call_bad".to_string(),
            name: "fs_write".to_string(),
            block_index: 0,
            raw_arguments: "x".repeat(10_000),
            error: "expected value at line 1 column 2".to_string(),
        };
        let message = invalid_tool_message(&call);
        assert!(message.contains("fs_write"));
        assert!(message.contains("expected value"));
        assert!(
            message.chars().count() < 2_500,
            "raw arguments must be truncated: {}",
            message.chars().count()
        );
    }
}
