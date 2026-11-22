//! Lead ↔ subagent delegation tools.
//!
//! The lead agent creates persistent subagent instances, each with its own
//! message thread and child run. Subagents cannot delegate further; they can
//! send directed messages to sibling agents. The plugin only coordinates —
//! every subagent tool call still passes policy, approval and sandbox checks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use phi_contracts::SLOT_TOOL_PROVIDER;
use phi_contracts::ports::{
    SideEffect, StartRunRequest, ThinkingEffort, ToolCall, ToolContext, ToolDefinition, ToolOutput,
    ToolPort,
};
use phi_contracts::services::Services;
use phi_domain::agent::{AgentRole, AgentStatus, AgentView, MessageKind, WorkMode};
use phi_domain::events::DomainEvent;
use phi_domain::message::{ContentBlock, Role};
use phi_i18n as i18n;
use phi_kernel::event_bus::EventBus;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::ports::{IdPort, LogPort};
use phi_kernel::registry::Contribution;

const MAX_ACTIVE_SUBAGENTS: usize = 4;
const DEFAULT_WAIT_SECS: u64 = 180;
const MAX_WAIT_SECS: u64 = 600;

pub struct SubagentToolsPlugin {
    services: Arc<Services>,
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    log: Arc<dyn LogPort>,
}

impl SubagentToolsPlugin {
    pub fn new(
        services: Arc<Services>,
        bus: Arc<EventBus>,
        ids: Arc<dyn IdPort>,
        log: Arc<dyn LogPort>,
    ) -> Self {
        Self {
            services,
            bus,
            ids,
            log,
        }
    }

    fn bad(message: String) -> Result<ToolOutput, String> {
        Ok(ToolOutput {
            content: message,
            is_error: true,
        })
    }

    fn required_str<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                i18n::t_args(
                    "Missing required argument: {key}",
                    &[("key", key.to_string())],
                )
            })
    }

    async fn delegate(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, String> {
        if ctx.role != AgentRole::Lead {
            return Self::bad(i18n::t("Only the lead agent can delegate to a subagent"));
        }
        let Some(session) = self.services.session() else {
            return Self::bad(i18n::t("Session store is not available"));
        };
        let Some(model) = self.services.model() else {
            return Self::bad(i18n::t("Model gateway is not available"));
        };
        let Some(runtime) = self.services.runtime() else {
            return Self::bad(i18n::t("Runtime is not available"));
        };

        let model_id = Self::required_str(&call.input, "model")?;
        let brief = call.input.get("brief").unwrap_or(&call.input);
        let objective = brief
            .get("objective")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| i18n::t("Missing required argument: brief.objective"))?;
        let constraints = brief
            .get("constraints")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let acceptance = brief
            .get("acceptance")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let context = brief
            .get("context")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let thinking = match call.input.get("thinking").and_then(Value::as_str) {
            Some(value) if !value.trim().is_empty() => match ThinkingEffort::parse(value) {
                Some(thinking) => Some(thinking),
                None => {
                    return Self::bad(i18n::t_args(
                        "Unknown thinking effort {effort}; expected off, low, medium or high",
                        &[("effort", value.to_string())],
                    ));
                }
            },
            _ => None,
        };
        let work_mode = match call.input.get("work_mode").and_then(Value::as_str) {
            Some(value) if !value.trim().is_empty() => match WorkMode::parse(value) {
                Some(mode) => mode,
                None => {
                    return Self::bad(i18n::t_args(
                        "Unknown work mode {mode}; expected shared or read_only",
                        &[("mode", value.to_string())],
                    ));
                }
            },
            _ => WorkMode::ReadOnly,
        };
        let wait = call
            .input
            .get("wait")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let agents = session.agents(&ctx.session_id);
        let active = agents
            .iter()
            .filter(|agent| agent.role == AgentRole::Subagent && agent.status.is_active())
            .count();
        if active >= MAX_ACTIVE_SUBAGENTS {
            return Self::bad(i18n::t_args(
                "Too many active subagents ({count}); wait for one to finish or cancel it first",
                &[("count", active.to_string())],
            ));
        }

        let roster = model.roster();
        let resolved_model = model.resolve_model(model_id);
        let Some(spec) = roster
            .iter()
            .find(|candidate| candidate.id.eq_ignore_ascii_case(&resolved_model))
        else {
            let available = roster
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Self::bad(i18n::t_args(
                "Unknown subagent model: {model}. Available models: {available}",
                &[("model", resolved_model), ("available", available)],
            ));
        };
        if spec.scale > ctx.lead_scale {
            return Self::bad(i18n::t_args(
                "Subagent model {model} (scale {scale}) is more capable than the lead model (scale {lead}); choose a smaller or equal model",
                &[
                    ("model", resolved_model),
                    ("scale", spec.scale.to_string()),
                    ("lead", ctx.lead_scale.to_string()),
                ],
            ));
        }
        if work_mode == WorkMode::Shared {
            if let Some(holder) = session.write_lease_holder(&ctx.session_id) {
                return Self::bad(i18n::t_args(
                    "Another agent holds the workspace write lease ({agent}); wait for it to finish or use read_only",
                    &[("agent", holder)],
                ));
            }
        }

        let agent_id = self.ids.new_id("agent");
        let thread_id = format!("agent:{agent_id}");
        if let Err(error) = self.bus.publish(
            &ctx.session_id,
            "plugin-subagent",
            &DomainEvent::AgentCreated {
                agent_id: agent_id.clone(),
                session_id: ctx.session_id.clone(),
                turn_id: ctx.turn_id.clone(),
                role: AgentRole::Subagent,
                parent_agent_id: Some(ctx.agent_id.clone()),
                root_agent_id: ctx.root_agent_id.clone(),
                model: resolved_model.clone(),
                thinking: thinking.map(|thinking| thinking.as_str().to_string()),
                thread_id: thread_id.clone(),
                work_mode,
                report_to: Some(ctx.agent_id.clone()),
                spawned_by_run_id: ctx.run_id.clone(),
            },
        ) {
            return Self::bad(i18n::t_args(
                "Failed to record the subagent: {error}",
                &[("error", error)],
            ));
        }

        let brief_text = format!(
            "Brief from the lead agent.\nObjective:\n{objective}\n\nConstraints:\n{}\n\nAcceptance criteria:\n{}\n\nContext:\n{}\n\nYou are a subagent: work only on this brief, do not address the user, and reply with a concise report (what you did, files changed, verification results, open questions). This final text is delivered to the lead.",
            if constraints.is_empty() {
                "(none)"
            } else {
                constraints
            },
            if acceptance.is_empty() {
                "(none)"
            } else {
                acceptance
            },
            if context.is_empty() {
                "(none)"
            } else {
                context
            },
        );
        if let Err(error) = session.append_message_in_thread(
            &ctx.session_id,
            Some(&ctx.turn_id),
            &thread_id,
            Some(&ctx.agent_id),
            Some(&agent_id),
            Some(MessageKind::Brief),
            Role::User,
            vec![ContentBlock::text(brief_text)],
        ) {
            let _ = self.bus.publish(
                &ctx.session_id,
                "plugin-subagent",
                &DomainEvent::AgentStatusChanged {
                    agent_id: agent_id.clone(),
                    session_id: ctx.session_id.clone(),
                    status: AgentStatus::Failed,
                    note: Some(error.clone()),
                },
            );
            return Self::bad(i18n::t_args(
                "Failed to save the subagent brief: {error}",
                &[("error", error)],
            ));
        }

        let request = StartRunRequest {
            session_id: ctx.session_id.clone(),
            turn_id: ctx.turn_id.clone(),
            workspace: ctx.workspace.clone(),
            model: Some(resolved_model.clone()),
            thinking,
            parent_run_id: Some(ctx.run_id.clone()),
            agent_id: Some(agent_id.clone()),
            thread_id: Some(thread_id.clone()),
            root_agent_id: Some(ctx.root_agent_id.clone()),
            role: AgentRole::Subagent,
            report_to: Some(ctx.agent_id.clone()),
            work_mode,
            lead_scale: Some(ctx.lead_scale),
        };
        let child_run_id = match runtime.start_run(request) {
            Ok(run_id) => run_id,
            Err(error) => {
                let _ = self.bus.publish(
                    &ctx.session_id,
                    "plugin-subagent",
                    &DomainEvent::AgentStatusChanged {
                        agent_id: agent_id.clone(),
                        session_id: ctx.session_id.clone(),
                        status: AgentStatus::Failed,
                        note: Some(error.clone()),
                    },
                );
                return Self::bad(i18n::t_args(
                    "Failed to start the subagent: {error}",
                    &[("error", error)],
                ));
            }
        };
        self.log.info(&format!(
            "subagent {agent_id} started by {}: run {child_run_id}",
            ctx.agent_id
        ));

        if wait {
            return self
                .wait_for(&[agent_id], true, DEFAULT_WAIT_SECS, cancel)
                .await;
        }
        Ok(ToolOutput {
            content: json!({
                "agentId": agent_id,
                "threadId": thread_id,
                "runId": child_run_id,
                "status": "running"
            })
            .to_string(),
            is_error: false,
        })
    }

    async fn wait_for_agents(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, String> {
        if ctx.role != AgentRole::Lead {
            return Self::bad(i18n::t("Only the lead agent can wait for subagents"));
        }
        let ids: Vec<String> = call
            .input
            .get("agent_ids")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Self::bad(i18n::t(
                "The agent_ids argument must contain at least one agent id",
            ));
        }
        let Some(session) = self.services.session() else {
            return Self::bad(i18n::t("Session store is not available"));
        };
        for id in &ids {
            match session.agent(id) {
                Some(agent)
                    if agent.session_id == ctx.session_id
                        && agent.root_agent_id == ctx.root_agent_id => {}
                Some(_) => {
                    return Self::bad(i18n::t_args(
                        "Agent {agent} is not part of this delegation family",
                        &[("agent", id.clone())],
                    ));
                }
                None => {
                    return Self::bad(i18n::t_args(
                        "Unknown agent: {agent}",
                        &[("agent", id.clone())],
                    ));
                }
            }
        }
        let mode_all = call
            .input
            .get("mode")
            .and_then(Value::as_str)
            .map(|mode| !mode.eq_ignore_ascii_case("any"))
            .unwrap_or(true);
        let timeout_secs = call
            .input
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_WAIT_SECS)
            .clamp(1, MAX_WAIT_SECS);
        self.wait_for(&ids, mode_all, timeout_secs, cancel).await
    }

    async fn wait_for(
        &self,
        ids: &[String],
        mode_all: bool,
        timeout_secs: u64,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, String> {
        let Some(session) = self.services.session() else {
            return Self::bad(i18n::t("Session store is not available"));
        };
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            let views: Vec<Option<AgentView>> = ids.iter().map(|id| session.agent(id)).collect();
            let finished: Vec<bool> = views
                .iter()
                .map(|view| match view {
                    Some(agent) => agent.status.is_terminal(),
                    None => true,
                })
                .collect();
            let done = if mode_all {
                finished.iter().all(|done| *done)
            } else {
                finished.iter().any(|done| *done)
            };
            if done {
                return Ok(ToolOutput {
                    content: json!({ "timedOut": false, "agents": agent_statuses(ids, &views) })
                        .to_string(),
                    is_error: false,
                });
            }
            if Instant::now() >= deadline {
                return Ok(ToolOutput {
                    content: json!({ "timedOut": true, "agents": agent_statuses(ids, &views) })
                        .to_string(),
                    is_error: false,
                });
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                _ = cancel.cancelled() => return Err("cancelled".to_string()),
            }
        }
    }

    fn send_message(&self, call: &ToolCall, ctx: &ToolContext) -> Result<ToolOutput, String> {
        let Some(session) = self.services.session() else {
            return Self::bad(i18n::t("Session store is not available"));
        };
        let to_agent_id = Self::required_str(&call.input, "to_agent_id")?;
        let message = Self::required_str(&call.input, "message")?;
        let mode = call
            .input
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("steer")
            .trim()
            .to_ascii_lowercase();
        if mode != "steer" && mode != "queue" {
            return Self::bad(i18n::t_args(
                "Unknown message mode {mode}; expected steer or queue",
                &[("mode", mode)],
            ));
        }
        let Some(sender) = session.agent(&ctx.agent_id) else {
            return Self::bad(i18n::t("The sending agent is not known"));
        };
        let Some(recipient) = session.agent(to_agent_id) else {
            return Self::bad(i18n::t_args(
                "Unknown agent: {agent}",
                &[("agent", to_agent_id.to_string())],
            ));
        };
        if recipient.id == sender.id {
            return Self::bad(i18n::t("An agent cannot send a message to itself"));
        }
        if recipient.session_id != sender.session_id
            || recipient.root_agent_id != sender.root_agent_id
        {
            return Self::bad(i18n::t(
                "The recipient is not part of this delegation family",
            ));
        }
        let thread_id = recipient.thread_id.clone();
        if let Err(error) = session.append_message_in_thread(
            &ctx.session_id,
            Some(&ctx.turn_id),
            &thread_id,
            Some(&sender.id),
            Some(&recipient.id),
            Some(MessageKind::Message),
            Role::User,
            vec![ContentBlock::text(format!(
                "Message from {sender_role} {sender_id}:\n{message}",
                sender_role = sender.role.as_str(),
                sender_id = sender.id
            ))],
        ) {
            return Self::bad(i18n::t_args(
                "Failed to deliver the message: {error}",
                &[("error", error)],
            ));
        }

        let running = session.active_run_for_agent(&recipient.id).is_some();
        if running {
            return Ok(ToolOutput {
                content: json!({
                    "agentId": recipient.id,
                    "status": "running",
                    "mode": mode
                })
                .to_string(),
                is_error: false,
            });
        }
        if recipient.role == AgentRole::Lead {
            return Self::bad(i18n::t(
                "The lead is not running; start a new turn to talk to it",
            ));
        }
        let Some(runtime) = self.services.runtime() else {
            return Self::bad(i18n::t("Runtime is not available"));
        };
        if session.active_root_run(&recipient.session_id).is_none() {
            return Self::bad(i18n::t(
                "The turn is already closed; start a new turn to wake this subagent",
            ));
        }
        let thinking = recipient
            .thinking
            .as_deref()
            .and_then(ThinkingEffort::parse);
        let request = StartRunRequest {
            session_id: recipient.session_id.clone(),
            turn_id: recipient.turn_id.clone(),
            workspace: ctx.workspace.clone(),
            model: Some(recipient.model.clone()),
            thinking,
            parent_run_id: Some(recipient.spawned_by_run_id.clone()),
            agent_id: Some(recipient.id.clone()),
            thread_id: Some(recipient.thread_id.clone()),
            root_agent_id: Some(recipient.root_agent_id.clone()),
            role: AgentRole::Subagent,
            report_to: recipient.report_to.clone(),
            work_mode: recipient.work_mode,
            lead_scale: None,
        };
        match runtime.start_run(request) {
            Ok(run_id) => Ok(ToolOutput {
                content: json!({
                    "agentId": recipient.id,
                    "runId": run_id,
                    "status": "running",
                    "mode": mode
                })
                .to_string(),
                is_error: false,
            }),
            Err(error) => Self::bad(i18n::t_args(
                "Failed to wake the subagent: {error}",
                &[("error", error)],
            )),
        }
    }

    fn cancel_agent(&self, call: &ToolCall, ctx: &ToolContext) -> Result<ToolOutput, String> {
        if ctx.role != AgentRole::Lead {
            return Self::bad(i18n::t("Only the lead agent can cancel a subagent"));
        }
        let Some(session) = self.services.session() else {
            return Self::bad(i18n::t("Session store is not available"));
        };
        let agent_id = Self::required_str(&call.input, "agent_id")?;
        let Some(agent) = session.agent(agent_id) else {
            return Self::bad(i18n::t_args(
                "Unknown agent: {agent}",
                &[("agent", agent_id.to_string())],
            ));
        };
        if agent.role != AgentRole::Subagent {
            return Self::bad(i18n::t("Only subagents can be cancelled"));
        }
        if agent.root_agent_id != ctx.root_agent_id {
            return Self::bad(i18n::t(
                "The subagent is not part of this delegation family",
            ));
        }
        let reason = call
            .input
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("cancelled by the lead");
        let cancelled = self
            .services
            .runtime()
            .map(|runtime| {
                session
                    .active_run_for_agent(&agent.id)
                    .map(|run_id| runtime.cancel(&run_id, reason))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !cancelled {
            let _ = self.bus.publish(
                &ctx.session_id,
                "plugin-subagent",
                &DomainEvent::AgentStatusChanged {
                    agent_id: agent.id.clone(),
                    session_id: agent.session_id.clone(),
                    status: AgentStatus::Cancelled,
                    note: Some(reason.to_string()),
                },
            );
        }
        Ok(ToolOutput {
            content: json!({ "agentId": agent.id, "cancelled": true }).to_string(),
            is_error: false,
        })
    }
}

fn agent_statuses(ids: &[String], views: &[Option<AgentView>]) -> Vec<Value> {
    ids.iter()
        .zip(views.iter())
        .map(|(id, view)| match view {
            Some(agent) => json!({
                "agentId": agent.id,
                "status": agent.status.as_str(),
                "reportMessageId": agent.report_message_id,
                "steps": agent.steps,
                "inputTokens": agent.input_tokens,
                "outputTokens": agent.output_tokens,
            }),
            None => json!({ "agentId": id, "status": "missing" }),
        })
        .collect()
}

fn tool(
    id: &str,
    description: &str,
    properties: Value,
    required: Vec<&str>,
    side_effect: SideEffect,
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
    }
}

#[async_trait]
impl ToolPort for SubagentToolsPlugin {
    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![
            tool(
                "delegate",
                "Create a subagent for a separable piece of work. The subagent runs in its own message thread; you receive a report, not its full trajectory. Choose a model whose scale is no larger than yours. Subagents cannot delegate further. Use wait=true to block for the first report, or wait_for_agents to collect several.",
                json!({
                    "model": { "type": "string", "description": "Model id from the available roster" },
                    "thinking": { "type": "string", "enum": ["off", "low", "medium", "high"] },
                    "work_mode": { "type": "string", "enum": ["read_only", "shared"], "description": "read_only (default) hides write tools; shared takes the workspace write lease" },
                    "wait": { "type": "boolean", "description": "Block until the subagent's first report" },
                    "brief": {
                        "type": "object",
                        "properties": {
                            "objective": { "type": "string" },
                            "constraints": { "type": "string" },
                            "acceptance": { "type": "string" },
                            "context": { "type": "string" }
                        },
                        "required": ["objective"]
                    }
                }),
                vec!["model", "brief"],
                SideEffect::None,
            ),
            tool(
                "wait_for_agents",
                "Wait for subagent reports or status changes. Returns the current status of each agent and whether the wait timed out. Reports are delivered to your thread as messages.",
                json!({
                    "agent_ids": { "type": "array", "items": { "type": "string" } },
                    "mode": { "type": "string", "enum": ["all", "any"], "description": "all (default) or any" },
                    "timeout_secs": { "type": "integer", "description": "Default 180, max 600" }
                }),
                vec!["agent_ids"],
                SideEffect::None,
            ),
            tool(
                "send_message",
                "Send a directed message to another agent in this delegation family. There is no broadcast. The recipient sees it in its own thread; if it is idle and the turn is still open, it is woken for a new run.",
                json!({
                    "to_agent_id": { "type": "string" },
                    "message": { "type": "string" },
                    "mode": { "type": "string", "enum": ["steer", "queue"], "description": "steer (default) or queue" }
                }),
                vec!["to_agent_id", "message"],
                SideEffect::None,
            ),
            tool(
                "cancel_agent",
                "Cancel a subagent's active run and release its workspace write lease. Only the lead agent may cancel.",
                json!({
                    "agent_id": { "type": "string" },
                    "reason": { "type": "string" }
                }),
                vec!["agent_id"],
                SideEffect::None,
            ),
        ]
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, String> {
        match call.tool_id.as_str() {
            "delegate" => self.delegate(call, ctx, cancel).await,
            "wait_for_agents" => self.wait_for_agents(call, ctx, cancel).await,
            "send_message" => self.send_message(call, ctx),
            "cancel_agent" => self.cancel_agent(call, ctx),
            other => Self::bad(i18n::t_args(
                "Unknown agent tool: {tool}",
                &[("tool", other.to_string())],
            )),
        }
    }
}

impl Plugin for SubagentToolsPlugin {
    fn id(&self) -> &'static str {
        "plugin-subagent"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_TOOL_PROVIDER.to_string(),
            id: "subagent".to_string(),
        }])
    }
}
