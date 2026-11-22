use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use phi_contracts::SLOT_SESSION_STORE;
use phi_contracts::ports::{SessionPort, TurnInfo};
use phi_domain::agent::{AgentRole, AgentStatus, AgentView, MessageKind, WorkMode};
use phi_domain::events::DomainEvent;
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_domain::view::{MessageView, RunView, SessionSummary, SessionView};
use phi_i18n as i18n;
use phi_kernel::event_bus::{EventBus, EventRecord, EventSink};
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::ports::{BlobPort, IdPort};
use phi_kernel::registry::Contribution;

#[derive(Clone)]
struct StoredMessage {
    id: String,
    turn_id: Option<String>,
    role: Role,
    ts: String,
    content_ref: String,
    thread_id: String,
    agent_id: Option<String>,
    to_agent_id: Option<String>,
    kind: Option<MessageKind>,
}

const MAIN_THREAD: &str = "main";

fn thread_of(message: &StoredMessage) -> &str {
    &message.thread_id
}

#[derive(Clone)]
struct RunRecord {
    id: String,
    turn_id: String,
    state: String,
    steps: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    pending_approval_id: Option<String>,
    error: Option<String>,
    stop_confirmation: Option<String>,
    started_at: Option<String>,
    updated_at: String,
    agent_id: Option<String>,
    thread_id: String,
    parent_run_id: Option<String>,
}

#[derive(Clone)]
struct AgentRecord {
    id: String,
    session_id: String,
    turn_id: String,
    role: AgentRole,
    parent_agent_id: Option<String>,
    root_agent_id: String,
    model: String,
    thinking: Option<String>,
    thread_id: String,
    work_mode: WorkMode,
    status: AgentStatus,
    report_to: Option<String>,
    spawned_by_run_id: String,
    report_message_id: Option<String>,
    steps: u64,
    input_tokens: u64,
    output_tokens: u64,
    created_at: String,
    updated_at: String,
}

impl RunRecord {
    fn is_active(&self) -> bool {
        matches!(
            self.state.as_str(),
            "Scheduled" | "Running" | "WaitingApproval"
        )
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.state.as_str(),
            "Succeeded" | "Failed" | "Aborted" | "AwaitingReconciliation"
        )
    }
}

#[derive(Clone)]
struct StoredSession {
    id: String,
    owner_node_id: String,
    title: String,
    created_at: String,
    updated_at: String,
    messages: Vec<StoredMessage>,
    runs: HashMap<String, RunRecord>,
    run_order: Vec<String>,
    agents: HashMap<String, AgentRecord>,
    turn_count: u64,
}

#[derive(Default)]
struct Projection {
    sessions: HashMap<String, StoredSession>,
}

/// Session read model, rebuilt by folding the journal.
///
/// Session is the consistency boundary for v1 (ADR-0011): run/tool/approval
/// events on a session line all use the session id as aggregate id.
pub struct SessionPlugin {
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    blob: Arc<dyn BlobPort>,
    node_id: String,
    state: RwLock<Projection>,
    /// Resolved message content, keyed by blob ref. The webview pulls a full
    /// session snapshot on every event; without this cache each pull would
    /// re-read every message file from disk.
    blocks: RwLock<HashMap<String, Vec<ContentBlock>>>,
}

impl SessionPlugin {
    pub fn new(
        bus: Arc<EventBus>,
        ids: Arc<dyn IdPort>,
        blob: Arc<dyn BlobPort>,
        node_id: String,
    ) -> Self {
        Self {
            bus,
            ids,
            blob,
            node_id,
            state: RwLock::new(Projection::default()),
            blocks: RwLock::new(HashMap::new()),
        }
    }

    pub fn load(&self, records: &[EventRecord]) {
        let mut projection = self.state.write().unwrap();
        for record in records {
            apply_record(&mut projection, record);
        }
    }

    /// On startup, a run without a terminal event means we crashed mid-flight.
    /// Never auto-retry: mark it for human reconciliation (I-5-4 / I-3-8).
    pub fn recover_interrupted(&self) -> usize {
        let mut pending: Vec<(String, String, String)> = Vec::new();
        {
            let projection = self.state.read().unwrap();
            for session in projection.sessions.values() {
                for run_id in &session.run_order {
                    if let Some(run) = session.runs.get(run_id) {
                        if !run.is_terminal() {
                            pending.push((session.id.clone(), run.id.clone(), run.state.clone()));
                        }
                    }
                }
            }
        }

        for (session_id, run_id, state) in &pending {
            let note = if state == "WaitingApproval" {
                i18n::t(
                    "The process restarted while an approval was pending; the action was not executed. Review it and send the request again.",
                )
            } else {
                i18n::t(
                    "The process restarted before the run finished; side effects are unknown. Inspect the workspace before retrying.",
                )
            };
            let _ = self.bus.publish(
                session_id,
                "plugin-session",
                &DomainEvent::RunInterrupted {
                    run_id: run_id.clone(),
                    session_id: session_id.clone(),
                    note: note.to_string(),
                },
            );
        }

        // An agent can be left in an active state if the process died before
        // its run started, or between a terminal run event and the status
        // sync. Never leave it holding the write lease.
        let stranded: Vec<(String, String)> = {
            let projection = self.state.read().unwrap();
            let mut out = Vec::new();
            for session in projection.sessions.values() {
                for agent in session.agents.values() {
                    if agent.status.is_active()
                        && !session.runs.values().any(|run| {
                            run.agent_id.as_deref() == Some(agent.id.as_str()) && run.is_active()
                        })
                    {
                        out.push((session.id.clone(), agent.id.clone()));
                    }
                }
            }
            out
        };
        for (session_id, agent_id) in stranded {
            let _ = self.bus.publish(
                &session_id,
                "plugin-session",
                &DomainEvent::AgentStatusChanged {
                    agent_id,
                    session_id: session_id.clone(),
                    status: AgentStatus::Interrupted,
                    note: Some(i18n::t(
                        "The process restarted while this agent was active; it was not resumed.",
                    )),
                },
            );
        }

        // A crash or a failed result append can leave an unpaired tool_use.
        // Repair every session, not only the ones with a still-active run: the
        // append may have failed after the run was already marked terminal,
        // and the provider would reject the next model call. The repair is
        // idempotent.
        let all_sessions: Vec<String> = {
            let projection = self.state.read().unwrap();
            projection.sessions.keys().cloned().collect()
        };
        for session_id in &all_sessions {
            let _ = self.repair_unpaired_tool_uses(session_id);
        }
        pending.len()
    }

    fn repair_unpaired_tool_uses(&self, session_id: &str) -> Result<usize, String> {
        let stored = {
            let projection = self.state.read().unwrap();
            match projection.sessions.get(session_id) {
                Some(session) => session.messages.clone(),
                None => return Ok(0),
            }
        };

        let mut resolved: HashSet<String> = HashSet::new();
        for message in &stored {
            for block in self.resolve_blocks(&message.content_ref) {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    resolved.insert(tool_use_id);
                }
            }
        }

        let mut missing: HashMap<String, Vec<String>> = HashMap::new();
        for message in &stored {
            for block in self.resolve_blocks(&message.content_ref) {
                if let ContentBlock::ToolUse { id, .. } = block {
                    if !resolved.contains(&id) {
                        let thread = message.thread_id.clone();
                        let entries = missing.entry(thread).or_default();
                        if !entries.contains(&id) {
                            entries.push(id);
                        }
                    }
                }
            }
        }
        if missing.is_empty() {
            return Ok(0);
        }

        let mut repaired = 0usize;
        for (thread_id, ids) in missing {
            let blocks = ids
                .iter()
                .map(|tool_use_id| {
                    ContentBlock::tool_result(
                        tool_use_id.clone(),
                        i18n::t("Run interrupted before the tool result was recorded; the action may have executed. The run needs manual reconciliation and will not be retried automatically."),
                        true,
                    )
                })
                .collect();
            self.append_message_in_thread(
                session_id,
                None,
                &thread_id,
                None,
                None,
                Some(MessageKind::Tool),
                Role::User,
                blocks,
            )?;
            repaired += ids.len();
        }
        Ok(repaired)
    }

    fn summary_of(session: &StoredSession) -> SessionSummary {
        SessionSummary {
            id: session.id.clone(),
            title: session.title.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
            message_count: session
                .messages
                .iter()
                .filter(|message| thread_of(message) == MAIN_THREAD)
                .count(),
            last_run_state: session
                .run_order
                .iter()
                .rev()
                .filter_map(|id| session.runs.get(id))
                .find(|run| run.parent_run_id.is_none())
                .map(|r| r.state.clone()),
        }
    }
}

impl SessionPort for SessionPlugin {
    fn node_id(&self) -> String {
        self.node_id.clone()
    }

    fn create_session(&self, title: &str) -> Result<SessionSummary, String> {
        let session_id = self.ids.new_id("sess");
        let title = title.trim();
        let title = if title.is_empty() {
            i18n::t("New session")
        } else {
            title.to_string()
        };
        self.bus.publish(
            &session_id,
            "plugin-session",
            &DomainEvent::SessionStarted {
                session_id: session_id.clone(),
                owner_node_id: self.node_id.clone(),
                title,
            },
        )?;
        self.list_sessions()
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| i18n::t("Session projection did not materialize"))
    }

    fn list_sessions(&self) -> Vec<SessionSummary> {
        let projection = self.state.read().unwrap();
        let mut sessions: Vec<SessionSummary> =
            projection.sessions.values().map(Self::summary_of).collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    fn get_session(&self, session_id: &str) -> Option<SessionView> {
        let stored = {
            let projection = self.state.read().unwrap();
            projection.sessions.get(session_id)?.clone()
        };

        let messages = stored
            .messages
            .iter()
            .filter(|message| thread_of(message) == MAIN_THREAD)
            .map(|m| MessageView {
                id: m.id.clone(),
                turn_id: m.turn_id.clone(),
                role: m.role,
                ts: m.ts.clone(),
                blocks: self.resolve_blocks(&m.content_ref),
                thread_id: Some(m.thread_id.clone()),
                agent_id: m.agent_id.clone(),
                to_agent_id: m.to_agent_id.clone(),
                kind: m.kind,
            })
            .collect();

        let runs = stored
            .run_order
            .iter()
            .filter_map(|id| stored.runs.get(id))
            .map(run_view)
            .collect();

        let mut agents: Vec<AgentView> = stored.agents.values().map(agent_view).collect();
        agents.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Some(SessionView {
            id: stored.id.clone(),
            owner_node_id: stored.owner_node_id.clone(),
            title: stored.title.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
            messages,
            runs,
            agents,
        })
    }

    fn start_turn(&self, session_id: &str) -> Result<TurnInfo, String> {
        let index = {
            let projection = self.state.read().unwrap();
            let session = projection.sessions.get(session_id).ok_or_else(|| {
                i18n::t_args("Unknown session: {id}", &[("id", session_id.to_string())])
            })?;
            session.turn_count + 1
        };
        let turn_id = self.ids.new_id("turn");
        self.bus.publish(
            session_id,
            "plugin-session",
            &DomainEvent::TurnStarted {
                session_id: session_id.to_string(),
                turn_id: turn_id.clone(),
                index,
            },
        )?;
        Ok(TurnInfo { turn_id, index })
    }

    fn append_message(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        role: Role,
        blocks: Vec<ContentBlock>,
    ) -> Result<String, String> {
        self.append_message_in_thread(
            session_id,
            turn_id,
            MAIN_THREAD,
            None,
            None,
            None,
            role,
            blocks,
        )
    }

    fn append_message_in_thread(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        thread_id: &str,
        agent_id: Option<&str>,
        to_agent_id: Option<&str>,
        kind: Option<MessageKind>,
        role: Role,
        blocks: Vec<ContentBlock>,
    ) -> Result<String, String> {
        {
            let projection = self.state.read().unwrap();
            if !projection.sessions.contains_key(session_id) {
                return Err(i18n::t_args(
                    "Unknown session: {id}",
                    &[("id", session_id.to_string())],
                ));
            }
        }
        let raw = serde_json::to_vec(&blocks).map_err(|e| e.to_string())?;
        let content_ref = self.blob.put(&raw)?;
        self.blocks
            .write()
            .unwrap()
            .insert(content_ref.clone(), blocks);
        let message_id = self.ids.new_id("msg");
        self.bus.publish(
            session_id,
            "plugin-session",
            &DomainEvent::MessageAppended {
                session_id: session_id.to_string(),
                turn_id: turn_id.map(|s| s.to_string()),
                message_id: message_id.clone(),
                role,
                content_ref,
                thread_id: Some(thread_id.to_string()),
                agent_id: agent_id.map(str::to_string),
                to_agent_id: to_agent_id.map(str::to_string),
                kind,
            },
        )?;
        Ok(message_id)
    }

    fn messages(&self, session_id: &str) -> Vec<ChatMessage> {
        self.messages_for_thread(session_id, MAIN_THREAD)
    }

    fn messages_for_thread(&self, session_id: &str, thread_id: &str) -> Vec<ChatMessage> {
        let stored = {
            let projection = self.state.read().unwrap();
            match projection.sessions.get(session_id) {
                Some(session) => session.messages.clone(),
                None => return Vec::new(),
            }
        };
        stored
            .iter()
            .filter(|m| thread_of(m) == thread_id)
            .map(|m| ChatMessage {
                role: m.role,
                blocks: self.resolve_blocks(&m.content_ref),
            })
            .collect()
    }

    fn active_run(&self, session_id: &str) -> Option<String> {
        let projection = self.state.read().unwrap();
        let session = projection.sessions.get(session_id)?;
        session
            .run_order
            .iter()
            .filter_map(|id| session.runs.get(id))
            .find(|r| r.is_active())
            .map(|r| r.id.clone())
    }

    fn active_root_run(&self, session_id: &str) -> Option<String> {
        let projection = self.state.read().unwrap();
        let session = projection.sessions.get(session_id)?;
        session
            .run_order
            .iter()
            .filter_map(|id| session.runs.get(id))
            .find(|r| r.is_active() && r.parent_run_id.is_none())
            .map(|r| r.id.clone())
    }

    fn active_run_for_agent(&self, agent_id: &str) -> Option<String> {
        let projection = self.state.read().unwrap();
        for session in projection.sessions.values() {
            if let Some(run) = session
                .run_order
                .iter()
                .filter_map(|id| session.runs.get(id))
                .find(|run| run.agent_id.as_deref() == Some(agent_id) && run.is_active())
            {
                return Some(run.id.clone());
            }
        }
        None
    }

    fn agents(&self, session_id: &str) -> Vec<AgentView> {
        let projection = self.state.read().unwrap();
        projection
            .sessions
            .get(session_id)
            .map(|session| session.agents.values().map(agent_view).collect())
            .unwrap_or_default()
    }

    fn agent(&self, agent_id: &str) -> Option<AgentView> {
        let projection = self.state.read().unwrap();
        projection
            .sessions
            .values()
            .find_map(|session| session.agents.get(agent_id))
            .map(agent_view)
    }

    fn write_lease_holder(&self, session_id: &str) -> Option<String> {
        let projection = self.state.read().unwrap();
        let session = projection.sessions.get(session_id)?;
        session
            .agents
            .values()
            .find(|agent| {
                agent.role == AgentRole::Subagent
                    && agent.work_mode == WorkMode::Shared
                    && agent.status.is_active()
            })
            .map(|agent| agent.id.clone())
    }

    fn find_run(&self, run_id: &str) -> Option<(String, RunView)> {
        let projection = self.state.read().unwrap();
        for session in projection.sessions.values() {
            if let Some(run) = session.runs.get(run_id) {
                return Some((session.id.clone(), run_view(run)));
            }
        }
        None
    }
}

impl SessionPlugin {
    fn resolve_blocks(&self, content_ref: &str) -> Vec<ContentBlock> {
        if let Some(cached) = self.blocks.read().unwrap().get(content_ref).cloned() {
            return cached;
        }
        let resolved = match self.blob.get(content_ref) {
            Ok(raw) => serde_json::from_slice::<Vec<ContentBlock>>(&raw).unwrap_or_else(|_| {
                vec![ContentBlock::text(i18n::t("(content could not be parsed)"))]
            }),
            Err(_) => vec![ContentBlock::text(i18n::t("(content is not readable)"))],
        };
        self.blocks
            .write()
            .unwrap()
            .insert(content_ref.to_string(), resolved.clone());
        resolved
    }
}

impl EventSink for SessionPlugin {
    fn on_event(&self, record: &EventRecord) {
        let mut projection = self.state.write().unwrap();
        apply_record(&mut projection, record);
    }
}

impl Plugin for SessionPlugin {
    fn id(&self) -> &'static str {
        "plugin-session"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_SESSION_STORE.to_string(),
            id: "default".to_string(),
        }])
    }
}

fn apply_record(projection: &mut Projection, record: &EventRecord) {
    let event: DomainEvent = match serde_json::from_value(record.payload.clone()) {
        Ok(event) => event,
        Err(_) => return,
    };

    match event {
        DomainEvent::SessionStarted {
            session_id,
            owner_node_id,
            title,
        } => {
            projection
                .sessions
                .entry(session_id.clone())
                .or_insert_with(|| StoredSession {
                    id: session_id,
                    owner_node_id,
                    title,
                    created_at: record.ts.clone(),
                    updated_at: record.ts.clone(),
                    messages: Vec::new(),
                    runs: HashMap::new(),
                    run_order: Vec::new(),
                    agents: HashMap::new(),
                    turn_count: 0,
                });
        }
        DomainEvent::TurnStarted {
            session_id, index, ..
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                session.turn_count = session.turn_count.max(index);
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::MessageAppended {
            session_id,
            turn_id,
            message_id,
            role,
            content_ref,
            thread_id,
            agent_id,
            to_agent_id,
            kind,
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                session.messages.push(StoredMessage {
                    id: message_id,
                    turn_id,
                    role,
                    ts: record.ts.clone(),
                    content_ref,
                    thread_id: thread_id.unwrap_or_else(|| MAIN_THREAD.to_string()),
                    agent_id,
                    to_agent_id,
                    kind,
                });
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::AgentCreated {
            agent_id,
            session_id,
            turn_id,
            role,
            parent_agent_id,
            root_agent_id,
            model,
            thinking,
            thread_id,
            work_mode,
            report_to,
            spawned_by_run_id,
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                session.agents.insert(
                    agent_id.clone(),
                    AgentRecord {
                        id: agent_id,
                        session_id,
                        turn_id,
                        role,
                        parent_agent_id,
                        root_agent_id,
                        model,
                        thinking,
                        thread_id,
                        work_mode,
                        status: AgentStatus::Created,
                        report_to,
                        spawned_by_run_id,
                        report_message_id: None,
                        steps: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        created_at: record.ts.clone(),
                        updated_at: record.ts.clone(),
                    },
                );
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::AgentStatusChanged {
            agent_id,
            session_id,
            status,
            note: _,
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                if let Some(agent) = session.agents.get_mut(&agent_id) {
                    agent.status = status;
                    agent.updated_at = record.ts.clone();
                }
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::AgentReported {
            agent_id,
            session_id,
            status,
            report_message_id,
            ..
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                if let Some(agent) = session.agents.get_mut(&agent_id) {
                    agent.status = status;
                    agent.report_message_id = Some(report_message_id);
                    agent.updated_at = record.ts.clone();
                }
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::RunScheduled {
            run_id,
            session_id,
            turn_id,
            agent_id,
            thread_id,
            parent_run_id,
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                if !session.runs.contains_key(&run_id) {
                    session.run_order.push(run_id.clone());
                }
                session.runs.insert(
                    run_id.clone(),
                    RunRecord {
                        id: run_id,
                        turn_id,
                        state: "Scheduled".to_string(),
                        steps: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        pending_approval_id: None,
                        error: None,
                        stop_confirmation: None,
                        started_at: None,
                        updated_at: record.ts.clone(),
                        agent_id,
                        thread_id: thread_id.unwrap_or_else(|| MAIN_THREAD.to_string()),
                        parent_run_id,
                    },
                );
                session.updated_at = record.ts.clone();
            }
        }
        DomainEvent::RunStarted { run_id } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "Running".to_string();
                run.started_at = Some(record.ts.clone());
            });
            sync_agent_status(projection, &run_id, AgentStatus::Running, &record.ts);
        }
        DomainEvent::RunResumed { run_id, .. } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "Running".to_string();
                run.pending_approval_id = None;
            });
            sync_agent_status(projection, &run_id, AgentStatus::Running, &record.ts);
        }
        DomainEvent::StepCompleted { run_id, .. } => {
            update_run(projection, record, &run_id, |_| {});
        }
        DomainEvent::StepStarted { run_id, index, .. } => {
            update_run(projection, record, &run_id, |run| {
                run.steps = index;
                if run.state == "Scheduled" {
                    run.state = "Running".to_string();
                }
            });
            sync_agent_progress(projection, &run_id, &record.ts);
            sync_agent_status(projection, &run_id, AgentStatus::Running, &record.ts);
        }
        DomainEvent::RunWaitingApproval {
            run_id,
            approval_id,
        } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "WaitingApproval".to_string();
                run.pending_approval_id = Some(approval_id);
            });
            sync_agent_status(
                projection,
                &run_id,
                AgentStatus::WaitingApproval,
                &record.ts,
            );
        }
        DomainEvent::ModelCallReturned {
            run_id,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            ..
        } => {
            update_run(projection, record, &run_id, |run| {
                run.input_tokens += input_tokens;
                run.output_tokens += output_tokens;
                run.cache_read_tokens += cache_read_tokens;
                run.cache_write_tokens += cache_write_tokens;
            });
            add_agent_tokens(projection, &run_id, input_tokens, output_tokens, &record.ts);
        }
        DomainEvent::RunSucceeded { run_id } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "Succeeded".to_string();
                run.pending_approval_id = None;
            });
            sync_agent_status(projection, &run_id, AgentStatus::Succeeded, &record.ts);
        }
        DomainEvent::RunFailed { run_id, error } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "Failed".to_string();
                run.error = Some(error);
                run.pending_approval_id = None;
            });
            sync_agent_status(projection, &run_id, AgentStatus::Failed, &record.ts);
        }
        DomainEvent::RunAborted {
            run_id,
            reason,
            stop_confirmation,
        } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "Aborted".to_string();
                run.error = Some(reason);
                run.stop_confirmation = Some(stop_confirmation);
                run.pending_approval_id = None;
            });
            sync_agent_status(projection, &run_id, AgentStatus::Cancelled, &record.ts);
        }
        DomainEvent::RunAwaitingReconciliation { run_id, note } => {
            update_run(projection, record, &run_id, |run| {
                run.state = "AwaitingReconciliation".to_string();
                run.error = Some(note);
                run.pending_approval_id = None;
            });
            sync_agent_status(projection, &run_id, AgentStatus::Failed, &record.ts);
        }
        DomainEvent::RunInterrupted {
            run_id,
            session_id,
            note,
        } => {
            if let Some(session) = projection.sessions.get_mut(&session_id) {
                if let Some(run) = session.runs.get_mut(&run_id) {
                    run.state = "AwaitingReconciliation".to_string();
                    run.error = Some(note);
                    run.pending_approval_id = None;
                    run.updated_at = record.ts.clone();
                }
                session.updated_at = record.ts.clone();
            }
            sync_agent_status(projection, &run_id, AgentStatus::Interrupted, &record.ts);
        }
        _ => {
            // Tool/model/approval leaf events only bump activity timestamps.
            if let Some(session) = projection.sessions.get_mut(&record.aggregate_id) {
                session.updated_at = record.ts.clone();
            }
        }
    }
}

fn update_run(
    projection: &mut Projection,
    record: &EventRecord,
    run_id: &str,
    mutate: impl FnOnce(&mut RunRecord),
) {
    if let Some(session) = projection.sessions.get_mut(&record.aggregate_id) {
        if let Some(run) = session.runs.get_mut(run_id) {
            mutate(run);
            run.updated_at = record.ts.clone();
        }
        session.updated_at = record.ts.clone();
    }
}

fn sync_agent_status(projection: &mut Projection, run_id: &str, status: AgentStatus, ts: &str) {
    for session in projection.sessions.values_mut() {
        let Some(run) = session.runs.get(run_id) else {
            continue;
        };
        let Some(agent_id) = run.agent_id.clone() else {
            return;
        };
        if let Some(agent) = session.agents.get_mut(&agent_id) {
            agent.status = status;
            agent.updated_at = ts.to_string();
        }
        return;
    }
}

fn sync_agent_progress(projection: &mut Projection, run_id: &str, ts: &str) {
    for session in projection.sessions.values_mut() {
        let Some(run) = session.runs.get(run_id) else {
            continue;
        };
        let Some(agent_id) = run.agent_id.clone() else {
            return;
        };
        let steps = run.steps;
        if let Some(agent) = session.agents.get_mut(&agent_id) {
            agent.steps = agent.steps.max(steps);
            agent.updated_at = ts.to_string();
        }
        return;
    }
}

fn add_agent_tokens(
    projection: &mut Projection,
    run_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    ts: &str,
) {
    for session in projection.sessions.values_mut() {
        let Some(run) = session.runs.get(run_id) else {
            continue;
        };
        let Some(agent_id) = run.agent_id.clone() else {
            return;
        };
        if let Some(agent) = session.agents.get_mut(&agent_id) {
            agent.input_tokens += input_tokens;
            agent.output_tokens += output_tokens;
            agent.updated_at = ts.to_string();
        }
        return;
    }
}

fn agent_view(agent: &AgentRecord) -> AgentView {
    AgentView {
        id: agent.id.clone(),
        session_id: agent.session_id.clone(),
        turn_id: agent.turn_id.clone(),
        role: agent.role,
        parent_agent_id: agent.parent_agent_id.clone(),
        root_agent_id: agent.root_agent_id.clone(),
        model: agent.model.clone(),
        thinking: agent.thinking.clone(),
        thread_id: agent.thread_id.clone(),
        work_mode: agent.work_mode,
        status: agent.status,
        report_to: agent.report_to.clone(),
        spawned_by_run_id: agent.spawned_by_run_id.clone(),
        report_message_id: agent.report_message_id.clone(),
        steps: agent.steps,
        input_tokens: agent.input_tokens,
        output_tokens: agent.output_tokens,
        created_at: agent.created_at.clone(),
        updated_at: agent.updated_at.clone(),
    }
}

fn run_view(r: &RunRecord) -> RunView {
    RunView {
        id: r.id.clone(),
        turn_id: r.turn_id.clone(),
        state: r.state.clone(),
        steps: r.steps,
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cache_read_tokens: r.cache_read_tokens,
        cache_write_tokens: r.cache_write_tokens,
        pending_approval_id: r.pending_approval_id.clone(),
        error: r.error.clone(),
        stop_confirmation: r.stop_confirmation.clone(),
        started_at: r.started_at.clone(),
        updated_at: r.updated_at.clone(),
        agent_id: r.agent_id.clone(),
        thread_id: Some(r.thread_id.clone()),
        parent_run_id: r.parent_run_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phi_adapters::fs_blob::FsBlobStore;
    use phi_adapters::fs_journal::JsonlJournal;
    use phi_adapters::system::{StderrLog, SystemClock, UuidIds};
    use serde_json::json;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("phi-test-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn folds_session_and_run_state() {
        let dir = temp_dir("session");
        let bus = Arc::new(EventBus::new(
            Arc::new(JsonlJournal::new(dir.join("events.jsonl"))),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        ));
        let plugin = Arc::new(SessionPlugin::new(
            bus.clone(),
            Arc::new(UuidIds),
            Arc::new(FsBlobStore::new(dir.join("blobs"))),
            "node-test".to_string(),
        ));
        bus.subscribe(plugin.clone());

        let session = plugin.create_session("t").unwrap();
        let turn = plugin.start_turn(&session.id).unwrap();
        plugin
            .append_message(
                &session.id,
                Some(&turn.turn_id),
                Role::User,
                vec![ContentBlock::text("hi")],
            )
            .unwrap();
        bus.publish(
            &session.id,
            "test",
            &json!({
                "type": "RunScheduled",
                "runId": "run-1",
                "sessionId": session.id,
                "turnId": turn.turn_id
            }),
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &json!({ "type": "RunSucceeded", "runId": "run-1" }),
        )
        .unwrap();

        let view = plugin.get_session(&session.id).unwrap();
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].blocks.len(), 1);
        assert_eq!(view.runs[0].state, "Succeeded");
    }

    #[test]
    fn repairs_unpaired_tool_use_for_a_terminal_run() {
        let dir = temp_dir("session-repair-terminal");
        let bus = Arc::new(EventBus::new(
            Arc::new(JsonlJournal::new(dir.join("events.jsonl"))),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        ));
        let plugin = Arc::new(SessionPlugin::new(
            bus.clone(),
            Arc::new(UuidIds),
            Arc::new(FsBlobStore::new(dir.join("blobs"))),
            "node-test".to_string(),
        ));
        bus.subscribe(plugin.clone());

        let session = plugin.create_session("t").unwrap();
        let turn = plugin.start_turn(&session.id).unwrap();
        bus.publish(
            &session.id,
            "test",
            &json!({
                "type": "RunScheduled",
                "runId": "run-1",
                "sessionId": session.id,
                "turnId": turn.turn_id
            }),
        )
        .unwrap();
        plugin
            .append_message(
                &session.id,
                None,
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call-1".to_string(),
                    name: "fs_list".to_string(),
                    input: json!({ "path": "." }),
                }],
            )
            .unwrap();
        // The result append failed, so the run is already terminal. Startup
        // recovery must still repair the dangling tool_use.
        bus.publish(
            &session.id,
            "test",
            &json!({ "type": "RunFailed", "runId": "run-1", "error": "journal write failed" }),
        )
        .unwrap();

        assert_eq!(plugin.recover_interrupted(), 0);
        let view = plugin.get_session(&session.id).unwrap();
        let last = view.messages.last().unwrap();
        match &last.blocks[0] {
            ContentBlock::ToolResult { tool_use_id, .. } => {
                assert_eq!(tool_use_id, "call-1")
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn repairs_unpaired_tool_use_after_interruption() {
        let dir = temp_dir("session-repair");
        let bus = Arc::new(EventBus::new(
            Arc::new(JsonlJournal::new(dir.join("events.jsonl"))),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        ));
        let plugin = Arc::new(SessionPlugin::new(
            bus.clone(),
            Arc::new(UuidIds),
            Arc::new(FsBlobStore::new(dir.join("blobs"))),
            "node-test".to_string(),
        ));
        bus.subscribe(plugin.clone());

        let session = plugin.create_session("t").unwrap();
        // Crash window: the assistant message was saved, its tool results were
        // not. The next model call would otherwise be rejected by the provider.
        plugin
            .append_message(
                &session.id,
                None,
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call-1".to_string(),
                    name: "fs_list".to_string(),
                    input: json!({ "path": "." }),
                }],
            )
            .unwrap();

        assert_eq!(plugin.repair_unpaired_tool_uses(&session.id).unwrap(), 1);
        let view = plugin.get_session(&session.id).unwrap();
        let last = view.messages.last().unwrap();
        assert_eq!(last.role, Role::User);
        match &last.blocks[0] {
            ContentBlock::ToolResult { tool_use_id, .. } => {
                assert_eq!(tool_use_id, "call-1")
            }
            other => panic!("expected tool_result, got {other:?}"),
        }

        // Repair must be idempotent across subsequent restarts.
        assert_eq!(plugin.repair_unpaired_tool_uses(&session.id).unwrap(), 0);
    }

    #[test]
    fn projects_agent_threads_and_write_lease() {
        let dir = temp_dir("session-agents");
        let bus = Arc::new(EventBus::new(
            Arc::new(JsonlJournal::new(dir.join("events.jsonl"))),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        ));
        let plugin = Arc::new(SessionPlugin::new(
            bus.clone(),
            Arc::new(UuidIds),
            Arc::new(FsBlobStore::new(dir.join("blobs"))),
            "node-test".to_string(),
        ));
        bus.subscribe(plugin.clone());

        let session = plugin.create_session("t").unwrap();
        let turn = plugin.start_turn(&session.id).unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::AgentCreated {
                agent_id: "agent-lead".to_string(),
                session_id: session.id.clone(),
                turn_id: turn.turn_id.clone(),
                role: AgentRole::Lead,
                parent_agent_id: None,
                root_agent_id: "agent-lead".to_string(),
                model: "lead-model".to_string(),
                thinking: None,
                thread_id: "main".to_string(),
                work_mode: WorkMode::Shared,
                report_to: None,
                spawned_by_run_id: "run-root".to_string(),
            },
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunScheduled {
                run_id: "run-root".to_string(),
                session_id: session.id.clone(),
                turn_id: turn.turn_id.clone(),
                agent_id: Some("agent-lead".to_string()),
                thread_id: Some("main".to_string()),
                parent_run_id: None,
            },
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunStarted {
                run_id: "run-root".to_string(),
            },
        )
        .unwrap();
        plugin
            .append_message_in_thread(
                &session.id,
                Some(&turn.turn_id),
                "main",
                Some("agent-lead"),
                None,
                Some(MessageKind::User),
                Role::User,
                vec![ContentBlock::text("lead task")],
            )
            .unwrap();

        bus.publish(
            &session.id,
            "test",
            &DomainEvent::AgentCreated {
                agent_id: "agent-child".to_string(),
                session_id: session.id.clone(),
                turn_id: turn.turn_id.clone(),
                role: AgentRole::Subagent,
                parent_agent_id: Some("agent-lead".to_string()),
                root_agent_id: "agent-lead".to_string(),
                model: "small-model".to_string(),
                thinking: None,
                thread_id: "agent:agent-child".to_string(),
                work_mode: WorkMode::Shared,
                report_to: Some("agent-lead".to_string()),
                spawned_by_run_id: "run-root".to_string(),
            },
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunScheduled {
                run_id: "run-child".to_string(),
                session_id: session.id.clone(),
                turn_id: turn.turn_id.clone(),
                agent_id: Some("agent-child".to_string()),
                thread_id: Some("agent:agent-child".to_string()),
                parent_run_id: Some("run-root".to_string()),
            },
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunStarted {
                run_id: "run-child".to_string(),
            },
        )
        .unwrap();
        plugin
            .append_message_in_thread(
                &session.id,
                Some(&turn.turn_id),
                "agent:agent-child",
                Some("agent-lead"),
                Some("agent-child"),
                Some(MessageKind::Brief),
                Role::User,
                vec![ContentBlock::text("child brief")],
            )
            .unwrap();
        assert_eq!(
            plugin.write_lease_holder(&session.id).as_deref(),
            Some("agent-child")
        );
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::ModelCallReturned {
                call_id: "call-1".to_string(),
                run_id: "run-child".to_string(),
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 2,
                cache_write_tokens: 1,
                finish_reason: "end_turn".to_string(),
            },
        )
        .unwrap();
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunSucceeded {
                run_id: "run-child".to_string(),
            },
        )
        .unwrap();

        let view = plugin.get_session(&session.id).unwrap();
        assert_eq!(view.messages.len(), 1, "main thread only");
        assert_eq!(view.agents.len(), 2);
        let child = view
            .agents
            .iter()
            .find(|agent| agent.id == "agent-child")
            .unwrap();
        assert_eq!(child.status, AgentStatus::Succeeded);
        assert_eq!(child.input_tokens, 10);
        assert_eq!(child.output_tokens, 5);
        assert_eq!(
            plugin
                .messages_for_thread(&session.id, "agent:agent-child")
                .len(),
            1
        );
        assert_eq!(plugin.write_lease_holder(&session.id), None);
        assert_eq!(
            plugin.active_root_run(&session.id).as_deref(),
            Some("run-root")
        );
        bus.publish(
            &session.id,
            "test",
            &DomainEvent::RunSucceeded {
                run_id: "run-root".to_string(),
            },
        )
        .unwrap();
        assert_eq!(plugin.active_root_run(&session.id), None);
    }
}
