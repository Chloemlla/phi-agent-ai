//! Headless shell.
//!
//! Runs the same `Node` as the desktop app without a UI. Durable events and
//! live model deltas go to stdout as JSONL; human summaries and logs go to
//! stderr. This is the primary way features are exercised in CI.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use phi_adapters::control;
use phi_adapters::jsonl_sink::{JsonlEventSink, JsonlStreamSink};
use phi_contracts::auxiliary::{
    AuxiliaryRoleInput, DecisionConfigInput, ImageConfigInput, ImageRequest,
};
use phi_contracts::memory::{
    ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryEpisodeInput, MemoryListFilter,
    MemoryQuery,
};
use phi_contracts::provider::{ModelSpec, ProviderInput};
use phi_contracts::stream::StreamPort;
use phi_domain::memory::{EpisodeSource, MemoryScope, MemoryState, SourceTrust};
use phi_domain::view::RunView;
use phi_i18n as i18n;
use phi_kernel::event_bus::EventSink;
use phi_node::{Node, NodeConfig, compose};

#[derive(Clone, Copy, Debug)]
enum ApprovalMode {
    Deny,
    Allow,
    Prompt,
}

struct GlobalOptions {
    data_dir: PathBuf,
    workspace: Option<PathBuf>,
    quiet: bool,
    language: Option<String>,
}

impl Default for GlobalOptions {
    fn default() -> Self {
        let data_dir = std::env::var_os("PHI_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".phi")
            });
        let workspace = std::env::var_os("PHI_WORKSPACE").map(PathBuf::from);
        Self {
            data_dir,
            workspace,
            quiet: false,
            language: None,
        }
    }
}

struct RunArgs {
    message: Option<String>,
    session_id: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    thinking: Option<String>,
    approval: ApprovalMode,
    stream: bool,
    title: Option<String>,
}

enum Command {
    Run(RunArgs),
    SessionsList,
    SessionsShow(String),
    SessionsDelete(String),
    AgentsList(String),
    AgentsShow(String),
    ImageConfigure(ImageConfigInput),
    ImageShow,
    ImageGenerate {
        prompt: String,
        session_id: Option<String>,
        size: Option<String>,
        count: u32,
        out: Option<PathBuf>,
    },
    ImageList {
        session_id: Option<String>,
    },
    ImageDelete(String),
    DecisionConfigure(DecisionConfigInput),
    DecisionShow,
    DecisionTest {
        state: String,
    },
    RolesList,
    RolesSet {
        role: String,
        input: AuxiliaryRoleInput,
    },
    RolesClear(String),
    MemoryMaintain {
        project_id: Option<String>,
        dry_run: bool,
    },
    Approve {
        id: String,
        approved: bool,
        note: Option<String>,
    },
    Cancel(String),
    Status(String),
    Bootstrap,
    ProvidersList,
    ProvidersShow(String),
    ProvidersAdd {
        input: ProviderInput,
        activate: bool,
    },
    ProvidersUpdate {
        input: ProviderInput,
        activate: bool,
    },
    ProvidersRemove(String),
    ProvidersUse(String),
    ProvidersTest(Option<String>),
    ProvidersModels(Option<String>),
    MemoryList(MemoryListFilter),
    MemorySearch(MemoryQuery),
    MemoryShow(String),
    MemoryAdd(MemoryAddRequest),
    MemoryInvalidate {
        id: String,
        reason: String,
    },
    MemoryCorrect {
        id: String,
        fact: String,
        reason: String,
    },
    MemorySupersede {
        id: String,
        request: MemoryAddRequest,
        reason: String,
    },
    MemoryForget {
        selector: ForgetSelector,
        reason: Option<String>,
    },
    MemoryStats,
    Help,
}

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // Resolve the language before parsing so even usage errors are localized.
    let language = scan_lang(&raw);
    i18n::set_lang(i18n::resolve(language.as_deref(), None));
    let (global, command) = match parse_cli(raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("[phi-headless] {error}");
            eprintln!();
            print_help();
            std::process::exit(2);
        }
    };
    let code = match execute(global, command).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[phi-headless] error: {error}");
            2
        }
    };
    std::process::exit(code);
}

async fn execute(global: GlobalOptions, command: Command) -> Result<i32, String> {
    match command {
        Command::Help => {
            print_help();
            Ok(0)
        }
        Command::Run(args) => run(global, args).await,
        Command::SessionsList => {
            let node = open_node(&global, false)?;
            let sessions = node.list_sessions();
            print_json(&serde_json::json!({
                "type": "sessions",
                "sessions": sessions,
            }))?;
            Ok(0)
        }
        Command::SessionsShow(id) => {
            let node = open_node(&global, false)?;
            print_json(&node.get_session(&id)?)?;
            Ok(0)
        }
        Command::SessionsDelete(id) => {
            let node = open_node(&global, true)?;
            let deleted = node.delete_session(&id)?;
            print_json(&serde_json::json!({
                "type": "session-deleted",
                "sessionId": id,
                "deleted": deleted,
            }))?;
            Ok(0)
        }
        Command::AgentsList(session_id) => {
            let node = open_node(&global, false)?;
            print_json(&serde_json::json!({
                "type": "agents",
                "sessionId": session_id,
                "agents": node.agents(&session_id),
            }))?;
            Ok(0)
        }
        Command::AgentsShow(agent_id) => {
            let node = open_node(&global, false)?;
            let agent = node.agent(&agent_id);
            let messages = node.agent_messages(&agent_id);
            print_json(&serde_json::json!({
                "type": "agent",
                "agent": agent,
                "messages": messages,
            }))?;
            Ok(0)
        }
        Command::ImageConfigure(input) => {
            let node = open_node(&global, true)?;
            print_json(&node.configure_image(input)?)?;
            Ok(0)
        }
        Command::ImageShow => {
            let node = open_node(&global, false)?;
            print_json(&node.image_status())?;
            Ok(0)
        }
        Command::ImageGenerate {
            prompt,
            session_id,
            size,
            count,
            out,
        } => {
            let node = open_node(&global, true)?;
            let response = node
                .generate_images(
                    session_id.as_deref(),
                    ImageRequest {
                        prompt,
                        size,
                        count,
                        reference_assets: Vec::new(),
                    },
                )
                .await?;
            let mut written = Vec::new();
            if let Some(out) = out {
                for (index, asset) in response.assets.iter().enumerate() {
                    let path = if response.assets.len() == 1 {
                        out.clone()
                    } else {
                        let stem = out
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or("image");
                        let extension = out
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or_else(|| match asset.mime.as_str() {
                                "image/jpeg" => "jpg",
                                "image/webp" => "webp",
                                "image/gif" => "gif",
                                "image/svg+xml" => "svg",
                                _ => "png",
                            });
                        out.with_file_name(format!("{stem}-{}{extension}", index + 1))
                    };
                    let bytes = node.image_asset_bytes(&asset.asset_id)?;
                    std::fs::write(&path, bytes).map_err(|error| {
                        i18n::t_args(
                            "Failed to write image: {error}",
                            &[("error", error.to_string())],
                        )
                    })?;
                    written.push(path.to_string_lossy().to_string());
                }
            }
            print_json(&serde_json::json!({
                "type": "image",
                "assets": response.assets,
                "written": written,
            }))?;
            Ok(0)
        }
        Command::ImageList { session_id } => {
            let node = open_node(&global, false)?;
            print_json(&serde_json::json!({
                "type": "images",
                "sessionId": session_id,
                "assets": node.image_assets(session_id.as_deref()),
            }))?;
            Ok(0)
        }
        Command::ImageDelete(asset_id) => {
            let node = open_node(&global, true)?;
            let deleted = node.delete_image_asset(&asset_id)?;
            print_json(&serde_json::json!({
                "type": "image_deleted",
                "assetId": asset_id,
                "deleted": deleted,
            }))?;
            Ok(0)
        }
        Command::DecisionConfigure(input) => {
            let node = open_node(&global, true)?;
            print_json(&node.configure_decision(input)?)?;
            Ok(0)
        }
        Command::DecisionShow => {
            let node = open_node(&global, false)?;
            print_json(&node.decision_status())?;
            Ok(0)
        }
        Command::DecisionTest { state } => {
            let node = open_node(&global, true)?;
            print_json(&node.test_decision(&state).await?)?;
            Ok(0)
        }
        Command::RolesList => {
            let node = open_node(&global, false)?;
            print_json(&serde_json::json!({
                "type": "roles",
                "roles": node.auxiliary_roles(),
            }))?;
            Ok(0)
        }
        Command::RolesSet { role, input } => {
            let node = open_node(&global, true)?;
            print_json(&node.set_auxiliary_role(&role, input)?)?;
            Ok(0)
        }
        Command::RolesClear(role) => {
            let node = open_node(&global, true)?;
            let removed = node.clear_auxiliary_role(&role)?;
            print_json(&serde_json::json!({
                "type": "role_cleared",
                "role": role,
                "removed": removed,
            }))?;
            Ok(0)
        }
        Command::MemoryMaintain {
            project_id,
            dry_run,
        } => {
            let node = open_node(&global, true)?;
            let report = node
                .maintain_memory(
                    project_id.as_deref(),
                    dry_run,
                    if dry_run { "cli dry-run" } else { "cli" },
                )
                .await?;
            print_json(&report)?;
            Ok(0)
        }
        Command::Approve { id, approved, note } => {
            // The run lives in another process; hand the decision over through
            // the data directory instead of composing a second writer.
            control::request_approval(&global.data_dir, &id, approved, note)?;
            print_json(&serde_json::json!({
                "type": "approval",
                "approvalId": id,
                "approved": approved,
                "queued": true,
            }))?;
            Ok(0)
        }
        Command::Cancel(run_id) => {
            control::request_cancel(&global.data_dir, &run_id)?;
            print_json(&serde_json::json!({
                "type": "cancel",
                "runId": run_id,
                "cancelled": true,
                "queued": true,
            }))?;
            Ok(0)
        }
        Command::Status(run_id) => {
            let node = open_node(&global, false)?;
            let status = node.run_status(&run_id);
            print_json(&serde_json::json!({
                "type": "run_status",
                "runId": run_id,
                "found": status.is_some(),
                "status": status,
            }))?;
            Ok(0)
        }
        Command::Bootstrap => {
            let node = open_node(&global, true)?;
            print_json(&node.bootstrap()?)?;
            Ok(0)
        }
        Command::ProvidersList => {
            let node = open_node(&global, false)?;
            print_json(&serde_json::json!({
                "type": "providers",
                "providers": node.provider_views(),
                "activeProviderId": node.active_provider_id(),
            }))?;
            Ok(0)
        }
        Command::ProvidersShow(id) => {
            let node = open_node(&global, false)?;
            let view = node
                .provider_view(&id)
                .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.clone())]))?;
            print_json(&view)?;
            Ok(0)
        }
        Command::ProvidersAdd { input, activate } => {
            let node = open_node(&global, true)?;
            let view = node.save_provider(input)?;
            if activate {
                node.set_active_provider(&view.id)?;
            }
            print_json(&view)?;
            Ok(0)
        }
        Command::ProvidersUpdate { input, activate } => {
            let node = open_node(&global, true)?;
            let view = node.save_provider(input)?;
            if activate {
                node.set_active_provider(&view.id)?;
            }
            print_json(&view)?;
            Ok(0)
        }
        Command::ProvidersRemove(id) => {
            let node = open_node(&global, true)?;
            let removed = node.remove_provider(&id)?;
            print_json(&serde_json::json!({
                "type": "provider_removed",
                "providerId": id,
                "removed": removed,
            }))?;
            Ok(0)
        }
        Command::ProvidersUse(id) => {
            let node = open_node(&global, true)?;
            node.set_active_provider(&id)?;
            print_json(&serde_json::json!({
                "type": "provider_active",
                "providerId": id,
            }))?;
            Ok(0)
        }
        Command::ProvidersTest(id) => {
            let node = open_node(&global, true)?;
            let result = node.test_provider(id.as_deref()).await;
            let ok = result.ok;
            print_json(&result)?;
            Ok(if ok { 0 } else { 4 })
        }
        Command::ProvidersModels(id) => {
            let node = open_node(&global, true)?;
            let models = node.fetch_models(id.as_deref()).await?;
            print_json(&serde_json::json!({
                "type": "models",
                "models": models,
            }))?;
            Ok(0)
        }
        Command::MemoryList(filter) => {
            let node = open_node(&global, false)?;
            let memories = node.memory_list(filter)?;
            print_json(&serde_json::json!({
                "type": "memories",
                "memories": memories,
            }))?;
            Ok(0)
        }
        Command::MemorySearch(query) => {
            let node = open_node(&global, false)?;
            let text = query.text.clone();
            let hits = node.memory_search(query)?;
            print_json(&serde_json::json!({
                "type": "memory_search",
                "query": text,
                "hits": hits,
            }))?;
            Ok(0)
        }
        Command::MemoryShow(id) => {
            let node = open_node(&global, false)?;
            let record = node
                .memory_get(&id)?
                .ok_or_else(|| i18n::t("Memory record not found"))?;
            let episode = match record.episode_id.as_deref() {
                Some(episode_id) => node.memory_episode(episode_id)?,
                None => None,
            };
            print_json(&serde_json::json!({
                "type": "memory",
                "record": record,
                "episode": episode,
            }))?;
            Ok(0)
        }
        Command::MemoryAdd(request) => {
            let node = open_node(&global, true)?;
            let outcome = node.memory_add(request)?;
            print_json(&outcome)?;
            Ok(memory_outcome_exit_code(&outcome))
        }
        Command::MemoryInvalidate { id, reason } => {
            let node = open_node(&global, true)?;
            node.memory_invalidate(&id, &reason)?;
            print_json(&serde_json::json!({
                "type": "memory_invalidated",
                "id": id,
                "reason": reason,
            }))?;
            Ok(0)
        }
        Command::MemoryCorrect { id, fact, reason } => {
            let node = open_node(&global, true)?;
            let outcome = node.memory_correct(&id, &fact, &reason)?;
            print_json(&outcome)?;
            Ok(memory_outcome_exit_code(&outcome))
        }
        Command::MemorySupersede {
            id,
            request,
            reason,
        } => {
            let node = open_node(&global, true)?;
            let outcome = node.memory_supersede(&id, request, &reason)?;
            print_json(&outcome)?;
            Ok(memory_outcome_exit_code(&outcome))
        }
        Command::MemoryForget { selector, reason } => {
            let node = open_node(&global, true)?;
            let report = node.memory_forget(selector, reason.as_deref().unwrap_or(""))?;
            print_json(&serde_json::json!({
                "type": "memory_forgotten",
                "records": report.records,
                "episodes": report.episodes,
            }))?;
            Ok(0)
        }
        Command::MemoryStats => {
            let node = open_node(&global, false)?;
            let stats = node.memory_stats()?;
            print_json(&serde_json::json!({
                "type": "memory_stats",
                "stats": stats,
            }))?;
            Ok(0)
        }
    }
}

fn memory_outcome_exit_code(outcome: &MemoryAddOutcome) -> i32 {
    match outcome {
        MemoryAddOutcome::Rejected { .. } => 4,
        _ => 0,
    }
}

fn open_node(global: &GlobalOptions, writable: bool) -> Result<Arc<Node>, String> {
    compose(NodeConfig {
        data_dir: global.data_dir.clone(),
        workspace: global.workspace.clone(),
        fallback_workspace: None,
        event_sinks: Vec::new(),
        stream_sink: None,
        language: global.language.clone(),
        writable,
    })
}

async fn run(global: GlobalOptions, args: RunArgs) -> Result<i32, String> {
    let event_sink: Arc<dyn EventSink> = if global.quiet {
        Arc::new(JsonlEventSink::quiet())
    } else {
        Arc::new(JsonlEventSink::new())
    };
    let stream_sink: Option<Arc<dyn StreamPort>> = if args.stream {
        Some(Arc::new(JsonlStreamSink))
    } else {
        None
    };
    let node = compose(NodeConfig {
        data_dir: global.data_dir.clone(),
        workspace: global.workspace.clone(),
        fallback_workspace: None,
        event_sinks: vec![event_sink],
        stream_sink,
        language: global.language.clone(),
        writable: true,
    })?;

    if let Some(provider) = args.provider.as_deref() {
        node.set_active_provider(provider)?;
    }

    let message = match args.message {
        Some(message) if message.trim() == "-" => read_stdin()?,
        Some(message) => message,
        None => read_stdin()?,
    };
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err(i18n::t(
            "Message is empty (use --message or pipe text through stdin)",
        ));
    }

    let session_id = match args.session_id {
        Some(id) => {
            node.get_session(&id)?;
            id
        }
        None => {
            let title = args
                .title
                .unwrap_or_else(|| message.chars().take(24).collect::<String>());
            node.create_session(Some(&title))?.id
        }
    };

    let worker = tokio::spawn(approval_worker(
        node.clone(),
        session_id.clone(),
        args.approval,
    ));
    let run_id = node.send_message_full(
        &session_id,
        &message,
        None,
        args.model.as_deref(),
        args.thinking.as_deref(),
    );
    let run_id = match run_id {
        Ok(run_id) => run_id,
        Err(error) => {
            worker.abort();
            return Err(error);
        }
    };

    let result = wait_for_terminal(&node, &session_id, &run_id).await;
    worker.abort();
    let run = result?;
    let auxiliary = node.settle_auxiliary(&session_id).await;

    print_json(&serde_json::json!({
        "type": "result",
        "sessionId": session_id,
        "runId": run_id,
        "run": run,
        "auxiliary": auxiliary,
    }))?;
    eprintln!(
        "[phi-headless] run {} {} steps={} tokens_in={} tokens_out={}{}",
        run.id,
        run.state,
        run.steps,
        run.input_tokens,
        run.output_tokens,
        run.error
            .as_deref()
            .map(|error| format!(" ({error})"))
            .unwrap_or_default()
    );

    Ok(match run.state.as_str() {
        "Succeeded" => 0,
        "Aborted" => 3,
        "AwaitingReconciliation" => 5,
        _ => 4,
    })
}

async fn wait_for_terminal(node: &Node, session_id: &str, run_id: &str) -> Result<RunView, String> {
    loop {
        if let Some(session) = node.session_view(session_id) {
            if let Some(run) = session.runs.iter().find(|run| run.id == run_id) {
                if matches!(
                    run.state.as_str(),
                    "Succeeded" | "Failed" | "Aborted" | "AwaitingReconciliation"
                ) {
                    return Ok(run.clone());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn approval_worker(node: Arc<Node>, session_id: String, mode: ApprovalMode) {
    loop {
        for approval in node.pending_approvals(&session_id) {
            let (approved, note) = match mode {
                ApprovalMode::Deny => (false, "headless: denied by --approval deny".to_string()),
                ApprovalMode::Allow => (
                    true,
                    "headless: auto-approved by --approval allow (tests only)".to_string(),
                ),
                ApprovalMode::Prompt => {
                    eprintln!(
                        "[approval] {} tool={} risk={}",
                        approval.id, approval.tool_id, approval.risk
                    );
                    eprintln!("[approval] args={}", approval.args_preview);
                    eprintln!("[approval] approve? [y/N]");
                    let line = tokio::task::spawn_blocking(prompt_line)
                        .await
                        .unwrap_or_default();
                    let approved = matches!(
                        line.trim().to_ascii_lowercase().as_str(),
                        "y" | "yes" | "approve"
                    );
                    (
                        approved,
                        format!(
                            "headless: interactive ({})",
                            if approved { "approved" } else { "rejected" }
                        ),
                    )
                }
            };
            if let Err(error) = node.approve(&approval.id, approved, Some(note)) {
                eprintln!(
                    "[phi-headless] {}",
                    i18n::t_args("Approval failed: {error}", &[("error", error)])
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn prompt_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| {
            i18n::t_args(
                "Failed to read stdin: {error}",
                &[("error", error.to_string())],
            )
        })?;
    Ok(text)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
    println!("{text}");
    Ok(())
}

// ─────────────────────────── argument parsing ───────────────────────────

/// Find `--lang` before the regular parser so usage errors are localized.
fn scan_lang(raw: &[String]) -> Option<String> {
    let mut index = 0;
    while index < raw.len() {
        let arg = &raw[index];
        if arg == "--lang" {
            return raw.get(index + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix("--lang=") {
            return Some(value.to_string());
        }
        index += 1;
    }
    None
}

fn parse_cli(raw: Vec<String>) -> Result<(GlobalOptions, Command), String> {
    let mut global = GlobalOptions::default();
    let mut remaining = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        let arg = &raw[index];
        match arg.as_str() {
            "--help" | "-h" => return Ok((global, Command::Help)),
            "--quiet" => {
                global.quiet = true;
                index += 1;
            }
            "--json" => {
                // stdout is already JSONL; accepted for compatibility.
                index += 1;
            }
            "--data-dir" => {
                index += 1;
                global.data_dir = PathBuf::from(
                    raw.get(index)
                        .ok_or_else(|| i18n::t("--data-dir requires a value"))?,
                );
                index += 1;
            }
            "--workspace" => {
                index += 1;
                global.workspace = Some(PathBuf::from(
                    raw.get(index)
                        .ok_or_else(|| i18n::t("--workspace requires a value"))?,
                ));
                index += 1;
            }
            "--lang" => {
                index += 1;
                global.language = Some(
                    raw.get(index)
                        .cloned()
                        .ok_or_else(|| i18n::t("--lang requires a value"))?,
                );
                index += 1;
            }
            _ if arg.starts_with("--data-dir=") => {
                global.data_dir = PathBuf::from(arg.trim_start_matches("--data-dir="));
                index += 1;
            }
            _ if arg.starts_with("--workspace=") => {
                global.workspace = Some(PathBuf::from(arg.trim_start_matches("--workspace=")));
                index += 1;
            }
            _ if arg.starts_with("--lang=") => {
                global.language = Some(arg.trim_start_matches("--lang=").to_string());
                index += 1;
            }
            _ => {
                remaining.push(arg.clone());
                index += 1;
            }
        }
    }

    let Some(command) = remaining.first().cloned() else {
        return Ok((global, Command::Help));
    };
    let rest = &remaining[1..];
    let command = match command.as_str() {
        "run" => Command::Run(parse_run(rest)?),
        "sessions" => match rest.first().map(String::as_str) {
            Some("list") | Some("ls") | None => Command::SessionsList,
            Some("show") => Command::SessionsShow(parse_id(rest, 1)?),
            Some("delete") | Some("rm") => Command::SessionsDelete(parse_id(rest, 1)?),
            Some(other) => {
                return Err(i18n::t_args(
                    "Unknown sessions subcommand: {command}",
                    &[("command", other.to_string())],
                ));
            }
        },
        "agents" => match rest.first().map(String::as_str) {
            Some("list") => Command::AgentsList(parse_id(rest, 1)?),
            Some("show") => Command::AgentsShow(parse_id(rest, 1)?),
            Some(other) => {
                return Err(i18n::t_args(
                    "Unknown agents subcommand: {command}",
                    &[("command", other.to_string())],
                ));
            }
            None => {
                return Err(i18n::t(
                    "agents requires a subcommand: list <session-id> or show <agent-id>",
                ));
            }
        },
        "image" => parse_image(rest)?,
        "decision" => parse_decision(rest)?,
        "roles" => parse_roles(rest)?,
        "approve" => parse_approve(rest)?,
        "cancel" => Command::Cancel(parse_id(rest, 0)?),
        "status" => Command::Status(parse_id(rest, 0)?),
        "bootstrap" => Command::Bootstrap,
        "providers" => parse_providers(rest)?,
        "memory" => parse_memory(rest)?,
        other => {
            return Err(i18n::t_args(
                "Unknown command: {command}",
                &[("command", other.to_string())],
            ));
        }
    };
    Ok((global, command))
}

fn parse_run(rest: &[String]) -> Result<RunArgs, String> {
    let mut args = RunArgs {
        message: None,
        session_id: None,
        provider: None,
        model: None,
        thinking: None,
        approval: ApprovalMode::Deny,
        stream: true,
        title: None,
    };
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--message")? {
            args.message = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--session")? {
            args.session_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--provider")? {
            args.provider = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--model")? {
            args.model = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--thinking")? {
            args.thinking = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--title")? {
            args.title = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--approval")? {
            args.approval = match value.trim().to_ascii_lowercase().as_str() {
                "deny" => ApprovalMode::Deny,
                "allow" | "auto" => ApprovalMode::Allow,
                "prompt" | "ask" => ApprovalMode::Prompt,
                other => {
                    return Err(i18n::t_args(
                        "Unknown approval mode {mode}; expected deny, allow or prompt",
                        &[("mode", other.to_string())],
                    ));
                }
            };
        } else {
            match rest[index].as_str() {
                "--no-stream" => {
                    args.stream = false;
                    index += 1;
                }
                "--stream" => {
                    args.stream = true;
                    index += 1;
                }
                other => {
                    return Err(i18n::t_args(
                        "Unknown run argument: {arg}",
                        &[("arg", other.to_string())],
                    ));
                }
            }
        }
    }
    Ok(args)
}

fn parse_approve(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut approved = None;
    let mut note = None;
    let mut index = 0;
    while index < rest.len() {
        let arg = &rest[index];
        if arg == "--allow" || arg == "--approve" {
            approved = Some(true);
            index += 1;
        } else if arg == "--deny" || arg == "--reject" {
            approved = Some(false);
            index += 1;
        } else if let Some(value) = take_flag(rest, &mut index, "--note")? {
            note = Some(value);
        } else if arg.as_str() == id.as_str() {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown approve argument: {arg}",
                &[("arg", arg.clone())],
            ));
        }
    }
    let approved = approved.ok_or_else(|| i18n::t("approve requires --allow or --deny"))?;
    Ok(Command::Approve { id, approved, note })
}

fn parse_id(rest: &[String], position: usize) -> Result<String, String> {
    if let Some(value) = rest.get(position) {
        if !value.starts_with('-') {
            return Ok(value.clone());
        }
    }
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            return Ok(value);
        }
        index += 1;
    }
    Err(i18n::t("Missing id"))
}

fn parse_providers(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Ok(Command::ProvidersList);
    };
    let args = &rest[1..];
    match sub {
        "list" | "ls" => Ok(Command::ProvidersList),
        "show" => Ok(Command::ProvidersShow(parse_id(args, 0)?)),
        "add" => {
            let (input, activate) = parse_provider_input(args, false)?;
            Ok(Command::ProvidersAdd { input, activate })
        }
        "update" | "edit" => {
            let (input, activate) = parse_provider_input(args, true)?;
            Ok(Command::ProvidersUpdate { input, activate })
        }
        "remove" | "rm" => Ok(Command::ProvidersRemove(parse_id(args, 0)?)),
        "use" | "activate" => Ok(Command::ProvidersUse(parse_id(args, 0)?)),
        "test" => Ok(Command::ProvidersTest(optional_id(args)?)),
        "models" => Ok(Command::ProvidersModels(optional_id(args)?)),
        other => Err(i18n::t_args(
            "Unknown providers subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn optional_id(rest: &[String]) -> Result<Option<String>, String> {
    match parse_id(rest, 0) {
        Ok(id) => Ok(Some(id)),
        Err(_) => Ok(None),
    }
}

// ─────────────────────────── memory parsing ───────────────────────────

fn parse_image(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t(
            "image requires a subcommand: configure, show, generate, list or delete",
        ));
    };
    let args = &rest[1..];
    match sub {
        "configure" => {
            let mut input = ImageConfigInput {
                enabled: true,
                format: "png".to_string(),
                ..Default::default()
            };
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--provider")? {
                    input.provider = value;
                } else if let Some(value) = take_flag(args, &mut index, "--base-url")? {
                    input.base_url = value;
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--api-key")? {
                    input.api_key = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--size")? {
                    input.size = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--quality")? {
                    input.quality = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--format")? {
                    input.format = value;
                } else if let Some(value) = take_flag(args, &mut index, "--max-per-turn")? {
                    input.max_per_turn = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--max-per-turn".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--max-bytes")? {
                    input.max_bytes = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--max-bytes".to_string()), ("value", value)],
                        )
                    })?;
                } else if args[index] == "--grant-egress" {
                    input.egress_granted = true;
                    index += 1;
                } else if args[index] == "--disabled" {
                    input.enabled = false;
                    index += 1;
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::ImageConfigure(input))
        }
        "show" => Ok(Command::ImageShow),
        "generate" => {
            let mut prompt = None;
            let mut session_id = None;
            let mut size = None;
            let mut count = 1;
            let mut out = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--prompt")? {
                    prompt = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--session")? {
                    session_id = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--size")? {
                    size = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--count")? {
                    count = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--count".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--out")? {
                    out = Some(PathBuf::from(value));
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::ImageGenerate {
                prompt: prompt.ok_or_else(|| i18n::t("--prompt is required"))?,
                session_id,
                size,
                count,
                out,
            })
        }
        "list" => {
            let mut session_id = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--session")? {
                    session_id = Some(value);
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::ImageList { session_id })
        }
        "delete" => Ok(Command::ImageDelete(parse_id(args, 0)?)),
        other => Err(i18n::t_args(
            "Unknown image subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn parse_decision(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t(
            "decision requires a subcommand: configure, show or test",
        ));
    };
    let args = &rest[1..];
    match sub {
        "configure" => {
            let mut input = DecisionConfigInput {
                enabled: true,
                timeout_ms: 8_000,
                triage_threshold: 0.92,
                triage_tools: vec!["fs_write".to_string()],
                compaction_threshold: 0.8,
                compaction_min_chars: 8_000,
                ..Default::default()
            };
            let mut custom_tools = false;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--endpoint")? {
                    input.endpoint = value;
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--api-key")? {
                    input.api_key = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--timeout-ms")? {
                    input.timeout_ms = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--timeout-ms".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--triage-threshold")? {
                    input.triage_threshold = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--triage-threshold".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--triage-tool")? {
                    if !custom_tools {
                        input.triage_tools.clear();
                        custom_tools = true;
                    }
                    input.triage_tools.push(value);
                } else if let Some(value) = take_flag(args, &mut index, "--compaction-threshold")? {
                    input.compaction_threshold = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[
                                ("flag", "--compaction-threshold".to_string()),
                                ("value", value),
                            ],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--compaction-min-chars")? {
                    input.compaction_min_chars = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[
                                ("flag", "--compaction-min-chars".to_string()),
                                ("value", value),
                            ],
                        )
                    })?;
                } else if args[index] == "--triage" {
                    input.triage_enabled = true;
                    index += 1;
                } else if args[index] == "--compaction" {
                    input.compaction_enabled = true;
                    index += 1;
                } else if args[index] == "--grant-egress" {
                    input.egress_granted = true;
                    index += 1;
                } else if args[index] == "--disabled" {
                    input.enabled = false;
                    index += 1;
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::DecisionConfigure(input))
        }
        "show" => Ok(Command::DecisionShow),
        "test" => {
            let mut state = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--state")? {
                    state = Some(value);
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::DecisionTest {
                state: state.ok_or_else(|| i18n::t("--state is required"))?,
            })
        }
        other => Err(i18n::t_args(
            "Unknown decision subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn parse_roles(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t("roles requires a subcommand: list, set or clear"));
    };
    let args = &rest[1..];
    match sub {
        "list" => Ok(Command::RolesList),
        "set" => {
            let role = args
                .first()
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| i18n::t("roles set requires a role name"))?;
            let mut input = AuxiliaryRoleInput::default();
            let mut index = usize::from(!args.is_empty());
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--provider")? {
                    input.provider_id = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--thinking")? {
                    input.thinking = Some(value);
                } else if args[index] == "--grant-egress" {
                    input.egress_granted = true;
                    index += 1;
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::RolesSet { role, input })
        }
        "clear" => Ok(Command::RolesClear(parse_id(args, 0)?)),
        other => Err(i18n::t_args(
            "Unknown roles subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn parse_memory(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Ok(Command::MemoryList(MemoryListFilter::default()));
    };
    // `memory --text foo` is accepted as `memory list --text foo`.
    if sub.starts_with('-') {
        return Ok(Command::MemoryList(parse_memory_list(rest)?));
    }
    let args = &rest[1..];
    match sub {
        "list" | "ls" => Ok(Command::MemoryList(parse_memory_list(args)?)),
        "search" | "find" => Ok(Command::MemorySearch(parse_memory_search(args)?)),
        "show" => Ok(Command::MemoryShow(parse_id(args, 0)?)),
        "add" | "remember" => Ok(Command::MemoryAdd(parse_memory_add(args)?)),
        "invalidate" => parse_memory_invalidate(args),
        "correct" | "fix" => parse_memory_correct(args),
        "supersede" | "replace" => parse_memory_supersede(args),
        "forget" | "delete" => parse_memory_forget(args),
        "maintain" | "maintenance" => parse_memory_maintain(args),
        "stats" => Ok(Command::MemoryStats),
        other => Err(i18n::t_args(
            "Unknown memory subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn parse_memory_maintain(args: &[String]) -> Result<Command, String> {
    let mut dry_run = true;
    let mut project_id = None;
    let mut index = 0;
    while index < args.len() {
        if let Some(value) = take_flag(args, &mut index, "--project")? {
            project_id = Some(value);
        } else if args[index] == "--apply" {
            dry_run = false;
            index += 1;
        } else if args[index] == "--dry-run" {
            dry_run = true;
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown argument: {arg}",
                &[("arg", args[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryMaintain {
        project_id,
        dry_run,
    })
}

fn parse_memory_scope(raw: &str) -> Result<MemoryScope, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(MemoryScope::User),
        "project" => Ok(MemoryScope::Project),
        "agent" => Ok(MemoryScope::Agent),
        "run" => Ok(MemoryScope::Run),
        other => Err(i18n::t_args(
            "Unknown memory scope {scope}; expected user, project, agent or run",
            &[("scope", other.to_string())],
        )),
    }
}

fn parse_memory_trust(raw: &str) -> Result<SourceTrust, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(SourceTrust::User),
        "system" => Ok(SourceTrust::System),
        "tool" => Ok(SourceTrust::Tool),
        "model" => Ok(SourceTrust::Model),
        "external" => Ok(SourceTrust::External),
        other => Err(i18n::t_args(
            "Unknown memory source {source}; expected user, system, tool, model or external",
            &[("source", other.to_string())],
        )),
    }
}

fn parse_memory_state(raw: &str) -> Result<MemoryState, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "active" => Ok(MemoryState::Active),
        "invalidated" => Ok(MemoryState::Invalidated),
        "superseded" => Ok(MemoryState::Superseded),
        other => Err(i18n::t_args(
            "Unknown memory state {state}; expected active, invalidated or superseded",
            &[("state", other.to_string())],
        )),
    }
}

fn parse_episode_source(raw: &str) -> Result<EpisodeSource, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user_input" | "user" => Ok(EpisodeSource::UserInput),
        "tool_result" | "tool" => Ok(EpisodeSource::ToolResult),
        "model_output" | "model" => Ok(EpisodeSource::ModelOutput),
        "external" => Ok(EpisodeSource::External),
        other => Err(i18n::t_args(
            "Unknown episode source {source}; expected user_input, tool_result, model_output or external",
            &[("source", other.to_string())],
        )),
    }
}

fn parse_memory_list(rest: &[String]) -> Result<MemoryListFilter, String> {
    let mut filter = MemoryListFilter::default();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--text")? {
            filter.text = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            filter.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--state")? {
            filter.states.push(parse_memory_state(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--project")? {
            filter.project_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--run")? {
            filter.run_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--limit")? {
            filter.limit = value.parse().map_err(|_| {
                i18n::t_args(
                    "--limit is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if let Some(value) = take_flag(rest, &mut index, "--offset")? {
            filter.offset = value.parse().map_err(|_| {
                i18n::t_args(
                    "--offset is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if !rest[index].starts_with('-') {
            filter.text = Some(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory list argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(filter)
}

fn parse_memory_search(rest: &[String]) -> Result<MemoryQuery, String> {
    let mut query = MemoryQuery {
        limit: 10,
        ..Default::default()
    };
    let mut parts: Vec<String> = Vec::new();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            query.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--limit")? {
            query.limit = value.parse().map_err(|_| {
                i18n::t_args(
                    "--limit is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if let Some(value) = take_flag(rest, &mut index, "--min-score")? {
            query.min_score = parse_finite_f64("--min-score", &value)?;
        } else if rest[index] == "--include-inactive" {
            query.include_inactive = true;
            index += 1;
        } else if !rest[index].starts_with('-') {
            parts.push(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory search argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    if parts.is_empty() {
        return Err(i18n::t("memory search requires a query"));
    }
    query.text = parts.join(" ");
    Ok(query)
}

fn parse_memory_add(rest: &[String]) -> Result<MemoryAddRequest, String> {
    let mut fact = None;
    let mut scope = MemoryScope::User;
    let mut trust = SourceTrust::User;
    let mut confidence = None;
    let mut valid_from = None;
    let mut valid_to = None;
    let mut episode = None;
    let mut episode_source = EpisodeSource::UserInput;
    let mut episode_label = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            scope = parse_memory_scope(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--source")? {
            trust = parse_memory_trust(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--confidence")? {
            confidence = Some(parse_finite_f64("--confidence", &value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-from")? {
            valid_from = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-to")? {
            valid_to = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--episode")? {
            episode = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--episode-source")? {
            episode_source = parse_episode_source(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--episode-label")? {
            episode_label = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else {
            return Err(i18n::t_args(
                "Unknown memory add argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let fact = fact.ok_or_else(|| i18n::t("memory add requires --fact TEXT"))?;
    if scope == MemoryScope::Run {
        return Err(i18n::t(
            "Run-scoped memories are written by the agent inside a run",
        ));
    }
    Ok(MemoryAddRequest {
        fact,
        scope,
        source_trust: trust,
        confidence,
        valid_from,
        valid_to,
        episode: episode.map(|content| MemoryEpisodeInput {
            content,
            source: episode_source,
            source_label: episode_label,
            session_id: None,
        }),
        reason,
    })
}

fn require_reason(value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| i18n::t("This memory command requires --reason TEXT"))
}

fn parse_finite_f64(flag: &str, value: &str) -> Result<f64, String> {
    let parsed: f64 = value.parse().map_err(|_| {
        i18n::t_args(
            "{flag} is not a number: {value}",
            &[("flag", flag.to_string()), ("value", value.to_string())],
        )
    })?;
    if !parsed.is_finite() {
        return Err(i18n::t_args(
            "{flag} must be a finite number: {value}",
            &[("flag", flag.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(parsed)
}

fn parse_memory_invalidate(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory invalidate argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryInvalidate {
        id,
        reason: require_reason(reason)?,
    })
}

fn parse_memory_correct(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut fact = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory correct argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryCorrect {
        id,
        fact: fact.ok_or_else(|| i18n::t("memory correct requires --fact TEXT"))?,
        reason: require_reason(reason)?,
    })
}

fn parse_memory_supersede(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut fact = None;
    let mut scope = None;
    let mut trust = SourceTrust::User;
    let mut confidence = None;
    let mut valid_from = None;
    let mut valid_to = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            scope = Some(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--source")? {
            trust = parse_memory_trust(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--confidence")? {
            confidence = Some(parse_finite_f64("--confidence", &value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-from")? {
            valid_from = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-to")? {
            valid_to = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory supersede argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let fact = fact.ok_or_else(|| i18n::t("memory supersede requires --fact TEXT"))?;
    let scope = scope.ok_or_else(|| i18n::t("memory supersede requires --scope"))?;
    if scope == MemoryScope::Run {
        return Err(i18n::t(
            "Run-scoped memories are written by the agent inside a run",
        ));
    }
    Ok(Command::MemorySupersede {
        id,
        request: MemoryAddRequest {
            fact,
            scope,
            source_trust: trust,
            confidence,
            valid_from,
            valid_to,
            episode: None,
            reason: None,
        },
        reason: require_reason(reason)?,
    })
}

fn parse_memory_forget(rest: &[String]) -> Result<Command, String> {
    let mut selector = ForgetSelector::default();
    let mut all = false;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            selector.ids.push(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            selector.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--project")? {
            selector.project_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--run")? {
            selector.run_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if rest[index] == "--all" {
            all = true;
            index += 1;
        } else if !rest[index].starts_with('-') {
            selector.ids.push(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory forget argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let has_selector = selector.has_selector();
    if all && has_selector {
        return Err(i18n::t("Do not combine --all with other forget selectors"));
    }
    if !all && !has_selector {
        return Err(i18n::t(
            "memory forget requires --id, --scope, --project, --run or --all",
        ));
    }
    selector.all = all;
    Ok(Command::MemoryForget { selector, reason })
}

fn parse_provider_input(
    rest: &[String],
    require_id: bool,
) -> Result<(ProviderInput, bool), String> {
    let mut input = ProviderInput::default();
    let mut activate = false;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            input.id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--name")? {
            input.name = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--protocol")? {
            input.protocol = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--base-url")? {
            input.base_url = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--api-key")? {
            input.api_key = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--model")? {
            input.model = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--auth")? {
            input.auth = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--models")? {
            input.models = Some(
                value
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .map(ModelSpec::from_id)
                    .collect(),
            );
        } else if let Some(value) = take_flag(rest, &mut index, "--model-map")? {
            let mut map = std::collections::HashMap::new();
            for pair in value.split(',') {
                let Some((key, value)) = pair.split_once('=') else {
                    return Err(i18n::t_args(
                        "--model-map expects key=model: {pair}",
                        &[("pair", pair.to_string())],
                    ));
                };
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
            input.model_map = Some(map);
        } else if let Some(value) = take_flag(rest, &mut index, "--max-output-tokens")? {
            input.max_output_tokens = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--max-output-tokens is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--temperature")? {
            input.temperature = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--temperature is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--sort-index")? {
            input.sort_index = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--sort-index is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--icon")? {
            input.icon = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--icon-color")? {
            input.icon_color = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--website")? {
            input.website_url = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--notes")? {
            input.notes = Some(value);
        } else {
            match rest[index].as_str() {
                "--activate" => {
                    activate = true;
                    index += 1;
                }
                "--disabled" => {
                    input.enabled = Some(false);
                    index += 1;
                }
                value if !value.starts_with('-') => {
                    positional.push(value.to_string());
                    index += 1;
                }
                other => {
                    return Err(i18n::t_args(
                        "Unknown provider argument: {arg}",
                        &[("arg", other.to_string())],
                    ));
                }
            }
        }
    }
    if input.id.is_none() {
        if let Some(first) = positional.first() {
            input.id = Some(first.clone());
        }
    }
    if require_id && input.id.is_none() {
        return Err(i18n::t("update requires a provider id"));
    }
    Ok((input, activate))
}

fn take_flag(args: &[String], index: &mut usize, flag: &str) -> Result<Option<String>, String> {
    let arg = args.get(*index).ok_or_else(|| {
        i18n::t_args(
            "Internal error: index out of bounds ({flag})",
            &[("flag", flag.to_string())],
        )
    })?;
    if arg == flag {
        let value = args.get(*index + 1).cloned().ok_or_else(|| {
            i18n::t_args("{flag} requires a value", &[("flag", flag.to_string())])
        })?;
        *index += 2;
        return Ok(Some(value));
    }
    let prefix = format!("{flag}=");
    if let Some(value) = arg.strip_prefix(&prefix) {
        *index += 1;
        return Ok(Some(value.to_string()));
    }
    Ok(None)
}

fn print_help() {
    eprintln!(
        r#"phi-headless — local-first agent node without a UI

Usage:
  phi-headless [global options] <command> [options]

Commands:
  run --message TEXT [--session ID] [--provider ID]
      [--model ID] [--thinking off|low|medium|high|xhigh|max]
      [--approval deny|allow|prompt] [--no-stream] [--title TEXT]
  sessions list
  sessions show <id>
  sessions delete <id>
  agents list <session-id>
  agents show <agent-id>
  image configure --provider openai_images|gemini_images --base-url URL
      --model MODEL [--api-key REF] [--size S] [--quality Q] [--format F]
      [--max-per-turn N] [--max-bytes N] [--grant-egress] [--disabled]
  image show
  image generate --prompt TEXT [--session ID] [--size S] [--count N] [--out PATH]
  image list [--session ID]
  image delete <asset-id>
  decision configure --endpoint URL --model MODEL [--api-key REF]
      [--timeout-ms N] [--triage] [--triage-threshold F] [--triage-tool TOOL]...
      [--compaction] [--compaction-threshold F] [--compaction-min-chars N]
      [--grant-egress] [--disabled]
  decision show
  decision test --state TEXT
  roles list
  roles set <role> --model MODEL [--provider ID] [--thinking LEVEL] [--grant-egress]
  roles clear <role>
  approve <id> --allow|--deny [--note TEXT]
  cancel <run-id>
  status <run-id>
  bootstrap
  providers list
  providers show <id>
  providers add --name NAME --protocol P --base-url URL --model MODEL
      [--api-key env:NAME|literal:VALUE|VALUE] [--auth x_api_key|bearer|none]
      [--max-output-tokens N]
      [--temperature N] [--models a,b] [--model-map role=model]
      [--icon NAME] [--icon-color #hex] [--website URL] [--notes TEXT]
      [--sort-index N] [--activate] [--disabled]
  providers update <id> [same flags]
  providers remove <id>
  providers use <id>
  providers test [id]
  providers models [id]
  memory list [--text TEXT] [--scope user|project|agent|run] [--state STATE]
      [--project ID] [--run ID] [--limit N] [--offset N]
  memory search QUERY [--scope SCOPE] [--limit N] [--min-score N]
      [--include-inactive]
  memory show <id>
  memory add --fact TEXT [--scope user|project|agent]
      [--source user|system|tool|model|external] [--confidence N]
      [--valid-from RFC3339] [--valid-to RFC3339] [--episode TEXT]
      [--episode-source SOURCE] [--episode-label TEXT] [--reason TEXT]
  memory invalidate <id> --reason TEXT
  memory correct <id> --fact TEXT --reason TEXT
  memory supersede <id> --fact TEXT --scope user|project|agent --reason TEXT
      [--source TRUST] [--confidence N] [--valid-from T] [--valid-to T]
  memory forget <id>... | --id ID | --scope SCOPE | --project ID | --run ID | --all
      [--reason TEXT]
  memory maintain [--project ID] [--dry-run|--apply]
  memory stats

Global options:
  --data-dir DIR    default $PHI_DATA_DIR or ./.phi
  --workspace DIR   default $PHI_WORKSPACE or <data-dir>/workspace
  --lang LANG       en or zh-CN (default $PHI_LANG, then saved setting)
  --quiet           suppress per-event JSON lines on stdout
  --json            accepted; stdout is already JSONL

stdout: one JSON object per line (events, model deltas, final result)
stderr: logs and human summaries
exit:   0 success, 3 aborted, 4 failed, 5 needs reconciliation, 2 usage/config error"#
    );
}
