//! `memory` command implementations.

use phi_contracts::memory::{
    ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryListFilter, MemoryQuery,
};
use phi_i18n as i18n;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) async fn maintain(
    global: &GlobalOptions,
    project_id: Option<String>,
    dry_run: bool,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
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

pub(super) fn list(global: &GlobalOptions, filter: MemoryListFilter) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let memories = node.memory_list(filter)?;
    print_json(&serde_json::json!({
        "type": "memories",
        "memories": memories,
    }))?;
    Ok(0)
}

pub(super) fn search(global: &GlobalOptions, query: MemoryQuery) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let text = query.text.clone();
    let hits = node.memory_search(query)?;
    print_json(&serde_json::json!({
        "type": "memory_search",
        "query": text,
        "hits": hits,
    }))?;
    Ok(0)
}

pub(super) fn show(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let record = node
        .memory_get(id)?
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

pub(super) fn add(global: &GlobalOptions, request: MemoryAddRequest) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let outcome = node.memory_add(request)?;
    print_json(&outcome)?;
    Ok(memory_outcome_exit_code(&outcome))
}

pub(super) fn invalidate(global: &GlobalOptions, id: &str, reason: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    node.memory_invalidate(id, reason)?;
    print_json(&serde_json::json!({
        "type": "memory_invalidated",
        "id": id,
        "reason": reason,
    }))?;
    Ok(0)
}

pub(super) fn correct(
    global: &GlobalOptions,
    id: &str,
    fact: &str,
    reason: &str,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let outcome = node.memory_correct(id, fact, reason)?;
    print_json(&outcome)?;
    Ok(memory_outcome_exit_code(&outcome))
}

pub(super) fn supersede(
    global: &GlobalOptions,
    id: &str,
    request: MemoryAddRequest,
    reason: &str,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let outcome = node.memory_supersede(id, request, reason)?;
    print_json(&outcome)?;
    Ok(memory_outcome_exit_code(&outcome))
}

pub(super) fn forget(
    global: &GlobalOptions,
    selector: ForgetSelector,
    reason: Option<String>,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let report = node.memory_forget(selector, reason.as_deref().unwrap_or(""))?;
    print_json(&serde_json::json!({
        "type": "memory_forgotten",
        "records": report.records,
        "episodes": report.episodes,
    }))?;
    Ok(0)
}

pub(super) fn stats(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let stats = node.memory_stats()?;
    print_json(&serde_json::json!({
        "type": "memory_stats",
        "stats": stats,
    }))?;
    Ok(0)
}

fn memory_outcome_exit_code(outcome: &MemoryAddOutcome) -> i32 {
    match outcome {
        MemoryAddOutcome::Rejected { .. } => 4,
        _ => 0,
    }
}
