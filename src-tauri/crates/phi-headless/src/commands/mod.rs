//! Command dispatch for the headless shell.

mod decision;
mod image;
mod memory;
mod providers;
mod roles;
mod run;
mod sessions;
mod status;
mod support;

use crate::cli::{Command, GlobalOptions};

pub(crate) async fn execute(global: GlobalOptions, command: Command) -> Result<i32, String> {
    match command {
        Command::Help => {
            crate::cli::print_help();
            Ok(0)
        }
        Command::Run(args) => run::run(global, args).await,
        Command::SessionsList => sessions::list(&global),
        Command::SessionsShow(id) => sessions::show(&global, &id),
        Command::SessionsDelete(id) => sessions::delete(&global, &id),
        Command::AgentsList(session_id) => sessions::agents_list(&global, &session_id),
        Command::AgentsShow(agent_id) => sessions::agents_show(&global, &agent_id),
        Command::ImageConfigure(input) => image::configure(&global, input),
        Command::ImageShow => image::show(&global),
        Command::ImageGenerate {
            prompt,
            session_id,
            size,
            count,
            out,
        } => image::generate(&global, prompt, session_id, size, count, out).await,
        Command::ImageList { session_id } => image::list(&global, session_id),
        Command::ImageDelete(asset_id) => image::delete(&global, &asset_id),
        Command::DecisionConfigure(input) => decision::configure(&global, input),
        Command::DecisionShow => decision::show(&global),
        Command::DecisionTest { state } => decision::test(&global, &state).await,
        Command::RolesList => roles::list(&global),
        Command::RolesSet { role, input } => roles::set(&global, &role, input),
        Command::RolesClear(role) => roles::clear(&global, &role),
        Command::MemoryMaintain {
            project_id,
            dry_run,
        } => memory::maintain(&global, project_id, dry_run).await,
        Command::Approve { id, approved, note } => status::approve(&global, id, approved, note),
        Command::Cancel(run_id) => status::cancel(&global, run_id),
        Command::Status(run_id) => status::status(&global, run_id),
        Command::Bootstrap => status::bootstrap(&global),
        Command::ProvidersList => providers::list(&global),
        Command::ProvidersShow(id) => providers::show(&global, &id),
        Command::ProvidersAdd { input, activate } => providers::add(&global, input, activate),
        Command::ProvidersUpdate { input, activate } => providers::update(&global, input, activate),
        Command::ProvidersRemove(id) => providers::remove(&global, &id),
        Command::ProvidersUse(id) => providers::use_provider(&global, &id),
        Command::ProvidersTest(id) => providers::test(&global, id).await,
        Command::ProvidersModels(id) => providers::models(&global, id).await,
        Command::MemoryList(filter) => memory::list(&global, filter),
        Command::MemorySearch(query) => memory::search(&global, query),
        Command::MemoryShow(id) => memory::show(&global, &id),
        Command::MemoryAdd(request) => memory::add(&global, request),
        Command::MemoryInvalidate { id, reason } => memory::invalidate(&global, &id, &reason),
        Command::MemoryCorrect { id, fact, reason } => {
            memory::correct(&global, &id, &fact, &reason)
        }
        Command::MemorySupersede {
            id,
            request,
            reason,
        } => memory::supersede(&global, &id, request, &reason),
        Command::MemoryForget { selector, reason } => memory::forget(&global, selector, reason),
        Command::MemoryStats => memory::stats(&global),
    }
}
