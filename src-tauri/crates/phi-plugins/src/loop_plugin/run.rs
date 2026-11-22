use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use phi_contracts::ports::{InvalidToolCall, ModelRequest, ToolCall, ToolCapability};
use phi_domain::agent::{AgentRole, AgentStatus, MessageKind, WorkMode};
use phi_domain::events::DomainEvent;
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_domain::view::RunState;
use phi_i18n as i18n;

use super::RunContext;
use super::context::{
    fallback_context, history_char_budget, images_for, inject_memory, last_user_text, role_prompt,
};
use super::support::{env_bool, env_u64, set_status, set_status_progress};
use super::tool_pipeline::{execute_tool, invalid_tool_message, take_invalid_call};

/// Appended to the normal system prompt so every reply can carry a small
/// HTML interface block rendered inside the message flow.
const INLINE_UI_PROMPT: &str = include_str!("../../../../assets/inline-ui-system-prompt.txt");

/// A model that keeps re-emitting unparsable tool arguments gets a few fresh
/// steps to correct itself, then the run fails honestly instead of looping
/// without progress.
const MAX_INVALID_TOOL_RECOVERIES: u32 = 5;

pub(super) enum RunOutcome {
    Succeeded,
    Failed(String),
    Aborted { reason: String, stop: String },
}

pub(super) async fn run_loop(ctx: RunContext) {
    let outcome = run_inner(&ctx).await;
    let agent_status = outcome.agent_status();

    if ctx.role == AgentRole::Lead {
        cancel_descendants(&ctx);
    }

    let (report_message_id, delivered_to) = if ctx.role == AgentRole::Subagent {
        deliver_report(&ctx, &outcome)
    } else {
        (None, None)
    };

    if ctx.role == AgentRole::Subagent {
        let _ = ctx.bus.publish(
            &ctx.session_id,
            "plugin-loop",
            &DomainEvent::AgentReported {
                agent_id: ctx.agent_id.clone(),
                session_id: ctx.session_id.clone(),
                run_id: ctx.run_id.clone(),
                status: agent_status,
                report_message_id: report_message_id.clone().unwrap_or_default(),
                delivered_to: delivered_to.clone(),
            },
        );
    }

    let state = match outcome {
        RunOutcome::Succeeded => {
            let _ = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::RunSucceeded {
                    run_id: ctx.run_id.clone(),
                },
            );
            RunState::Succeeded
        }
        RunOutcome::Failed(error) => {
            let _ = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::RunFailed {
                    run_id: ctx.run_id.clone(),
                    error: error.clone(),
                },
            );
            RunState::Failed
        }
        RunOutcome::Aborted { reason, stop } => {
            let _ = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::RunAborted {
                    run_id: ctx.run_id.clone(),
                    reason: reason.clone(),
                    stop_confirmation: stop.clone(),
                },
            );
            RunState::Aborted
        }
    };

    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::AgentStatusChanged {
            agent_id: ctx.agent_id.clone(),
            session_id: ctx.session_id.clone(),
            status: agent_status,
            note: None,
        },
    );

    {
        let mut status = ctx.status.lock().unwrap();
        status.state = state;
    }

    if ctx.role == AgentRole::Lead {
        let _ = ctx.bus.publish(
            &ctx.session_id,
            "plugin-loop",
            &DomainEvent::TurnClosed {
                turn_id: ctx.turn_id.clone(),
                terminal_run_id: ctx.run_id.clone(),
            },
        );
    }

    {
        let mut running = ctx.running.lock().unwrap();
        running.remove(&ctx.run_id);
    }
}

impl RunOutcome {
    fn agent_status(&self) -> AgentStatus {
        match self {
            RunOutcome::Succeeded => AgentStatus::Succeeded,
            RunOutcome::Failed(_) => AgentStatus::Failed,
            RunOutcome::Aborted { .. } => AgentStatus::Cancelled,
        }
    }
}

fn cancel_descendants(ctx: &RunContext) {
    let running = ctx.running.lock().unwrap();
    let descendants: Vec<CancellationToken> = running
        .values()
        .filter(|control| {
            control.session_id == ctx.session_id
                && control.root_agent_id == ctx.root_agent_id
                && control.parent_run_id.is_some()
        })
        .map(|control| control.cancel.clone())
        .collect();
    drop(running);
    for token in descendants {
        token.cancel();
    }
}

fn deliver_report(ctx: &RunContext, outcome: &RunOutcome) -> (Option<String>, Option<String>) {
    let Some(session) = ctx.services.session() else {
        return (None, None);
    };
    // If the requester already reached a terminal state, do not append a
    // report to a closed turn. The cancellation record still exists.
    if let Some(parent_run_id) = &ctx.parent_run_id {
        match session.find_run(parent_run_id) {
            Some((_, parent)) if parent.is_active() => {}
            _ => return (None, None),
        }
    }
    let report = match outcome {
        RunOutcome::Succeeded => last_assistant_text(&session, ctx),
        RunOutcome::Failed(error) => {
            i18n::t_args("The subagent failed: {error}", &[("error", error.clone())])
        }
        RunOutcome::Aborted { reason, stop } => i18n::t_args(
            "The subagent was stopped: {reason}. {stop}",
            &[("reason", reason.clone()), ("stop", stop.clone())],
        ),
    };
    let requester = session
        .agent(&ctx.agent_id)
        .and_then(|agent| agent.report_to)
        .or_else(|| session.lead_agent(&ctx.session_id).map(|agent| agent.id));
    let Some(requester) = requester else {
        return (None, None);
    };
    let lead_id = session.lead_agent(&ctx.session_id).map(|agent| agent.id);
    let thread_id = if lead_id.as_deref() == Some(requester.as_str()) {
        "main".to_string()
    } else {
        match session.agent(&requester) {
            Some(agent) => agent.thread_id,
            None => "main".to_string(),
        }
    };
    let text = i18n::t_args(
        "Report from subagent {agent} ({status}):\n{report}",
        &[
            ("agent", ctx.agent_id.clone()),
            ("status", outcome.agent_status().as_str().to_string()),
            ("report", report),
        ],
    );
    let message_id = session
        .append_message_in_thread(
            &ctx.session_id,
            Some(&ctx.turn_id),
            &thread_id,
            Some(&ctx.agent_id),
            Some(&requester),
            Some(MessageKind::Report),
            Role::User,
            vec![ContentBlock::text(text)],
        )
        .ok();
    (message_id, Some(requester))
}

fn last_assistant_text(
    session: &Arc<dyn phi_contracts::ports::SessionPort>,
    ctx: &RunContext,
) -> String {
    let messages = session.messages_for_thread(&ctx.session_id, &ctx.thread_id);
    for message in messages.into_iter().rev() {
        if message.role != Role::Assistant {
            continue;
        }
        let text = message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            return text;
        }
    }
    i18n::t("(the subagent produced no text report)")
}

pub(super) async fn run_inner(ctx: &RunContext) -> RunOutcome {
    let Some(session) = ctx.services.session() else {
        return RunOutcome::Failed(i18n::t("Session store is not available"));
    };
    let Some(model) = ctx.services.model() else {
        return RunOutcome::Failed(i18n::t("Model gateway is not available"));
    };

    set_status(ctx, RunState::Running);
    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::RunStarted {
            run_id: ctx.run_id.clone(),
        },
    );

    let tools = {
        let mut tools = ctx.services.tool_definitions();
        if ctx.role == AgentRole::Subagent {
            tools.retain(|tool| {
                !tool.capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ToolCapability::AgentControl | ToolCapability::ExternalEgress
                    )
                })
            });
        }
        if ctx.work_mode == WorkMode::ReadOnly {
            tools.retain(|tool| {
                !tool.capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ToolCapability::WorkspaceWrite | ToolCapability::Shell
                    )
                })
            });
        }
        if ctx
            .services
            .image()
            .map(|image| !image.status().ready)
            .unwrap_or(true)
        {
            tools.retain(|tool| !tool.capabilities.contains(&ToolCapability::ImageGeneration));
        }
        tools
    };
    let max_output_tokens = env_u64("PHI_MAX_OUTPUT_TOKENS", 8192) as u32;
    // The model id was resolved (and pinned) when the run was scheduled. Size
    // history from that model's window, not from whichever provider happens to
    // be active now; otherwise a per-message or subagent model switch
    // over-trims or overflows the real context.
    let requested_model = ctx.resolved_model.clone();
    let context_window = model
        .roster()
        .into_iter()
        .find(|spec| spec.id.eq_ignore_ascii_case(&requested_model))
        .and_then(|spec| spec.context_window)
        .or_else(|| model.status().context_window);
    let context_budget = history_char_budget(context_window, max_output_tokens);
    // Capture the role prompt once: agent statuses change during the run, and
    // a mutating system prefix would invalidate the provider's prompt cache.
    let role_system = role_prompt(&session, ctx);
    // There is no run-level step cap; the loop ends when the model stops
    // calling tools or the user cancels. Truncation is recovered a bounded
    // number of times before the run fails honestly.
    let mut truncation_recoveries = 0_u32;
    // Malformed tool arguments are recoverable per call, but a model that
    // never produces parsable JSON must not get a fresh step forever.
    let mut invalid_tool_recoveries = 0_u32;
    // A lead may not finish while subagents are still active.
    let mut finish_guard = 0_u32;
    // A text-only truncation continues with an ephemeral prompt: the agent
    // must not fabricate a user-authored message in the transcript.
    let mut continuation_prompt: Option<String> = None;
    // Memory is retrieved once per run; re-injecting it every step changes the
    // system prefix mid-run and can invalidate preserved thinking.
    let mut memory_injected = false;
    let mut step_index = 0_u64;

    loop {
        step_index += 1;
        if ctx.cancel.is_cancelled() {
            return aborted(
                ctx,
                &i18n::t("Cancelled by user"),
                &i18n::t("Stopped: the in-flight request was aborted."),
            );
        }

        let history = session.messages_for_thread(&ctx.session_id, &ctx.thread_id);
        let memory_query = last_user_text(&history);
        let mut assembled = match ctx.services.context() {
            Some(assembler) => assembler.assemble(history, &ctx.workspace, context_budget),
            None => fallback_context(history, &ctx.workspace),
        };
        assembled.system = format!("{}\n\n{}", assembled.system, role_system);
        if ctx.role == AgentRole::Lead {
            assembled.system = format!("{}\n\n{}", assembled.system, INLINE_UI_PROMPT);
        }
        if !memory_injected {
            inject_memory(&ctx, memory_query.as_deref(), &mut assembled).await;
            memory_injected = true;
        }
        if let Some(prompt) = continuation_prompt.take() {
            assembled.messages.push(ChatMessage::user_text(prompt));
        }

        let step_id = ctx.ids.new_id("step");
        if let Err(error) = ctx.bus.publish(
            &ctx.session_id,
            "plugin-loop",
            &DomainEvent::StepStarted {
                run_id: ctx.run_id.clone(),
                step_id: step_id.clone(),
                index: step_index,
                phase: "Planning".to_string(),
            },
        ) {
            return RunOutcome::Failed(i18n::t_args(
                "Failed to record the step start: {error}",
                &[("error", error)],
            ));
        }
        set_status_progress(ctx, step_index);

        let mut response = None;
        let mut attempt = 0;
        while attempt < 2 {
            attempt += 1;
            let call_id = ctx.ids.new_id("call");
            let system_chars = assembled.system.chars().count() as u64;
            let tools_chars = tools
                .iter()
                .map(|tool| {
                    tool.description.chars().count() + tool.input_schema.to_string().chars().count()
                })
                .sum::<usize>() as u64;
            let history_chars = serde_json::to_string(&assembled.messages)
                .map(|raw| raw.chars().count() as u64)
                .unwrap_or(0);
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::ModelCallIssued {
                    call_id: call_id.clone(),
                    run_id: ctx.run_id.clone(),
                    step_id: step_id.clone(),
                    model: requested_model.clone(),
                    max_output_tokens,
                    system_chars,
                    tools_chars,
                    history_chars,
                    context_window,
                },
            ) {
                return RunOutcome::Failed(i18n::t_args(
                    "Failed to record the model call: {error}",
                    &[("error", error)],
                ));
            }

            let images = match images_for(ctx, &assembled.messages) {
                Ok(images) => images,
                Err(error) => return RunOutcome::Failed(error),
            };
            let request = ModelRequest {
                model: requested_model.clone(),
                system: assembled.system.clone(),
                messages: assembled.messages.clone(),
                tools: tools.clone(),
                max_output_tokens,
                thinking: ctx.thinking,
                call_id: call_id.clone(),
                run_id: ctx.run_id.clone(),
                stream: env_bool("PHI_STREAM", true),
                images,
            };

            let call_started = Instant::now();
            let result = model.invoke(request, ctx.cancel.clone()).await;
            let duration_ms = call_started.elapsed().as_millis() as u64;

            match result {
                Ok(resp) => {
                    if let Err(publish_error) = ctx.bus.publish(
                        &ctx.session_id,
                        "plugin-loop",
                        &DomainEvent::ModelCallReturned {
                            call_id,
                            run_id: ctx.run_id.clone(),
                            input_tokens: resp.input_tokens,
                            output_tokens: resp.output_tokens,
                            cache_read_tokens: resp.cache_read_tokens,
                            cache_write_tokens: resp.cache_write_tokens,
                            duration_ms,
                            finish_reason: resp.finish_reason.clone(),
                        },
                    ) {
                        return RunOutcome::Failed(i18n::t_args(
                            "Failed to record the model response: {error}",
                            &[("error", publish_error)],
                        ));
                    }
                    set_status_progress(ctx, step_index);
                    response = Some(resp);
                    break;
                }
                Err(error) => {
                    if let Err(publish_error) = ctx.bus.publish(
                        &ctx.session_id,
                        "plugin-loop",
                        &DomainEvent::ModelCallFailed {
                            call_id,
                            run_id: ctx.run_id.clone(),
                            error: error.message.clone(),
                        },
                    ) {
                        return RunOutcome::Failed(i18n::t_args(
                            "Failed to record the model failure: {error}",
                            &[("error", publish_error)],
                        ));
                    }
                    if error.message == "cancelled" || ctx.cancel.is_cancelled() {
                        return aborted(
                            ctx,
                            &i18n::t("Cancelled by user"),
                            &i18n::t(
                                "Stopped: model request was aborted; no tool side effects occurred.",
                            ),
                        );
                    }
                    if error.retryable && attempt == 1 {
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                            _ = ctx.cancel.cancelled() => {
                                return aborted(
                                    ctx,
                                    &i18n::t("Cancelled by user"),
                                    &i18n::t("Stopped: cancelled during retry backoff."),
                                );
                            }
                        }
                        continue;
                    }
                    // Keep what the model had already produced. This branch
                    // is only reached after retries are exhausted or when the
                    // error is not retryable, so a successful retry never
                    // duplicates text.
                    if let Some(partial) = error.partial_text {
                        if !partial.trim().is_empty() {
                            let _ = session.append_message_in_thread(
                                &ctx.session_id,
                                Some(&ctx.turn_id),
                                &ctx.thread_id,
                                Some(&ctx.agent_id),
                                None,
                                Some(MessageKind::Assistant),
                                Role::Assistant,
                                vec![ContentBlock::text(partial)],
                            );
                        }
                    }
                    return RunOutcome::Failed(error.message);
                }
            }
        }

        let Some(mut response) = response else {
            return RunOutcome::Failed(i18n::t("Model call returned no response"));
        };

        // Normalize tool ids so tool_use/tool_result pairs always match. An
        // empty id would otherwise make an invalid call fail to match its
        // parsed entry and execute with `{}`. Align by block index so a valid
        // id-less call cannot consume a malformed call's entry.
        let mut generated_by_index: HashMap<usize, String> = HashMap::new();
        for (index, block) in response.blocks.iter_mut().enumerate() {
            if let ContentBlock::ToolUse { id, .. } = block {
                if id.trim().is_empty() {
                    let generated = ctx.ids.new_id("call");
                    *id = generated.clone();
                    generated_by_index.insert(index, generated);
                }
            }
        }
        let generated_ids: HashSet<String> = generated_by_index.values().cloned().collect();
        for invalid in &mut response.invalid_tool_calls {
            if invalid.id.trim().is_empty() {
                if let Some(generated) = generated_by_index.get(&invalid.block_index) {
                    invalid.id = generated.clone();
                }
            }
        }

        // A thinking block cut off before its signature cannot be replayed.
        response.blocks.retain(|block| match block {
            ContentBlock::Thinking { signature, .. } => !signature.is_empty(),
            ContentBlock::RedactedThinking { data } => !data.is_empty(),
            _ => true,
        });
        if response.blocks.is_empty() {
            response
                .blocks
                .push(ContentBlock::text(i18n::t("(model returned no content)")));
        }

        let tool_calls: Vec<(String, String, Value)> = response
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if let Err(error) = session.append_message_in_thread(
            &ctx.session_id,
            Some(&ctx.turn_id),
            &ctx.thread_id,
            Some(&ctx.agent_id),
            None,
            Some(MessageKind::Assistant),
            Role::Assistant,
            response.blocks.clone(),
        ) {
            return RunOutcome::Failed(i18n::t_args(
                "Failed to save assistant message: {error}",
                &[("error", error)],
            ));
        }

        if matches!(response.finish_reason.as_str(), "length" | "max_tokens") {
            // Truncation is recoverable a bounded number of times. Never
            // execute tool calls from a cut-off response: the arguments may
            // be incomplete.
            const MAX_TRUNCATION_RECOVERIES: u32 = 3;
            if truncation_recoveries >= MAX_TRUNCATION_RECOVERIES {
                // The assistant message is already stored. Keep the protocol
                // history valid before failing by pairing any tool_use block.
                if !tool_calls.is_empty() {
                    let blocks: Vec<ContentBlock> = tool_calls
                        .iter()
                        .map(|(call_id, _, _)| {
                            ContentBlock::tool_result(
                                call_id.clone(),
                                i18n::t(
                                    "Response was truncated before the tool call completed; the tool was not executed.",
                                ),
                                true,
                            )
                        })
                        .collect();
                    let _ = session.append_message_in_thread(
                        &ctx.session_id,
                        Some(&ctx.turn_id),
                        &ctx.thread_id,
                        Some(&ctx.agent_id),
                        None,
                        Some(MessageKind::Tool),
                        Role::User,
                        blocks,
                    );
                }
                return RunOutcome::Failed(i18n::t_args(
                    "The model response was truncated by the output token limit ({max_output_tokens} tokens) three times; the run stopped to avoid an endless loop.",
                    &[("max_output_tokens", max_output_tokens.to_string())],
                ));
            }
            truncation_recoveries += 1;
            if tool_calls.is_empty() {
                // The assistant text is already stored. The continuation
                // instruction is injected only into this run's next request;
                // it is not a message the human wrote.
                continuation_prompt = Some(i18n::t(
                    "The previous response was cut off by the output token limit. Continue exactly where you stopped, without repeating what you already wrote.",
                ));
            } else {
                // Emit paired results for every tool_use block so the stored
                // history remains valid for the provider.
                let blocks: Vec<ContentBlock> = tool_calls
                    .iter()
                    .map(|(call_id, _, _)| {
                        ContentBlock::tool_result(
                            call_id.clone(),
                            i18n::t(
                                "Response was truncated before the tool call completed; the tool was not executed.",
                            ),
                            true,
                        )
                    })
                    .collect();
                if let Err(error) = session.append_message_in_thread(
                    &ctx.session_id,
                    Some(&ctx.turn_id),
                    &ctx.thread_id,
                    Some(&ctx.agent_id),
                    None,
                    Some(MessageKind::Tool),
                    Role::User,
                    blocks,
                ) {
                    return RunOutcome::Failed(i18n::t_args(
                        "Failed to save tool results: {error}",
                        &[("error", error)],
                    ));
                }
            }
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::StepCompleted {
                    run_id: ctx.run_id.clone(),
                    step_id,
                },
            ) {
                return RunOutcome::Failed(i18n::t_args(
                    "Failed to record the step completion: {error}",
                    &[("error", error)],
                ));
            }
            // Recover: let the model continue from the truncated state.
            continue;
        }

        if tool_calls.is_empty() {
            if let Err(error) = ctx.bus.publish(
                &ctx.session_id,
                "plugin-loop",
                &DomainEvent::StepCompleted {
                    run_id: ctx.run_id.clone(),
                    step_id,
                },
            ) {
                return RunOutcome::Failed(i18n::t_args(
                    "Failed to record the step completion: {error}",
                    &[("error", error)],
                ));
            }
            if ctx.role == AgentRole::Lead {
                let active = active_subagents(&session, ctx);
                if !active.is_empty() {
                    finish_guard += 1;
                    if finish_guard > 3 {
                        return RunOutcome::Failed(i18n::t(
                            "The lead tried to finish while subagents were still active; the run stopped to avoid orphaned work.",
                        ));
                    }
                    continuation_prompt = Some(i18n::t_args(
                        "You still have active subagents: {agents}. Call wait_for_agents to collect their reports, or cancel_agent to stop them, before finishing.",
                        &[("agents", active.join(", "))],
                    ));
                    continue;
                }
            }
            return RunOutcome::Succeeded;
        }

        let mut result_blocks = Vec::new();
        let mut unknown_side_effect = false;
        let mut cancelled = false;
        let mut invalid_calls: Vec<InvalidToolCall> = response.invalid_tool_calls.clone();
        for (call_id, tool_id, input) in tool_calls {
            // Once cancellation is observed, never start another tool. Still
            // emit a paired result so the history stays valid for the provider.
            if cancelled || ctx.cancel.is_cancelled() {
                cancelled = true;
                result_blocks.push(ContentBlock::tool_result(
                    call_id,
                    i18n::t("Run cancelled; action was not executed."),
                    true,
                ));
                continue;
            }
            // The model produced arguments that were not valid JSON. Do not
            // execute the tool; answer with a paired error so the protocol
            // history stays valid and the model can re-issue the call.
            if let Some(invalid) = take_invalid_call(
                &mut invalid_calls,
                &call_id,
                &tool_id,
                generated_ids.contains(&call_id),
            ) {
                let error = invalid_tool_message(&invalid);
                invalid_tool_recoveries += 1;
                let _ = ctx.bus.publish(
                    &ctx.session_id,
                    "plugin-loop",
                    &DomainEvent::ToolInvokeFailed {
                        invocation_id: call_id.clone(),
                        run_id: ctx.run_id.clone(),
                        error: error.clone(),
                    },
                );
                result_blocks.push(ContentBlock::tool_result(call_id, error, true));
                continue;
            }
            let outcome = execute_tool(
                ctx,
                &step_id,
                ToolCall {
                    id: call_id,
                    tool_id,
                    input,
                },
            )
            .await;
            if outcome.cancelled {
                cancelled = true;
            }
            if outcome.unknown_side_effect {
                unknown_side_effect = true;
            }
            result_blocks.push(outcome.block);
        }

        if let Err(error) = session.append_message_in_thread(
            &ctx.session_id,
            Some(&ctx.turn_id),
            &ctx.thread_id,
            Some(&ctx.agent_id),
            None,
            Some(MessageKind::Tool),
            Role::User,
            result_blocks,
        ) {
            return RunOutcome::Failed(i18n::t_args(
                "Failed to save tool results: {error}",
                &[("error", error)],
            ));
        }

        if let Err(error) = ctx.bus.publish(
            &ctx.session_id,
            "plugin-loop",
            &DomainEvent::StepCompleted {
                run_id: ctx.run_id.clone(),
                step_id,
            },
        ) {
            return RunOutcome::Failed(i18n::t_args(
                "Failed to record the step completion: {error}",
                &[("error", error)],
            ));
        }

        if cancelled || ctx.cancel.is_cancelled() {
            let stop = if unknown_side_effect {
                i18n::t(
                    "Stopped: the command was terminated and may have produced partial side effects. Inspect and verify the workspace (no automatic retry).",
                )
            } else {
                i18n::t("Stopped: the in-flight tool call was aborted.")
            };
            return RunOutcome::Aborted {
                reason: i18n::t("Cancelled by user"),
                stop,
            };
        }

        if invalid_tool_recoveries >= MAX_INVALID_TOOL_RECOVERIES {
            return RunOutcome::Failed(i18n::t(
                "The model repeatedly sent tool arguments that were not valid JSON; the run stopped without executing them.",
            ));
        }
    }
}

fn active_subagents(
    session: &Arc<dyn phi_contracts::ports::SessionPort>,
    ctx: &RunContext,
) -> Vec<String> {
    session
        .agents(&ctx.session_id)
        .into_iter()
        .filter(|agent| {
            agent.role == AgentRole::Subagent
                && agent.status.is_active()
                && agent.root_agent_id == ctx.root_agent_id
        })
        .map(|agent| agent.id)
        .collect()
}

fn aborted(_ctx: &RunContext, reason: &str, stop: &str) -> RunOutcome {
    RunOutcome::Aborted {
        reason: reason.to_string(),
        stop: stop.to_string(),
    }
}
