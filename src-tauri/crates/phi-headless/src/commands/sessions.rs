//! `sessions` and `agents` command implementations.

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn list(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let sessions = node.list_sessions();
    print_json(&serde_json::json!({
        "type": "sessions",
        "sessions": sessions,
    }))?;
    Ok(0)
}

pub(super) fn show(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&node.get_session(id)?)?;
    Ok(0)
}

pub(super) fn delete(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let deleted = node.delete_session(id)?;
    print_json(&serde_json::json!({
        "type": "session-deleted",
        "sessionId": id,
        "deleted": deleted,
    }))?;
    Ok(0)
}

pub(super) fn agents_list(global: &GlobalOptions, session_id: &str) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&serde_json::json!({
        "type": "agents",
        "sessionId": session_id,
        "agents": node.agents(session_id),
    }))?;
    Ok(0)
}

pub(super) fn agents_show(global: &GlobalOptions, agent_id: &str) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let agent = node.agent(agent_id);
    let messages = node.agent_messages(agent_id);
    print_json(&serde_json::json!({
        "type": "agent",
        "agent": agent,
        "messages": messages,
    }))?;
    Ok(0)
}
