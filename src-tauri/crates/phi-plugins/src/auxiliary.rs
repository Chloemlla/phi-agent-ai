//! Auxiliary text roles: session titles, memory maintenance and summaries.
//!
//! These are service calls, not agents: no Turn, Run or Step is created. Each
//! role is bound to a provider/model in `auxiliary.json` and requires an
//! explicit egress grant.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::json_store::JsonStore;
use phi_contracts::SLOT_AUXILIARY_INVOKE;
use phi_contracts::auxiliary::{
    AUX_MEMORY_MAINTENANCE, AUX_ROLES, AUX_SESSION_TITLE, AUX_SUMMARIZER, AuxiliaryPort,
    AuxiliaryRoleInput, AuxiliaryRoleView, MaintenanceProposalView, MaintenanceReport,
};
use phi_contracts::memory::{MemoryAddRequest, MemoryListFilter, Principal};
use phi_contracts::ports::{ModelRequest, ThinkingEffort};
use phi_contracts::services::Services;
use phi_domain::memory::{MemoryRecord, MemoryScope, MemoryState};
use phi_domain::message::{ChatMessage, ContentBlock};
use phi_i18n as i18n;
use phi_kernel::event_bus::EventBus;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::ports::{ClockPort, IdPort, LogPort};
use phi_kernel::registry::Contribution;

const MAINTENANCE_FACT_THRESHOLD: u64 = 20;
const MAX_MAINTENANCE_FACTS: usize = 200;
const MAX_MAINTENANCE_CHARS: usize = 60_000;
const MAX_FACT_CHARS: usize = 2_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RoleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thinking: Option<String>,
    egress_granted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AuxiliaryConfig {
    roles: BTreeMap<String, RoleConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MaintenanceState {
    last_fact_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_run_at: Option<String>,
}

pub struct AuxiliaryPlugin {
    services: Arc<Services>,
    config: JsonStore<AuxiliaryConfig>,
    maintenance: JsonStore<MaintenanceState>,
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    clock: Arc<dyn ClockPort>,
    log: Arc<dyn LogPort>,
}

impl AuxiliaryPlugin {
    pub fn new(
        services: Arc<Services>,
        config_path: std::path::PathBuf,
        maintenance_path: std::path::PathBuf,
        bus: Arc<EventBus>,
        ids: Arc<dyn IdPort>,
        clock: Arc<dyn ClockPort>,
        log: Arc<dyn LogPort>,
    ) -> Self {
        Self {
            services,
            config: JsonStore::load(config_path),
            maintenance: JsonStore::load(maintenance_path),
            bus,
            ids,
            clock,
            log,
        }
    }

    fn role(&self, role: &str) -> Option<RoleConfig> {
        self.config
            .snapshot()
            .roles
            .get(role)
            .cloned()
            .filter(|config| !config.model.trim().is_empty())
    }

    fn role_view(&self, role: &str) -> AuxiliaryRoleView {
        let config = self.role(role);
        let model_available = self.services.model().is_some();
        let (configured, egress, provider_id, model, thinking) = match &config {
            Some(config) => (
                true,
                config.egress_granted,
                config.provider_id.clone(),
                config.model.clone(),
                config.thinking.clone(),
            ),
            None => (false, false, None, String::new(), None),
        };
        let ready = configured && egress && model_available;
        let reason = if !configured {
            Some(i18n::t("This auxiliary role is not configured"))
        } else if !egress {
            Some(i18n::t("Auxiliary egress is not granted for this role"))
        } else if !model_available {
            Some(i18n::t("Model gateway is not available"))
        } else {
            None
        };
        AuxiliaryRoleView {
            role: role.to_string(),
            provider_id,
            model,
            thinking,
            egress_granted: egress,
            configured,
            ready,
            reason,
        }
    }

    async fn call_role(
        &self,
        role: &str,
        purpose: &str,
        system: &str,
        user: &str,
        max_tokens: u32,
        session_id: Option<&str>,
        cancel: CancellationToken,
    ) -> Result<String, String> {
        let config = self.role(role).ok_or_else(|| {
            i18n::t_args(
                "Auxiliary role is not configured: {role}",
                &[("role", role.to_string())],
            )
        })?;
        if !config.egress_granted {
            return Err(i18n::t_args(
                "Auxiliary egress is not granted for {role}",
                &[("role", role.to_string())],
            ));
        }
        let model = self
            .services
            .model()
            .ok_or_else(|| i18n::t("Model gateway is not available"))?;
        let resolved = model.resolve_model_on(config.provider_id.as_deref(), &config.model);
        let call_id = self.ids.new_id("aux");
        let aggregate = session_id.unwrap_or("node");
        let _ = self.bus.publish(
            aggregate,
            "plugin-auxiliary",
            &phi_domain::events::DomainEvent::AuxiliaryCallIssued {
                call_id: call_id.clone(),
                role: role.to_string(),
                provider_id: config.provider_id.clone(),
                model: resolved.clone(),
                purpose: purpose.to_string(),
                session_id: session_id.map(str::to_string),
            },
        );
        let thinking = config.thinking.as_deref().and_then(ThinkingEffort::parse);
        let request = ModelRequest {
            model: resolved,
            system: system.to_string(),
            messages: vec![ChatMessage::user_text(user)],
            tools: Vec::new(),
            max_output_tokens: max_tokens,
            thinking,
            call_id: call_id.clone(),
            run_id: String::new(),
            stream: false,
        };
        let call = model.invoke_on(config.provider_id.as_deref(), request, cancel.clone());
        let result = match tokio::time::timeout(Duration::from_secs(45), call).await {
            Ok(result) => result,
            Err(_) => {
                let message = i18n::t_args(
                    "Auxiliary call for {role} timed out",
                    &[("role", role.to_string())],
                );
                let _ = self.bus.publish(
                    aggregate,
                    "plugin-auxiliary",
                    &phi_domain::events::DomainEvent::AuxiliaryCallFailed {
                        call_id,
                        role: role.to_string(),
                        error: message.clone(),
                    },
                );
                return Err(message);
            }
        };
        match result {
            Ok(response) => {
                let text = response
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = self.bus.publish(
                    aggregate,
                    "plugin-auxiliary",
                    &phi_domain::events::DomainEvent::AuxiliaryCallReturned {
                        call_id,
                        role: role.to_string(),
                        input_tokens: response.input_tokens,
                        output_tokens: response.output_tokens,
                    },
                );
                Ok(text)
            }
            Err(error) => {
                let message = error.message;
                let _ = self.bus.publish(
                    aggregate,
                    "plugin-auxiliary",
                    &phi_domain::events::DomainEvent::AuxiliaryCallFailed {
                        call_id,
                        role: role.to_string(),
                        error: message.clone(),
                    },
                );
                Err(message)
            }
        }
    }

    fn principal(&self, project_id: Option<&str>) -> Option<Principal> {
        let session = self.services.session()?;
        Some(Principal {
            user_id: session.node_id(),
            project_id: project_id.map(str::to_string),
            agent_id: Some("maintenance".to_string()),
            run_id: None,
        })
    }

    fn active_facts(
        &self,
        project_id: Option<&str>,
    ) -> Result<(Principal, Vec<MemoryRecord>, u64), String> {
        let memory = self
            .services
            .memory()
            .ok_or_else(|| i18n::t("Memory store is not available"))?;
        let principal = self
            .principal(project_id)
            .ok_or_else(|| i18n::t("Session store is not available"))?;
        let stats = memory.stats(&principal)?;
        let mut records = memory.list(
            &principal,
            &MemoryListFilter {
                user_id: principal.user_id.clone(),
                text: None,
                scopes: Vec::new(),
                states: vec![MemoryState::Active],
                project_id: project_id.map(str::to_string),
                run_id: None,
                limit: MAX_MAINTENANCE_FACTS,
                offset: 0,
            },
        )?;
        // User-scope facts are not project-bound; include them explicitly.
        let user_facts = memory.list(
            &principal,
            &MemoryListFilter {
                user_id: principal.user_id.clone(),
                text: None,
                scopes: vec![MemoryScope::User],
                states: vec![MemoryState::Active],
                project_id: None,
                run_id: None,
                limit: MAX_MAINTENANCE_FACTS,
                offset: 0,
            },
        )?;
        for record in user_facts {
            if !records.iter().any(|existing| existing.id == record.id) {
                records.push(record);
            }
        }
        Ok((principal, records, stats.active))
    }
}

#[async_trait]
impl AuxiliaryPort for AuxiliaryPlugin {
    fn roles(&self) -> Vec<AuxiliaryRoleView> {
        AUX_ROLES.iter().map(|role| self.role_view(role)).collect()
    }

    fn set_role(&self, role: &str, input: AuxiliaryRoleInput) -> Result<AuxiliaryRoleView, String> {
        let role = role.trim();
        if !AUX_ROLES.contains(&role) {
            return Err(i18n::t_args(
                "Unknown auxiliary role {role}; expected one of {roles}",
                &[("role", role.to_string()), ("roles", AUX_ROLES.join(", "))],
            ));
        }
        let model = input.model.trim();
        if model.is_empty() {
            return Err(i18n::t("Auxiliary model is empty"));
        }
        let thinking = match input.thinking.as_deref() {
            Some(value) if !value.trim().is_empty() => {
                Some(ThinkingEffort::parse(value).ok_or_else(|| {
                    i18n::t_args(
                        "Unknown thinking effort {effort}; expected off, low, medium, high, xhigh or max",
                        &[("effort", value.to_string())],
                    )
                })?)
                .map(|effort| effort.as_str().to_string())
            }
            _ => None,
        };
        let provider_id = input
            .provider_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.config.update(|config| {
            config.roles.insert(
                role.to_string(),
                RoleConfig {
                    provider_id,
                    model: model.to_string(),
                    thinking,
                    egress_granted: input.egress_granted,
                },
            );
        })?;
        Ok(self.role_view(role))
    }

    fn clear_role(&self, role: &str) -> Result<bool, String> {
        let removed = self
            .config
            .update(|config| config.roles.remove(role).is_some())?;
        Ok(removed)
    }

    fn title_ready(&self) -> bool {
        self.role_view(AUX_SESSION_TITLE).ready
    }

    fn maintenance_ready(&self) -> bool {
        self.role_view(AUX_MEMORY_MAINTENANCE).ready
    }

    fn summarizer_ready(&self) -> bool {
        self.role_view(AUX_SUMMARIZER).ready
    }

    async fn suggest_title(
        &self,
        session_id: &str,
        cancel: CancellationToken,
    ) -> Result<Option<String>, String> {
        if !self.title_ready() {
            return Ok(None);
        }
        let Some(session) = self.services.session() else {
            return Ok(None);
        };
        let Some(view) = session.get_session(session_id) else {
            return Ok(None);
        };
        if view.title != i18n::t("New session") {
            return Ok(None);
        }
        let mut user = String::new();
        let mut assistant = String::new();
        for message in &view.messages {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                continue;
            }
            match message.role {
                phi_domain::message::Role::User if user.is_empty() => user = text,
                phi_domain::message::Role::Assistant if assistant.is_empty() => assistant = text,
                _ => {}
            }
            if !user.is_empty() && !assistant.is_empty() {
                break;
            }
        }
        if user.is_empty() || assistant.is_empty() {
            return Ok(None);
        }
        let user: String = user.chars().take(2_000).collect();
        let assistant: String = assistant.chars().take(2_000).collect();
        let text = self
            .call_role(
                AUX_SESSION_TITLE,
                "session_title",
                "You name conversations. Reply with exactly one short title line, no quotes, no markdown, at most 60 characters.",
                &format!("User: {user}\nAssistant: {assistant}"),
                64,
                Some(session_id),
                cancel,
            )
            .await?;
        let title = text
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '“' | '”' | '‘' | '’'))
            .trim()
            .to_string();
        if title.is_empty() || title.chars().count() > 60 {
            return Ok(None);
        }
        session.rename_session(session_id, &title, "auxiliary")?;
        Ok(Some(title))
    }

    async fn summarize(
        &self,
        text: &str,
        max_chars: usize,
        cancel: CancellationToken,
    ) -> Result<Option<String>, String> {
        if !self.summarizer_ready() || text.trim().is_empty() {
            return Ok(None);
        }
        let input: String = text.chars().take(60_000).collect();
        let system = format!(
            "Summarize the tool output below for another agent. Keep exact paths, commands, error messages and numbers. Return plain text, at most {max_chars} characters.",
        );
        let summary = self
            .call_role(
                AUX_SUMMARIZER,
                "tool_output_summary",
                &system,
                &input,
                2_048,
                None,
                cancel,
            )
            .await?;
        let summary = summary.trim();
        if summary.is_empty() {
            return Ok(None);
        }
        Ok(Some(summary.chars().take(max_chars).collect()))
    }

    fn maintenance_due(&self, project_id: Option<&str>) -> bool {
        if !self.maintenance_ready() {
            return false;
        }
        let Ok((_, _, active)) = self.active_facts(project_id) else {
            return false;
        };
        let last = self.maintenance.snapshot().last_fact_count;
        active >= last.saturating_add(MAINTENANCE_FACT_THRESHOLD)
    }

    async fn maintain_memory(
        &self,
        project_id: Option<String>,
        dry_run: bool,
        trigger: &str,
        cancel: CancellationToken,
    ) -> Result<MaintenanceReport, String> {
        if !self.maintenance_ready() {
            return Err(i18n::t("The memory maintenance role is not ready"));
        }
        let memory = self
            .services
            .memory()
            .ok_or_else(|| i18n::t("Memory store is not available"))?;
        let (principal, records, active) = self.active_facts(project_id.as_deref())?;
        let _ = self.bus.publish(
            "node",
            "plugin-auxiliary",
            &phi_domain::events::DomainEvent::MemoryMaintenanceStarted {
                trigger: trigger.to_string(),
                batch_size: records.len() as u64,
                dry_run,
            },
        );
        if records.is_empty() {
            let report = MaintenanceReport {
                trigger: trigger.to_string(),
                dry_run,
                skipped: vec![i18n::t("No active facts to maintain")],
                ..Default::default()
            };
            if !dry_run {
                self.mark_maintained(active)?;
            }
            return Ok(report);
        }
        let mut budget = MAX_MAINTENANCE_CHARS;
        let mut payload = Vec::new();
        let mut included = Vec::new();
        for record in &records {
            let fact: String = record.fact.chars().take(MAX_FACT_CHARS).collect();
            let line = json!({
                "id": record.id,
                "scope": record.scope.as_str(),
                "trust": record.source_trust.as_str(),
                "confidence": record.confidence,
                "fact": fact,
            });
            let size = line.to_string().chars().count();
            if size > budget && !payload.is_empty() {
                break;
            }
            budget = budget.saturating_sub(size);
            payload.push(line);
            included.push(record.clone());
        }
        let system = "You maintain an agent's long-term memory. Given active facts as JSON, return ONLY a JSON object: \
            {\"operations\":[{\"op\":\"merge\",\"ids\":[\"id\",\"id\"],\"fact\":\"merged fact\",\"reason\":\"...\"}, \
            {\"op\":\"supersede\",\"oldId\":\"id\",\"fact\":\"new fact\",\"reason\":\"...\"}, \
            {\"op\":\"correct\",\"id\":\"id\",\"fact\":\"corrected fact\",\"reason\":\"...\"}, \
            {\"op\":\"invalidate\",\"id\":\"id\",\"reason\":\"...\"}]}. \
            Never emit forget. Only merge clear duplicates; only supersede when a fact is clearly outdated. \
            Keep facts short and self-contained. Do not invent ids.";
        let user = format!(
            "Active facts:\n{}",
            serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_string())
        );
        let response = self
            .call_role(
                AUX_MEMORY_MAINTENANCE,
                "memory_maintenance",
                system,
                &user,
                4_096,
                None,
                cancel,
            )
            .await?;
        let operations = parse_operations(&response)?;
        let kinds: Vec<String> = {
            let mut kinds: Vec<String> = operations
                .iter()
                .filter_map(|operation| operation.get("op").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            kinds.sort();
            kinds.dedup();
            kinds
        };
        let _ = self.bus.publish(
            "node",
            "plugin-auxiliary",
            &phi_domain::events::DomainEvent::MemoryMaintenanceProposed {
                proposal_count: operations.len() as u64,
                kinds,
            },
        );

        let by_id: BTreeMap<&str, &MemoryRecord> = included
            .iter()
            .map(|record| (record.id.as_str(), record))
            .collect();
        let mut report = MaintenanceReport {
            trigger: trigger.to_string(),
            dry_run,
            proposals: operations.len() as u64,
            ..Default::default()
        };
        for operation in operations {
            let Some(op) = operation.get("op").and_then(Value::as_str) else {
                report.skipped.push(i18n::t("Operation has no op field"));
                continue;
            };
            let reason = operation
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("maintenance")
                .to_string();
            match op {
                "merge" => {
                    let ids: Vec<String> = operation
                        .get("ids")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let fact = operation
                        .get("fact")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if ids.len() < 2 || fact.is_empty() {
                        report.skipped.push(i18n::t("Invalid merge operation"));
                        continue;
                    }
                    let mut selected = Vec::new();
                    let mut missing = false;
                    for id in &ids {
                        match by_id.get(id.as_str()) {
                            Some(record) => selected.push((*record).clone()),
                            None => missing = true,
                        }
                    }
                    if missing || fact.chars().count() > MAX_FACT_CHARS {
                        report
                            .skipped
                            .push(i18n::t("Merge references an unknown or oversized fact"));
                        continue;
                    }
                    if selected
                        .iter()
                        .any(|record| record.scope == MemoryScope::User)
                    {
                        report.pending.push(MaintenanceProposalView {
                            op: op.to_string(),
                            ids,
                            reason,
                        });
                        continue;
                    }
                    if dry_run {
                        report.pending.push(MaintenanceProposalView {
                            op: op.to_string(),
                            ids,
                            reason,
                        });
                        continue;
                    }
                    let first = &selected[0];
                    let request = MemoryAddRequest {
                        fact: fact.clone(),
                        scope: first.scope,
                        source_trust: first.source_trust,
                        confidence: Some(first.confidence),
                        valid_from: first.valid_from.clone(),
                        valid_to: first.valid_to.clone(),
                        episode: None,
                        reason: Some(reason.clone()),
                    };
                    match memory.supersede(&principal, &first.id, request, &reason) {
                        Ok(_) => {
                            for record in selected.iter().skip(1) {
                                let _ = memory.invalidate(&principal, &record.id, &reason);
                            }
                            report.applied += 1;
                        }
                        Err(error) => {
                            report.skipped.push(error);
                        }
                    }
                }
                "supersede" => {
                    let old_id = operation
                        .get("oldId")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let fact = operation
                        .get("fact")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let Some(old) = by_id.get(old_id) else {
                        report
                            .skipped
                            .push(i18n::t("Supersede references an unknown fact"));
                        continue;
                    };
                    if fact.is_empty() || fact.chars().count() > MAX_FACT_CHARS {
                        report.skipped.push(i18n::t("Invalid supersede operation"));
                        continue;
                    }
                    if old.scope == MemoryScope::User {
                        report.pending.push(MaintenanceProposalView {
                            op: op.to_string(),
                            ids: vec![old_id.to_string()],
                            reason,
                        });
                        continue;
                    }
                    if dry_run {
                        report.pending.push(MaintenanceProposalView {
                            op: op.to_string(),
                            ids: vec![old_id.to_string()],
                            reason,
                        });
                        continue;
                    }
                    let request = MemoryAddRequest {
                        fact,
                        scope: old.scope,
                        source_trust: old.source_trust,
                        confidence: Some(old.confidence),
                        valid_from: old.valid_from.clone(),
                        valid_to: old.valid_to.clone(),
                        episode: None,
                        reason: Some(reason.clone()),
                    };
                    match memory.supersede(&principal, old_id, request, &reason) {
                        Ok(_) => report.applied += 1,
                        Err(error) => report.skipped.push(error),
                    }
                }
                "correct" | "invalidate" => {
                    let id = operation
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    report.pending.push(MaintenanceProposalView {
                        op: op.to_string(),
                        ids: if id.is_empty() {
                            Vec::new()
                        } else {
                            vec![id.to_string()]
                        },
                        reason,
                    });
                }
                "forget" => {
                    report
                        .skipped
                        .push(i18n::t("Forget is never performed automatically"));
                }
                other => {
                    report.skipped.push(i18n::t_args(
                        "Unknown maintenance operation: {op}",
                        &[("op", other.to_string())],
                    ));
                }
            }
        }
        let _ = self.bus.publish(
            "node",
            "plugin-auxiliary",
            &phi_domain::events::DomainEvent::MemoryMaintenanceApplied {
                applied: report.applied,
                pending: report.pending.len() as u64,
                reason: if dry_run {
                    "dry run".to_string()
                } else {
                    trigger.to_string()
                },
            },
        );
        if !dry_run {
            self.mark_maintained(active)?;
        }
        self.log.info(&format!(
            "memory maintenance: {} proposals, {} applied, {} pending",
            report.proposals,
            report.applied,
            report.pending.len()
        ));
        Ok(report)
    }
}

impl AuxiliaryPlugin {
    fn mark_maintained(&self, active: u64) -> Result<(), String> {
        self.maintenance.update(|state| {
            state.last_fact_count = active;
            state.last_run_at = Some(self.clock.now_rfc3339());
        })?;
        Ok(())
    }
}

fn parse_operations(text: &str) -> Result<Vec<Value>, String> {
    let start = text
        .find('{')
        .ok_or_else(|| i18n::t("The memory maintenance model returned no JSON object"))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| i18n::t("The memory maintenance model returned incomplete JSON"))?;
    let value: Value = serde_json::from_str(&text[start..=end]).map_err(|error| {
        i18n::t_args(
            "The memory maintenance response is not valid JSON: {error}",
            &[("error", error.to_string())],
        )
    })?;
    value
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| i18n::t("The memory maintenance response has no operations array"))
}

impl Plugin for AuxiliaryPlugin {
    fn id(&self) -> &'static str {
        "plugin-auxiliary"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_AUXILIARY_INVOKE.to_string(),
            id: "default".to_string(),
        }])
    }
}
