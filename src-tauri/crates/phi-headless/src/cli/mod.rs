//! Command-line surface: global options, command parsing and help text.

use std::path::PathBuf;

use phi_contracts::auxiliary::{AuxiliaryRoleInput, DecisionConfigInput, ImageConfigInput};
use phi_contracts::memory::{ForgetSelector, MemoryAddRequest, MemoryListFilter, MemoryQuery};
use phi_contracts::provider::ProviderInput;
use phi_i18n as i18n;

mod common;
mod decision;
mod help;
mod image;
mod memory;
mod providers;
mod roles;
mod run;
mod sessions;
mod status;

pub(crate) use help::print_help;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ApprovalMode {
    Deny,
    Allow,
    Prompt,
}

pub(crate) struct GlobalOptions {
    pub(crate) data_dir: PathBuf,
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) quiet: bool,
    pub(crate) language: Option<String>,
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

pub(crate) struct RunArgs {
    pub(crate) message: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) thinking: Option<String>,
    pub(crate) approval: ApprovalMode,
    pub(crate) stream: bool,
    pub(crate) title: Option<String>,
}

pub(crate) enum Command {
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

/// Find `--lang` before the regular parser so usage errors are localized.
pub(crate) fn scan_lang(raw: &[String]) -> Option<String> {
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

pub(crate) fn parse_cli(raw: Vec<String>) -> Result<(GlobalOptions, Command), String> {
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
        "run" => Command::Run(run::parse_run(rest)?),
        "sessions" => sessions::parse_sessions(rest)?,
        "agents" => sessions::parse_agents(rest)?,
        "image" => image::parse_image(rest)?,
        "decision" => decision::parse_decision(rest)?,
        "roles" => roles::parse_roles(rest)?,
        "approve" => status::parse_approve(rest)?,
        "cancel" => Command::Cancel(common::parse_id(rest, 0)?),
        "status" => Command::Status(common::parse_id(rest, 0)?),
        "bootstrap" => Command::Bootstrap,
        "providers" => providers::parse_providers(rest)?,
        "memory" => memory::parse_memory(rest)?,
        other => {
            return Err(i18n::t_args(
                "Unknown command: {command}",
                &[("command", other.to_string())],
            ));
        }
    };
    Ok((global, command))
}
