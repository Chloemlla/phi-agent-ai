//! Helpers shared by command implementations.

use std::sync::Arc;

use phi_node::{Node, NodeConfig, compose};

use crate::cli::GlobalOptions;

pub(super) fn open_node(global: &GlobalOptions, writable: bool) -> Result<Arc<Node>, String> {
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

pub(super) fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
    println!("{text}");
    Ok(())
}
