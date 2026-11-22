//! `decision` command implementations.

use phi_contracts::auxiliary::DecisionConfigInput;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn configure(global: &GlobalOptions, input: DecisionConfigInput) -> Result<i32, String> {
    let node = open_node(global, true)?;
    print_json(&node.configure_decision(input)?)?;
    Ok(0)
}

pub(super) fn show(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&node.decision_status())?;
    Ok(0)
}

pub(super) async fn test(global: &GlobalOptions, state: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    print_json(&node.test_decision(state).await?)?;
    Ok(0)
}
