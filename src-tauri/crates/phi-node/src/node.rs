//! Shared node composition and application services.
//!
//! This crate is the shell-independent core: the desktop shell and the
//! headless CLI both compose the same `Node` and call the same methods. No
//! Tauri type may appear here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use phi_adapters::config::ConfigStore;
use phi_adapters::control;
use phi_adapters::fs_blob::FsBlobStore;
use phi_adapters::fs_journal::JsonlJournal;
use phi_adapters::lock::DataDirLock;
use phi_adapters::memory_sqlite::SqliteMemoryStore;
use phi_adapters::system::{StderrLog, SystemClock, UuidIds};
use phi_contracts::auxiliary::{
    AuxiliaryRoleInput, AuxiliaryRoleView, DecisionConfigInput, DecisionQuestion, DecisionRequest,
    DecisionResponse, DecisionStatus, ImageAsset, ImageConfigInput, ImageRequest, ImageResponse,
    ImageStatus, MaintenanceReport, SettleReport,
};
use phi_contracts::memory::{
    ForgetReport, ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryHit, MemoryListFilter,
    MemoryPort, MemoryQuery, MemoryStats, MemoryStore, Principal,
};
use phi_contracts::ports::{ModelStatus, RunStatus, SessionPort, StartRunRequest, ThinkingEffort};
use phi_contracts::provider::{ModelSpec, ProviderInput, ProviderTestView, ProviderView};
use phi_contracts::services::Services;
use phi_contracts::stream::{StreamHub, StreamPort};
use phi_contracts::{REQUIRED_SLOTS, SLOT_PERSISTENCE_JOURNAL};
use phi_domain::agent::{AgentRole, AgentView, WorkMode};
use phi_domain::memory::{Episode, MemoryRecord};
use phi_domain::message::{ChatMessage, ContentBlock, Role};
use phi_domain::view::{PendingApprovalView, SessionSummary, SessionView};
use phi_i18n as i18n;
use phi_kernel::event_bus::{EventBus, EventSink};
use phi_kernel::lifecycle::{Kernel, RunMode};
use phi_kernel::ports::LogPort;
use phi_kernel::registry::Registry;
use phi_plugins::approver::HumanApprover;
use phi_plugins::auxiliary::AuxiliaryPlugin;
use phi_plugins::context::DefaultContextPlugin;
use phi_plugins::decision::DecisionPlugin;
use phi_plugins::image::ImagePlugin;
use phi_plugins::loop_plugin::LoopPlugin;
use phi_plugins::memory::MemoryPlugin;
use phi_plugins::model_gateway::ModelGateway;
use phi_plugins::policy::DefaultPolicyPlugin;
use phi_plugins::provider_store::ProviderStore;
use phi_plugins::sandbox::FsSandboxPlugin;
use phi_plugins::session::SessionPlugin;
use phi_plugins::store::StorePlugin;
use phi_plugins::subagent::SubagentToolsPlugin;
use phi_plugins::tools::BuiltinToolsPlugin;

#[derive(Clone)]
pub struct NodeConfig {
    pub data_dir: PathBuf,
    /// Explicit workspace selection (headless `--workspace`). Wins over the
    /// persisted setting and the fallback.
    pub workspace: Option<PathBuf>,
    /// Used when no workspace has been persisted yet. The desktop shell passes
    /// `Documents/phi-workspace`; headless defaults to `<data_dir>/workspace`.
    pub fallback_workspace: Option<PathBuf>,
    /// Shell-specific event sinks (Tauri emitter, JSONL writer, test collector).
    pub event_sinks: Vec<Arc<dyn EventSink>>,
    /// Shell-specific live stream sink. Without one the gateway stays
    /// non-streaming.
    pub stream_sink: Option<Arc<dyn StreamPort>>,
    /// Per-process language override (CLI `--lang`). `None` falls back to
    /// `PHI_LANG`, the persisted setting, then the OS locale.
    pub language: Option<String>,
    /// Writable nodes take the data-directory lock and may append to the
    /// journal or persist settings. Read-only shells (for example
    /// `phi-headless status`) only project the existing log.
    pub writable: bool,
}

pub struct Node {
    node_id: String,
    bus: Arc<EventBus>,
    kernel: Arc<Kernel>,
    services: Arc<Services>,
    config: Arc<ConfigStore>,
    providers: Arc<ProviderStore>,
    model_gateway: Arc<ModelGateway>,
    data_dir: PathBuf,
    /// Workspace resolved for this process. An explicit `--workspace` wins over
    /// the persisted setting, so memory attribution must use this value rather
    /// than re-reading the config snapshot.
    workspace: PathBuf,
    /// An explicit shell selection. `None` means the workspace can still change
    /// through the settings surface while the node is running.
    explicit_workspace: Option<PathBuf>,
    writable: bool,
    // Stop the watcher before releasing the data-directory lock.
    _control_watcher: Option<ControlWatcher>,
    _data_lock: Option<DataDirLock>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub node_id: String,
    pub mode: String,
    pub mode_reason: String,
    pub model: ModelStatus,
    pub workspace: Option<String>,
    pub sessions: Vec<SessionSummary>,
    pub last_session_id: Option<String>,
    pub providers: Vec<ProviderView>,
    pub active_provider_id: Option<String>,
    pub language: String,
    /// Host platform (`windows`, `macos`, `linux`, `android`, `ios`). The
    /// desktop title bar uses it to match the window chrome.
    pub platform: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionView,
    pub pending_approvals: Vec<PendingApprovalView>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub run_id: String,
}

/// Applies approval/cancel command files written by another shell process.
struct ControlWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for ControlWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn compose(config: NodeConfig) -> Result<Arc<Node>, String> {
    // Acquire the single-writer lock before touching any persistent file.
    let data_lock = if config.writable {
        Some(DataDirLock::acquire(&config.data_dir)?)
    } else {
        None
    };

    let journal = Arc::new(JsonlJournal::new(
        config.data_dir.join("journal").join("events.jsonl"),
    ));
    let blob = Arc::new(FsBlobStore::new(config.data_dir.join("blobs")));
    let clock = Arc::new(SystemClock::new());
    let ids = Arc::new(UuidIds);
    let log = Arc::new(StderrLog);
    let app_config = Arc::new(ConfigStore::load(config.data_dir.join("config.json")));

    let configured_language = app_config.snapshot().language;
    let language = i18n::resolve(config.language.as_deref(), configured_language.as_deref());
    i18n::set_lang(language);
    if config.writable
        && configured_language.is_none()
        && config.language.is_none()
        && std::env::var("PHI_LANG").is_err()
    {
        let code = language.code().to_string();
        app_config.update(|config| config.language = Some(code))?;
    }

    let node_id = if config.writable {
        app_config.update(|config| {
            if config.node_id.is_none() {
                config.node_id = Some(format!("node_{}", uuid::Uuid::new_v4().simple()));
            }
            config.node_id.clone().unwrap_or_default()
        })?
    } else {
        app_config
            .snapshot()
            .node_id
            .unwrap_or_else(|| "node_readonly".to_string())
    };

    let workspace = resolve_workspace(&config, &app_config, &log)?;
    log.info(&format!("workspace: {}", workspace.display()));

    let providers = Arc::new(ProviderStore::load(config.data_dir.join("providers.json")));
    if config.writable {
        if let Err(error) = providers.seed_from_env() {
            log.warn(&i18n::t_args(
                "Failed to import provider environment variables: {error}",
                &[("error", error)],
            ));
        }
    }

    let bus = Arc::new(EventBus::new(
        journal.clone(),
        clock.clone(),
        ids.clone(),
        log.clone(),
    ));
    if !config.writable {
        bus.set_read_only();
    }
    let records = bus.restore()?;

    let session = Arc::new(SessionPlugin::new(
        bus.clone(),
        ids.clone(),
        blob.clone(),
        node_id.clone(),
    ));
    session.load(&records);

    // Projections must observe events before any live publishing happens.
    bus.subscribe(session.clone());
    for sink in config.event_sinks {
        bus.subscribe(sink);
    }

    let streams = Arc::new(StreamHub::new(config.stream_sink));
    let sandbox = Arc::new(FsSandboxPlugin);
    let model_gateway = Arc::new(ModelGateway::new(providers.clone(), streams));
    let policy = Arc::new(DefaultPolicyPlugin::new(sandbox.clone()));
    let approver = Arc::new(HumanApprover::new(bus.clone(), ids.clone(), clock.clone()));
    let tools = Arc::new(BuiltinToolsPlugin::new(
        sandbox.clone(),
        config.data_dir.join("backups"),
    ));
    let context = Arc::new(DefaultContextPlugin::new(sandbox.clone()));

    // Memory is optional: if the database cannot be opened the node still
    // starts and retrieval degrades to "no memories" instead of pretending.
    let memory_path = memory_database_path(&config.data_dir);
    let memory_store: Option<Arc<dyn MemoryStore>> = if config.writable {
        match SqliteMemoryStore::open(&memory_path, true) {
            Ok(store) => Some(Arc::new(store)),
            Err(error) => {
                log.warn(&format!("memory is unavailable: {error}"));
                None
            }
        }
    } else if memory_path.exists() {
        match SqliteMemoryStore::open(&memory_path, false) {
            Ok(store) => Some(Arc::new(store)),
            Err(error) => {
                log.warn(&format!("memory is unavailable: {error}"));
                None
            }
        }
    } else {
        None
    };
    let memory = memory_store.map(|store| {
        Arc::new(MemoryPlugin::new(
            store,
            clock.clone(),
            ids.clone(),
            bus.clone(),
            log.clone(),
            node_id.clone(),
        ))
    });

    let services = Arc::new(Services::new());
    services.set_session(session.clone());
    services.set_model(model_gateway.clone());
    services.set_policy(policy.clone());
    services.set_sandbox(sandbox.clone());
    services.set_approver(approver.clone());
    services.set_context(context.clone());
    services.add_tool_provider(tools.clone());
    if let Some(memory) = &memory {
        services.set_memory(memory.clone());
        services.add_tool_provider(memory.clone());
    }

    let runtime = Arc::new(LoopPlugin::new(
        bus.clone(),
        ids.clone(),
        log.clone(),
        blob.clone(),
        services.clone(),
    ));
    services.set_runtime(runtime.clone());

    let subagent = Arc::new(SubagentToolsPlugin::new(
        services.clone(),
        bus.clone(),
        ids.clone(),
        log.clone(),
    ));
    services.add_tool_provider(subagent.clone());

    let image = Arc::new(ImagePlugin::new(
        config.data_dir.join("image.json"),
        config.data_dir.join("assets"),
        bus.clone(),
        clock.clone(),
        log.clone(),
    ));
    services.set_image(image.clone());
    services.add_tool_provider(image.clone());

    let decision = Arc::new(DecisionPlugin::new(
        config.data_dir.join("decision.json"),
        bus.clone(),
        ids.clone(),
        log.clone(),
    ));
    services.set_decision(decision.clone());

    let auxiliary = Arc::new(AuxiliaryPlugin::new(
        services.clone(),
        config.data_dir.join("auxiliary.json"),
        config.data_dir.join("memory").join("maintenance.json"),
        bus.clone(),
        ids.clone(),
        clock.clone(),
        log.clone(),
    ));
    services.set_auxiliary(auxiliary.clone());

    let registry = Arc::new(Registry::new());
    let kernel = Arc::new(Kernel::new(bus.clone(), registry.clone(), log.clone()));
    kernel.add_plugin(Arc::new(StorePlugin::new(journal.clone(), blob.clone())));
    kernel.add_plugin(sandbox.clone());
    kernel.add_plugin(model_gateway.clone());
    kernel.add_plugin(policy.clone());
    kernel.add_plugin(approver.clone());
    kernel.add_plugin(session.clone());
    kernel.add_plugin(tools.clone());
    kernel.add_plugin(context.clone());
    kernel.add_plugin(runtime.clone());
    kernel.add_plugin(subagent.clone());
    kernel.add_plugin(image.clone());
    kernel.add_plugin(decision.clone());
    kernel.add_plugin(auxiliary.clone());
    if let Some(memory) = &memory {
        kernel.add_plugin(memory.clone());
    }

    if config.writable {
        kernel.activate_all();
        // Slot metadata and typed service wiring are two views of the same
        // composition; both must agree before the node reports Normal.
        let mut missing = services.missing_slots(REQUIRED_SLOTS);
        missing.extend(registry.missing_slots(&[SLOT_PERSISTENCE_JOURNAL]));
        if missing.is_empty() {
            kernel.set_mode(
                RunMode::Normal,
                &i18n::t("All required slots are available"),
            );
        } else {
            kernel.set_mode(
                RunMode::Maintenance,
                &i18n::t_args(
                    "Required services are missing: {slots}",
                    &[("slots", missing.join(", "))],
                ),
            );
        }
    } else {
        // Read-only shells never activate plugins: activation probes storage,
        // and a probe must not write while another process owns the journal.
        kernel.set_initial_mode(RunMode::Normal, &i18n::t("Read-only shell"));
    }

    if config.writable {
        let interrupted = session.recover_interrupted();
        if interrupted > 0 {
            log.warn(&format!(
                "{interrupted} interrupted run(s) marked AwaitingReconciliation"
            ));
        }
        control::clean_stale(&config.data_dir, Duration::from_secs(24 * 60 * 60));
    }

    let control_watcher = if config.writable {
        Some(spawn_control_watcher(
            services.clone(),
            config.data_dir.clone(),
        ))
    } else {
        None
    };

    Ok(Arc::new(Node {
        node_id,
        bus,
        kernel,
        services,
        config: app_config,
        providers,
        model_gateway,
        data_dir: config.data_dir,
        workspace,
        explicit_workspace: config.workspace.clone(),
        writable: config.writable,
        _data_lock: data_lock,
        _control_watcher: control_watcher,
    }))
}

fn spawn_control_watcher(services: Arc<Services>, data_dir: PathBuf) -> ControlWatcher {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stop_for_thread.load(Ordering::Relaxed) {
            for command in control::take_cancels(&data_dir) {
                if let Some(runtime) = services.runtime() {
                    runtime.cancel(&command.run_id, &i18n::t("Cancelled by user"));
                }
            }
            for command in control::take_approvals(&data_dir) {
                if let Some(approver) = services.approver() {
                    approver.resolve(&command.approval_id, command.approved, command.note);
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    ControlWatcher {
        stop,
        handle: Some(handle),
    }
}

fn resolve_workspace(
    config: &NodeConfig,
    app_config: &ConfigStore,
    log: &Arc<StderrLog>,
) -> Result<PathBuf, String> {
    // 1. An explicit shell selection always wins.
    if let Some(path) = &config.workspace {
        let workspace = path.clone();
        persist_workspace(config, app_config, &workspace)?;
        log.info(&format!("workspace (explicit): {}", workspace.display()));
        return Ok(workspace);
    }
    // 2. The saved setting, when it still exists.
    if let Some(path) = app_config.snapshot().workspace {
        let candidate = PathBuf::from(&path);
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    // 3. Shell default, then the data-directory default.
    let workspace = match &config.fallback_workspace {
        Some(path) => path.clone(),
        None => config.data_dir.join("workspace"),
    };
    persist_workspace(config, app_config, &workspace)?;
    if config.writable {
        log.info(&format!(
            "default workspace created: {}",
            workspace.display()
        ));
    }
    Ok(workspace)
}

fn persist_workspace(
    config: &NodeConfig,
    app_config: &ConfigStore,
    workspace: &Path,
) -> Result<(), String> {
    if !config.writable {
        return Ok(());
    }
    std::fs::create_dir_all(workspace).map_err(|error| {
        i18n::t_args(
            "Failed to create workspace: {error}",
            &[("error", error.to_string())],
        )
    })?;
    let display = workspace.to_string_lossy().to_string();
    app_config.update(|config| config.workspace = Some(display))?;
    Ok(())
}

fn memory_database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("memory").join("memory.db")
}

impl Node {
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn writable(&self) -> bool {
        self.writable
    }

    pub fn bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    fn require_writable(&self) -> Result<(), String> {
        if self.writable {
            Ok(())
        } else {
            Err(i18n::t("The node is read-only"))
        }
    }

    fn session_port(&self) -> Result<Arc<dyn SessionPort>, String> {
        self.services
            .session()
            .ok_or_else(|| i18n::t("Session store is not available"))
    }

    pub fn mode(&self) -> RunMode {
        self.kernel.mode()
    }

    pub fn mode_reason(&self) -> String {
        self.kernel.mode_reason()
    }

    pub fn bootstrap(&self) -> Result<Bootstrap, String> {
        let mut sessions = self.list_sessions();
        if sessions.is_empty() && self.writable {
            self.create_session(Some(&i18n::t("New session")))?;
            sessions = self.list_sessions();
        }
        let model = self.model_status();
        Ok(Bootstrap {
            node_id: self.node_id.clone(),
            mode: self.kernel.mode().as_str().to_string(),
            mode_reason: self.kernel.mode_reason(),
            model,
            workspace: self.config.snapshot().workspace,
            last_session_id: sessions.first().map(|session| session.id.clone()),
            sessions,
            providers: self.provider_views(),
            active_provider_id: self.active_provider_id(),
            language: i18n::current().code().to_string(),
            platform: std::env::consts::OS.to_string(),
        })
    }

    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        self.services
            .session()
            .map(|session| session.list_sessions())
            .unwrap_or_default()
    }

    pub fn session_view(&self, session_id: &str) -> Option<SessionView> {
        self.services
            .session()
            .and_then(|session| session.get_session(session_id))
    }

    pub fn agents(&self, session_id: &str) -> Vec<AgentView> {
        self.services
            .session()
            .map(|session| session.agents(session_id))
            .unwrap_or_default()
    }

    pub fn agent(&self, agent_id: &str) -> Option<AgentView> {
        self.services
            .session()
            .and_then(|session| session.agent(agent_id))
    }

    pub fn agent_messages(&self, agent_id: &str) -> Option<Vec<ChatMessage>> {
        let session = self.services.session()?;
        let agent = session.agent(agent_id)?;
        Some(session.messages_for_thread(&agent.session_id, &agent.thread_id))
    }

    pub fn pending_approvals(&self, session_id: &str) -> Vec<PendingApprovalView> {
        self.services
            .approver()
            .map(|approver| approver.pending_for_session(session_id))
            .unwrap_or_default()
    }

    pub fn provider_views(&self) -> Vec<ProviderView> {
        self.providers.views()
    }

    pub fn provider_view(&self, id: &str) -> Option<ProviderView> {
        self.providers.view(id)
    }

    pub fn active_provider_id(&self) -> Option<String> {
        self.providers.active_id()
    }

    pub fn set_language(&self, code: &str) -> Result<String, String> {
        self.require_writable()?;
        let lang = i18n::Lang::parse(code).ok_or_else(|| {
            i18n::t_args(
                "Unknown language {lang}; expected en or zh-CN",
                &[("lang", code.to_string())],
            )
        })?;
        i18n::set_lang(lang);
        let code = lang.code().to_string();
        self.config
            .update(|config| config.language = Some(code.clone()))?;
        Ok(code)
    }

    pub fn set_workspace(&self, path: &str) -> Result<String, String> {
        self.require_writable()?;
        let canonical = PathBuf::from(path.trim()).canonicalize().map_err(|error| {
            i18n::t_args(
                "Path is not available: {error}",
                &[("error", error.to_string())],
            )
        })?;
        if !canonical.is_dir() {
            return Err(i18n::t("Workspace must be a directory"));
        }
        let display = canonical.to_string_lossy().to_string();
        self.config
            .update(|config| config.workspace = Some(display.clone()))?;
        Ok(display)
    }

    pub fn create_session(&self, title: Option<&str>) -> Result<SessionSummary, String> {
        self.require_writable()?;
        self.session_port()?
            .create_session(title.unwrap_or(&i18n::t("New session")))
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool, String> {
        self.require_writable()?;
        self.session_port()?.delete_session(session_id)
    }

    pub fn get_session(&self, session_id: &str) -> Result<SessionDetail, String> {
        let session = self.session_view(session_id).ok_or_else(|| {
            i18n::t_args("Unknown session: {id}", &[("id", session_id.to_string())])
        })?;
        let pending_approvals = self.pending_approvals(session_id);
        Ok(SessionDetail {
            session,
            pending_approvals,
        })
    }

    pub fn send_message(&self, session_id: &str, text: &str) -> Result<String, String> {
        self.send_message_with_provider(session_id, text, None)
    }

    pub fn send_message_with_provider(
        &self,
        session_id: &str,
        text: &str,
        provider_id: Option<&str>,
    ) -> Result<String, String> {
        self.send_message_full(session_id, text, provider_id, None, None)
    }

    pub fn send_message_full(
        &self,
        session_id: &str,
        text: &str,
        provider_id: Option<&str>,
        model: Option<&str>,
        thinking: Option<&str>,
    ) -> Result<String, String> {
        self.require_writable()?;
        if self.kernel.mode() != RunMode::Normal {
            return Err(i18n::t_args(
                "Node is in {mode} mode: {reason}",
                &[
                    ("mode", self.kernel.mode().as_str().to_string()),
                    ("reason", self.kernel.mode_reason()),
                ],
            ));
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(i18n::t("Message is empty"));
        }
        let model = model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let thinking = match thinking {
            Some(value) if !value.trim().is_empty() => {
                Some(ThinkingEffort::parse(value).ok_or_else(|| {
                    i18n::t_args(
                        "Unknown thinking effort {effort}; expected off, low, medium, high, xhigh or max",
                        &[("effort", value.trim().to_string())],
                    )
                })?)
            }
            _ => None,
        };
        let workspace = self
            .config
            .snapshot()
            .workspace
            .ok_or_else(|| i18n::t("Set a workspace first"))?;
        let workspace = PathBuf::from(workspace);
        if !workspace.is_dir() {
            return Err(i18n::t("Workspace does not exist; set it again"));
        }
        let session = self.session_port()?;
        if let Some(active) = session.active_run(session_id) {
            return Err(i18n::t_args(
                "This session still has a running task: {run_id}",
                &[("run_id", active)],
            ));
        }
        let runtime = self
            .services
            .runtime()
            .ok_or_else(|| i18n::t("Runtime is not available"))?;
        if let Some(provider_id) = provider_id {
            self.providers.set_active(provider_id)?;
        }
        let turn = session.start_turn(session_id)?;
        session.append_message(
            session_id,
            Some(&turn.turn_id),
            Role::User,
            vec![ContentBlock::text(text)],
        )?;
        runtime.start_run(StartRunRequest {
            session_id: session_id.to_string(),
            turn_id: turn.turn_id,
            workspace,
            model,
            thinking,
            // A user turn starts the lead run; the runtime assigns the lead
            // agent id and thread when the session has no agent yet.
            parent_run_id: None,
            agent_id: None,
            thread_id: None,
            root_agent_id: None,
            role: AgentRole::Lead,
            report_to: None,
            work_mode: WorkMode::Shared,
            lead_scale: None,
        })
    }

    pub fn approve(
        &self,
        approval_id: &str,
        approved: bool,
        note: Option<String>,
    ) -> Result<(), String> {
        self.require_writable()?;
        let approver = self
            .services
            .approver()
            .ok_or_else(|| i18n::t("Approval component is unavailable"))?;
        if approver.resolve(approval_id, approved, note) {
            Ok(())
        } else {
            Err(i18n::t("Approval does not exist or was already handled"))
        }
    }

    /// Cancel an in-process run, or queue the request for the process that
    /// owns it. Returns false only when the run is not active anywhere we can
    /// see.
    pub fn cancel_run(&self, run_id: &str) -> bool {
        if let Some(runtime) = self.services.runtime() {
            if runtime.cancel(run_id, &i18n::t("Cancelled by user")) {
                return true;
            }
        }
        if self.writable && control::request_cancel(&self.data_dir, run_id).is_ok() {
            return true;
        }
        false
    }

    pub fn image_status(&self) -> Option<ImageStatus> {
        self.services.image().map(|image| image.status())
    }

    pub fn configure_image(&self, input: ImageConfigInput) -> Result<ImageStatus, String> {
        self.require_writable()?;
        self.services
            .image()
            .ok_or_else(|| i18n::t("Image generation is not available"))?
            .configure(input)
    }

    pub async fn generate_images(
        &self,
        session_id: Option<&str>,
        request: ImageRequest,
    ) -> Result<ImageResponse, String> {
        self.require_writable()?;
        self.services
            .image()
            .ok_or_else(|| i18n::t("Image generation is not available"))?
            .generate(
                request,
                session_id.map(str::to_string),
                tokio_util::sync::CancellationToken::new(),
            )
            .await
    }

    pub fn image_assets(&self, session_id: Option<&str>) -> Vec<ImageAsset> {
        self.services
            .image()
            .map(|image| image.list(session_id))
            .unwrap_or_default()
    }

    pub fn image_asset_bytes(&self, asset_id: &str) -> Result<Vec<u8>, String> {
        self.services
            .image()
            .ok_or_else(|| i18n::t("Image generation is not available"))?
            .bytes(asset_id)
    }

    pub fn delete_image_asset(&self, asset_id: &str) -> Result<bool, String> {
        self.require_writable()?;
        self.services
            .image()
            .ok_or_else(|| i18n::t("Image generation is not available"))?
            .delete(asset_id)
    }

    pub fn decision_status(&self) -> Option<DecisionStatus> {
        self.services.decision().map(|decision| decision.status())
    }

    pub fn configure_decision(&self, input: DecisionConfigInput) -> Result<DecisionStatus, String> {
        self.require_writable()?;
        self.services
            .decision()
            .ok_or_else(|| i18n::t("The decision model is not available"))?
            .configure(input)
    }

    pub async fn test_decision(&self, state: &str) -> Result<DecisionResponse, String> {
        self.require_writable()?;
        let decision = self
            .services
            .decision()
            .ok_or_else(|| i18n::t("The decision model is not available"))?;
        let mut questions = BTreeMap::new();
        questions.insert(
            "probe".to_string(),
            DecisionQuestion::Noul {
                instructions: "Is this a safe, well-formed test decision?".to_string(),
            },
        );
        decision
            .evaluate(
                DecisionRequest {
                    point: "probe".to_string(),
                    state: state.to_string(),
                    questions,
                    session_id: None,
                    run_id: None,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await
    }

    pub fn auxiliary_roles(&self) -> Vec<AuxiliaryRoleView> {
        self.services
            .auxiliary()
            .map(|auxiliary| auxiliary.roles())
            .unwrap_or_default()
    }

    pub fn set_auxiliary_role(
        &self,
        role: &str,
        input: AuxiliaryRoleInput,
    ) -> Result<AuxiliaryRoleView, String> {
        self.require_writable()?;
        self.services
            .auxiliary()
            .ok_or_else(|| i18n::t("Auxiliary models are not available"))?
            .set_role(role, input)
    }

    pub fn clear_auxiliary_role(&self, role: &str) -> Result<bool, String> {
        self.require_writable()?;
        self.services
            .auxiliary()
            .ok_or_else(|| i18n::t("Auxiliary models are not available"))?
            .clear_role(role)
    }

    /// Run the post-turn auxiliary work: a session title when one is needed,
    /// and memory maintenance when the fact threshold is reached. Both are
    /// best-effort; failures are returned as warnings, not run failures.
    pub async fn settle_auxiliary(&self, session_id: &str) -> SettleReport {
        let mut report = SettleReport::default();
        let Some(auxiliary) = self.services.auxiliary() else {
            return report;
        };
        if !self.writable {
            report.warnings.push(i18n::t("The node is read-only"));
            return report;
        }
        let cancel = tokio_util::sync::CancellationToken::new();
        if auxiliary.title_ready() {
            match auxiliary.suggest_title(session_id, cancel.clone()).await {
                Ok(title) => report.title = title,
                Err(error) => report.warnings.push(error),
            }
        }
        let project = self.config.snapshot().workspace;
        if auxiliary.maintenance_due(project.as_deref()) {
            match auxiliary
                .maintain_memory(project, false, "threshold", cancel)
                .await
            {
                Ok(maintenance) => report.maintenance = Some(maintenance),
                Err(error) => report.warnings.push(error),
            }
        }
        report
    }

    pub async fn maintain_memory(
        &self,
        project_id: Option<&str>,
        dry_run: bool,
        trigger: &str,
    ) -> Result<MaintenanceReport, String> {
        self.require_writable()?;
        let auxiliary = self
            .services
            .auxiliary()
            .ok_or_else(|| i18n::t("Auxiliary models are not available"))?;
        auxiliary
            .maintain_memory(
                project_id.map(str::to_string),
                dry_run,
                trigger,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
    }

    pub fn model_status(&self) -> ModelStatus {
        match self.services.model() {
            Some(model) => model.status(),
            None => ModelStatus {
                provider: "none".to_string(),
                model: "none".to_string(),
                configured: false,
                provider_id: None,
                protocol: String::new(),
                base_url: None,
                has_key: false,
                context_window: None,
            },
        }
    }

    pub fn set_api_key(&self, key: &str) -> Result<ModelStatus, String> {
        self.require_writable()?;
        match self.services.model() {
            Some(model) => model.set_api_key(key),
            None => Err(i18n::t("Model gateway is not available")),
        }
    }

    pub fn run_status(&self, run_id: &str) -> Option<RunStatus> {
        if let Some(runtime) = self.services.runtime() {
            if let Some(status) = runtime.status(run_id) {
                return Some(status);
            }
        }
        // Completed runs leave the in-memory map; the journal projection is
        // the durable source (and the only source for another process).
        let session = self.services.session()?;
        let (session_id, run) = session.find_run(run_id)?;
        Some(RunStatus {
            run_id: run.id,
            session_id,
            state: run.state,
            steps: run.steps,
            agent_id: run.agent_id,
            thread_id: run.thread_id,
            parent_run_id: run.parent_run_id,
        })
    }

    pub fn save_provider(&self, input: ProviderInput) -> Result<ProviderView, String> {
        self.require_writable()?;
        self.providers.upsert(input)
    }

    pub fn remove_provider(&self, id: &str) -> Result<bool, String> {
        self.require_writable()?;
        self.providers.remove(id)
    }

    pub fn set_active_provider(&self, id: &str) -> Result<(), String> {
        self.require_writable()?;
        self.providers.set_active(id)
    }

    /// Best-effort catalogue metadata for model ids. Pure: no I/O, no lock.
    pub fn describe_models(&self, ids: &[String]) -> Vec<ModelSpec> {
        phi_plugins::model_catalog::describe_many(ids)
    }

    // ── memory ────────────────────────────────────────────────────────

    fn principal(&self) -> Principal {
        let project = self
            .explicit_workspace
            .clone()
            .or_else(|| self.config.snapshot().workspace.map(PathBuf::from))
            .unwrap_or_else(|| self.workspace.clone());
        Principal {
            user_id: self.node_id.clone(),
            project_id: Some(project.to_string_lossy().to_string()),
            agent_id: Some("main".to_string()),
            run_id: None,
        }
    }

    fn memory_port(&self) -> Result<Arc<dyn MemoryPort>, String> {
        self.services
            .memory()
            .ok_or_else(|| i18n::t("Memory is not available"))
    }

    /// Read paths degrade to an empty memory when the data directory has never
    /// created a memory database. Writes still fail honestly.
    fn memory_read_port(&self) -> Result<Option<Arc<dyn MemoryPort>>, String> {
        if let Some(port) = self.services.memory() {
            return Ok(Some(port));
        }
        if !self.writable && !memory_database_path(&self.data_dir).exists() {
            return Ok(None);
        }
        Err(i18n::t("Memory is not available"))
    }

    pub fn memory_add(&self, request: MemoryAddRequest) -> Result<MemoryAddOutcome, String> {
        self.require_writable()?;
        self.memory_port()?.add(&self.principal(), request)
    }

    pub fn memory_search(&self, query: MemoryQuery) -> Result<Vec<MemoryHit>, String> {
        match self.memory_read_port()? {
            Some(port) => port.search(&self.principal(), &query),
            None => Ok(Vec::new()),
        }
    }

    pub fn memory_list(&self, filter: MemoryListFilter) -> Result<Vec<MemoryRecord>, String> {
        match self.memory_read_port()? {
            Some(port) => port.list(&self.principal(), &filter),
            None => Ok(Vec::new()),
        }
    }

    pub fn memory_get(&self, id: &str) -> Result<Option<MemoryRecord>, String> {
        match self.memory_read_port()? {
            Some(port) => port.get(&self.principal(), id),
            None => Ok(None),
        }
    }

    pub fn memory_episode(&self, id: &str) -> Result<Option<Episode>, String> {
        match self.memory_read_port()? {
            Some(port) => port.episode(&self.principal(), id),
            None => Ok(None),
        }
    }

    pub fn memory_invalidate(&self, id: &str, reason: &str) -> Result<(), String> {
        self.require_writable()?;
        self.memory_port()?
            .invalidate(&self.principal(), id, reason)
    }

    pub fn memory_supersede(
        &self,
        old_id: &str,
        request: MemoryAddRequest,
        reason: &str,
    ) -> Result<MemoryAddOutcome, String> {
        self.require_writable()?;
        self.memory_port()?
            .supersede(&self.principal(), old_id, request, reason)
    }

    pub fn memory_correct(
        &self,
        id: &str,
        corrected_fact: &str,
        reason: &str,
    ) -> Result<MemoryAddOutcome, String> {
        self.require_writable()?;
        self.memory_port()?
            .correct(&self.principal(), id, corrected_fact, reason)
    }

    pub fn memory_forget(
        &self,
        selector: ForgetSelector,
        reason: &str,
    ) -> Result<ForgetReport, String> {
        self.require_writable()?;
        self.memory_port()?
            .forget(&self.principal(), selector, reason)
    }

    pub fn memory_stats(&self) -> Result<MemoryStats, String> {
        match self.memory_read_port()? {
            Some(port) => port.stats(&self.principal()),
            None => Ok(MemoryStats::default()),
        }
    }

    pub async fn test_provider(&self, id: Option<&str>) -> ProviderTestView {
        self.model_gateway.test_provider(id).await
    }

    pub async fn fetch_models(&self, id: Option<&str>) -> Result<Vec<String>, String> {
        let (_, models) = self.model_gateway.list_models(id).await?;
        Ok(models)
    }

    /// Fetch the provider's live model list without persisting it. The
    /// settings wizard previews models before the provider is saved.
    pub async fn preview_models(&self, id: Option<&str>) -> Result<Vec<String>, String> {
        self.model_gateway.preview_models(id).await
    }
}
