//! `providers` command implementations.

use phi_contracts::provider::ProviderInput;
use phi_i18n as i18n;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn list(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&serde_json::json!({
        "type": "providers",
        "providers": node.provider_views(),
        "activeProviderId": node.active_provider_id(),
    }))?;
    Ok(0)
}

pub(super) fn show(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, false)?;
    let view = node
        .provider_view(id)
        .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())]))?;
    print_json(&view)?;
    Ok(0)
}

pub(super) fn add(
    global: &GlobalOptions,
    input: ProviderInput,
    activate: bool,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let view = node.save_provider(input)?;
    if activate {
        node.set_active_provider(&view.id)?;
    }
    print_json(&view)?;
    Ok(0)
}

pub(super) fn update(
    global: &GlobalOptions,
    input: ProviderInput,
    activate: bool,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let view = node.save_provider(input)?;
    if activate {
        node.set_active_provider(&view.id)?;
    }
    print_json(&view)?;
    Ok(0)
}

pub(super) fn remove(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let removed = node.remove_provider(id)?;
    print_json(&serde_json::json!({
        "type": "provider_removed",
        "providerId": id,
        "removed": removed,
    }))?;
    Ok(0)
}

pub(super) fn use_provider(global: &GlobalOptions, id: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    node.set_active_provider(id)?;
    print_json(&serde_json::json!({
        "type": "provider_active",
        "providerId": id,
    }))?;
    Ok(0)
}

pub(super) async fn test(global: &GlobalOptions, id: Option<String>) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let result = node.test_provider(id.as_deref()).await;
    let ok = result.ok;
    print_json(&result)?;
    Ok(if ok { 0 } else { 4 })
}

pub(super) async fn models(global: &GlobalOptions, id: Option<String>) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let models = node.fetch_models(id.as_deref()).await?;
    print_json(&serde_json::json!({
        "type": "models",
        "models": models,
    }))?;
    Ok(0)
}
