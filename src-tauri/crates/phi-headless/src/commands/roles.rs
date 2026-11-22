//! `roles` command implementations.

use phi_contracts::auxiliary::AuxiliaryRoleInput;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn list(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&serde_json::json!({
        "type": "roles",
        "roles": node.auxiliary_roles(),
    }))?;
    Ok(0)
}

pub(super) fn set(
    global: &GlobalOptions,
    role: &str,
    input: AuxiliaryRoleInput,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    print_json(&node.set_auxiliary_role(role, input)?)?;
    Ok(0)
}

pub(super) fn clear(global: &GlobalOptions, role: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let removed = node.clear_auxiliary_role(role)?;
    print_json(&serde_json::json!({
        "type": "role_cleared",
        "role": role,
        "removed": removed,
    }))?;
    Ok(0)
}
