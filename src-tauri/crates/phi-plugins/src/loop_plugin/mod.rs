use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use phi_contracts::SLOT_RUNTIME_SKELETON;
use phi_contracts::ports::{RunStatus, RuntimePort, StartRunRequest, ThinkingEffort};
use phi_contracts::services::Services;
use phi_domain::agent::{AgentRole, WorkMode};
use phi_domain::events::DomainEvent;
use phi_domain::view::RunState;
use phi_i18n as i18n;
use phi_kernel::event_bus::EventBus;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::ports::{BlobPort, IdPort, LogPort};
use phi_kernel::registry::Contribution;

mod context;
mod run;
mod support;
mod tool_pipeline;

struct RunControl {
    cancel: CancellationToken,
    status: Arc<Mutex<RunStatus>>,
    session_id: String,
    root_agent_id: String,
    parent_run_id: Option<String>,
}

pub struct LoopPlugin {
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    log: Arc<dyn LogPort>,
    blob: Arc<dyn BlobPort>,
    services: Arc<Services>,
    running: Arc<Mutex<HashMap<String, RunControl>>>,
}

impl LoopPlugin {
    pub fn new(
        bus: Arc<EventBus>,
        ids: Arc<dyn IdPort>,
        log: Arc<dyn LogPort>,
        blob: Arc<dyn BlobPort>,
        services: Arc<Services>,
    ) -> Self {
        Self {
            bus,
            ids,
            log,
            blob,
            services,
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Plugin for LoopPlugin {
    fn id(&self) -> &'static str {
        "plugin-loop"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_RUNTIME_SKELETON.to_string(),
            id: "react".to_string(),
        }])
    }
}

impl RuntimePort for LoopPlugin {
    fn start_run(&self, req: StartRunRequest) -> Result<String, String> {
        let session = self.services.session().ok_or_else(|| {
            i18n::t("Session store is not available (session.store slot is missing)")
        })?;
        let model = self
            .services
            .model()
            .ok_or_else(|| i18n::t("Model gateway is not available"))?;
        if !req.workspace.is_dir() {
            return Err(i18n::t("Workspace does not exist or is not a directory"));
        }

        let run_id = self.ids.new_id("run");
        let is_child = req.parent_run_id.is_some();
        let mut root_agent_id = req.root_agent_id.clone();
        let (agent_id, thread_id, work_mode, report_to, resolved_model, lead_scale) = if is_child {
            let parent_run_id = req.parent_run_id.clone().unwrap_or_default();
            let (parent_session, parent) = session.find_run(&parent_run_id).ok_or_else(|| {
                i18n::t_args(
                    "Parent run not found: {run_id}",
                    &[("run_id", parent_run_id.clone())],
                )
            })?;
            if parent_session != req.session_id {
                return Err(i18n::t("The parent run belongs to another session"));
            }
            if !parent.is_active() {
                return Err(i18n::t("The parent run is no longer active"));
            }
            if parent.parent_run_id.is_some() {
                return Err(i18n::t("A subagent cannot delegate to another subagent"));
            }
            let agent_id = req
                .agent_id
                .clone()
                .ok_or_else(|| i18n::t("A subagent run requires an agent id"))?;
            let thread_id = req
                .thread_id
                .clone()
                .ok_or_else(|| i18n::t("A subagent run requires a thread id"))?;
            if session.agent(&agent_id).is_none() {
                return Err(i18n::t_args(
                    "Unknown subagent: {agent}",
                    &[("agent", agent_id.clone())],
                ));
            }
            if session.active_run_for_agent(&agent_id).is_some() {
                return Err(i18n::t_args(
                    "Subagent {agent} is already running",
                    &[("agent", agent_id.clone())],
                ));
            }
            let work_mode = req.work_mode;
            if work_mode == WorkMode::Shared {
                if let Some(holder) = session.write_lease_holder(&req.session_id) {
                    if holder != agent_id {
                        return Err(i18n::t("Another agent holds the workspace write lease"));
                    }
                }
            }
            let resolved = model.resolve_model(req.model.as_deref().unwrap_or(""));
            let roster = model.roster();
            let spec = roster
                .iter()
                .find(|model| model.id.eq_ignore_ascii_case(&resolved))
                .ok_or_else(|| {
                    i18n::t_args(
                        "Unknown subagent model: {model}",
                        &[("model", resolved.clone())],
                    )
                })?;
            let lead_scale = req.lead_scale.unwrap_or_else(|| {
                session
                    .lead_agent(&req.session_id)
                    .and_then(|lead| {
                        roster
                            .iter()
                            .find(|model| model.id.eq_ignore_ascii_case(&lead.model))
                            .map(|model| model.scale)
                    })
                    .unwrap_or(3)
            });
            if spec.scale > lead_scale {
                return Err(i18n::t_args(
                    "Subagent model {model} is more capable than the lead model",
                    &[("model", resolved.clone())],
                ));
            }
            if root_agent_id.is_none() {
                root_agent_id = session
                    .lead_agent(&req.session_id)
                    .map(|lead| lead.id)
                    .or_else(|| Some(agent_id.clone()));
            }
            (
                agent_id,
                thread_id,
                work_mode,
                req.report_to.clone(),
                resolved,
                lead_scale,
            )
        } else {
            if session.active_root_run(&req.session_id).is_some() {
                return Err(i18n::t_args(
                    "This session still has a running task: {run_id}",
                    &[(
                        "run_id",
                        session.active_root_run(&req.session_id).unwrap_or_default(),
                    )],
                ));
            }
            let resolved = model.resolve_model(req.model.as_deref().unwrap_or("default"));
            let lead_scale = model
                .roster()
                .iter()
                .find(|model| model.id.eq_ignore_ascii_case(&resolved))
                .map(|model| model.scale)
                .unwrap_or(3);
            let lead = session.lead_agent(&req.session_id);
            let (agent_id, thread_id) = match lead {
                Some(lead) => (lead.id, lead.thread_id),
                None => {
                    let agent_id = self.ids.new_id("agent");
                    let thread_id = "main".to_string();
                    self.bus.publish(
                        &req.session_id,
                        "plugin-loop",
                        &DomainEvent::AgentCreated {
                            agent_id: agent_id.clone(),
                            session_id: req.session_id.clone(),
                            turn_id: req.turn_id.clone(),
                            role: AgentRole::Lead,
                            parent_agent_id: None,
                            root_agent_id: agent_id.clone(),
                            model: resolved.clone(),
                            thinking: req.thinking.map(|thinking| thinking.as_str().to_string()),
                            thread_id: thread_id.clone(),
                            work_mode: WorkMode::Shared,
                            report_to: None,
                            spawned_by_run_id: run_id.clone(),
                        },
                    )?;
                    (agent_id, thread_id)
                }
            };
            (
                agent_id,
                thread_id,
                WorkMode::Shared,
                None,
                resolved,
                lead_scale,
            )
        };
        let root_agent_id = root_agent_id.unwrap_or_else(|| agent_id.clone());
        let role = if is_child {
            AgentRole::Subagent
        } else {
            AgentRole::Lead
        };
        let parent_run_id = req.parent_run_id.clone();

        self.bus.publish(
            &req.session_id,
            "plugin-loop",
            &DomainEvent::RunScheduled {
                run_id: run_id.clone(),
                session_id: req.session_id.clone(),
                turn_id: req.turn_id.clone(),
                agent_id: Some(agent_id.clone()),
                thread_id: Some(thread_id.clone()),
                parent_run_id: parent_run_id.clone(),
            },
        )?;

        let status = Arc::new(Mutex::new(RunStatus {
            run_id: run_id.clone(),
            session_id: req.session_id.clone(),
            state: RunState::Scheduled,
            steps: 0,
            agent_id: Some(agent_id.clone()),
            thread_id: Some(thread_id.clone()),
            parent_run_id: parent_run_id.clone(),
        }));
        let cancel = CancellationToken::new();
        {
            let mut running = self.running.lock().unwrap();
            running.insert(
                run_id.clone(),
                RunControl {
                    cancel: cancel.clone(),
                    status: status.clone(),
                    session_id: req.session_id.clone(),
                    root_agent_id: root_agent_id.clone(),
                    parent_run_id: parent_run_id.clone(),
                },
            );
        }

        let ctx = RunContext {
            bus: self.bus.clone(),
            ids: self.ids.clone(),
            blob: self.blob.clone(),
            services: self.services.clone(),
            running: self.running.clone(),
            run_id: run_id.clone(),
            session_id: req.session_id,
            turn_id: req.turn_id,
            workspace: req.workspace,
            resolved_model,
            thinking: req.thinking,
            role,
            agent_id,
            root_agent_id,
            thread_id,
            parent_run_id,
            work_mode,
            report_to,
            lead_scale,
            cancel,
            status,
            image_bytes: Mutex::new(None),
        };
        tokio::spawn(run::run_loop(ctx));
        Ok(run_id)
    }

    fn cancel(&self, run_id: &str, reason: &str) -> bool {
        let running = self.running.lock().unwrap();
        let Some(control) = running.get(run_id) else {
            return false;
        };
        self.log
            .info(&format!("cancel requested for {run_id}: {reason}"));
        control.cancel.cancel();
        if control.parent_run_id.is_none() {
            let session_id = control.session_id.clone();
            let root_agent_id = control.root_agent_id.clone();
            let descendants: Vec<CancellationToken> = running
                .values()
                .filter(|other| {
                    other.session_id == session_id
                        && other.root_agent_id == root_agent_id
                        && other.parent_run_id.is_some()
                })
                .map(|other| other.cancel.clone())
                .collect();
            for token in descendants {
                token.cancel();
            }
        }
        true
    }

    fn status(&self, run_id: &str) -> Option<RunStatus> {
        self.running
            .lock()
            .unwrap()
            .get(run_id)
            .map(|control| control.status.lock().unwrap().clone())
    }
}

struct RunContext {
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    blob: Arc<dyn BlobPort>,
    services: Arc<Services>,
    running: Arc<Mutex<HashMap<String, RunControl>>>,
    run_id: String,
    session_id: String,
    turn_id: String,
    workspace: PathBuf,
    /// The model id after provider mapping; this is what the gateway receives.
    resolved_model: String,
    /// Per-message extended reasoning effort.
    thinking: Option<ThinkingEffort>,
    role: AgentRole,
    agent_id: String,
    root_agent_id: String,
    thread_id: String,
    parent_run_id: Option<String>,
    work_mode: WorkMode,
    report_to: Option<String>,
    lead_scale: u8,
    cancel: CancellationToken,
    status: Arc<Mutex<RunStatus>>,
    /// Image bytes resolved from the asset store on first use. The map is
    /// immutable for the run and shared with each model request.
    image_bytes: Mutex<Option<Arc<HashMap<String, Vec<u8>>>>>,
}
