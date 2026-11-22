//! Approval, cancellation, status and bootstrap commands.

use phi_adapters::control;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn approve(
    global: &GlobalOptions,
    id: String,
    approved: bool,
    note: Option<String>,
) -> Result<i32, String> {
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

pub(super) fn cancel(global: &GlobalOptions, run_id: String) -> Result<i32, String> {
    control::request_cancel(&global.data_dir, &run_id)?;
    print_json(&serde_json::json!({
        "type": "cancel",
        "runId": run_id,
        "cancelled": true,
        "queued": true,
    }))?;
    Ok(0)
}

pub(super) fn status(global: &GlobalOptions, run_id: String) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let status = node.run_status(&run_id);
    print_json(&serde_json::json!({
        "type": "run_status",
        "runId": run_id,
        "found": status.is_some(),
        "status": status,
    }))?;
    Ok(0)
}

pub(super) fn bootstrap(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, true)?;
    print_json(&node.bootstrap()?)?;
    Ok(0)
}
