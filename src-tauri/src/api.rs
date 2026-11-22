use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::app::AppStateHandle;
use phi_contracts::auxiliary::ImageAsset;
use phi_contracts::memory::{
    ForgetReport, ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryHit, MemoryListFilter,
    MemoryQuery, MemoryStats,
};
use phi_contracts::provider::{ModelSpec, ProviderInput, ProviderTestView, ProviderView};
use phi_domain::memory::{MemoryRecord, MemoryScope, SourceTrust};
use phi_domain::view::SessionSummary;
use phi_i18n as i18n;
use phi_node::{Bootstrap, Node, SendResult, SessionDetail};

/// Run disk- or database-heavy node work on the blocking pool. Tauri executes
/// synchronous commands on the native event loop, so commands that touch the
/// journal, SQLite, file I/O or whole-session serialization go through here.
async fn blocking<T, F>(state: &AppStateHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Node) -> Result<T, String> + Send + 'static,
{
    let node = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || work(&node))
        .await
        .map_err(|error| {
            i18n::t_args(
                "Background task failed: {error}",
                &[("error", error.to_string())],
            )
        })?
}

/// `blocking` for commands whose return type is a plain value. Those commands
/// cannot borrow `State` across the await point (Tauri requires their future to
/// be `'static`), so they inject an `AppHandle` and hand over an owned node.
async fn blocking_value<T, F>(node: Arc<Node>, work: F) -> T
where
    T: Send + 'static,
    F: FnOnce(&Node) -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || work(&node))
        .await
        .expect("blocking command task failed")
}

#[tauri::command]
pub async fn get_bootstrap(state: State<'_, AppStateHandle>) -> Result<Bootstrap, String> {
    blocking(&state, |node| node.bootstrap()).await
}

#[tauri::command]
pub async fn set_language(
    state: State<'_, AppStateHandle>,
    language: String,
) -> Result<String, String> {
    blocking(&state, move |node| node.set_language(&language)).await
}

#[tauri::command]
pub async fn set_workspace(
    state: State<'_, AppStateHandle>,
    path: String,
) -> Result<String, String> {
    blocking(&state, move |node| node.set_workspace(&path)).await
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppStateHandle>,
    title: Option<String>,
) -> Result<SessionSummary, String> {
    blocking(&state, move |node| node.create_session(title.as_deref())).await
}

#[tauri::command]
pub async fn delete_session(
    state: State<'_, AppStateHandle>,
    session_id: String,
) -> Result<bool, String> {
    blocking(&state, move |node| node.delete_session(&session_id)).await
}

#[tauri::command]
pub async fn rename_session(
    state: State<'_, AppStateHandle>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    blocking(&state, move |node| node.rename_session(&session_id, &title)).await
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppStateHandle>,
    session_id: String,
) -> Result<SessionDetail, String> {
    blocking(&state, move |node| node.get_session(&session_id)).await
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppStateHandle>,
    session_id: String,
    text: String,
    provider_id: Option<String>,
    model: Option<String>,
    thinking: Option<String>,
    attachment_ids: Option<Vec<String>>,
) -> Result<SendResult, String> {
    let run_id = blocking(&state, move |node| {
        node.send_message_full(
            &session_id,
            &text,
            provider_id.as_deref(),
            model.as_deref(),
            thinking.as_deref(),
            attachment_ids.as_deref().unwrap_or(&[]),
        )
    })
    .await?;
    Ok(SendResult { run_id })
}

#[tauri::command]
pub async fn add_image_attachment(
    state: State<'_, AppStateHandle>,
    session_id: Option<String>,
    path: String,
) -> Result<ImageAsset, String> {
    blocking(&state, move |node| {
        node.add_image_attachment(Path::new(&path), session_id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn image_asset_data_url(
    state: State<'_, AppStateHandle>,
    asset_id: String,
) -> Result<String, String> {
    blocking(&state, move |node| node.image_asset_data_url(&asset_id)).await
}

#[tauri::command]
pub async fn approve(
    state: State<'_, AppStateHandle>,
    approval_id: String,
    approved: bool,
    note: Option<String>,
) -> Result<(), String> {
    blocking(&state, move |node| {
        node.approve(&approval_id, approved, note)
    })
    .await
}

#[tauri::command]
pub async fn cancel_run(state: State<'_, AppStateHandle>, run_id: String) -> Result<bool, String> {
    blocking(&state, move |node| Ok(node.cancel_run(&run_id))).await
}

// ─────────────────────────── providers (settings surface) ───────────────────

#[tauri::command]
pub async fn list_providers(app: AppHandle) -> Vec<ProviderView> {
    let node = app.state::<AppStateHandle>().0.clone();
    blocking_value(node, |node| node.provider_views()).await
}

#[tauri::command]
pub async fn save_provider(
    state: State<'_, AppStateHandle>,
    input: ProviderInput,
) -> Result<ProviderView, String> {
    blocking(&state, move |node| node.save_provider(input)).await
}

#[tauri::command]
pub async fn remove_provider(
    state: State<'_, AppStateHandle>,
    provider_id: String,
) -> Result<bool, String> {
    blocking(&state, move |node| node.remove_provider(&provider_id)).await
}

#[tauri::command]
pub async fn set_active_provider(
    state: State<'_, AppStateHandle>,
    provider_id: String,
) -> Result<(), String> {
    blocking(&state, move |node| node.set_active_provider(&provider_id)).await
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppStateHandle>,
    provider_id: Option<String>,
) -> Result<ProviderTestView, String> {
    Ok(state.0.test_provider(provider_id.as_deref()).await)
}

/// Preview a provider's live model list without persisting it. The settings
/// wizard uses this before the provider is saved.
#[tauri::command]
pub async fn preview_models(
    state: State<'_, AppStateHandle>,
    provider_id: Option<String>,
) -> Result<Vec<String>, String> {
    state.0.preview_models(provider_id.as_deref()).await
}

/// Catalogue suggestions for the settings page. Pure metadata; saving the
/// provider is what persists them.
#[tauri::command]
pub async fn describe_models(app: AppHandle, model_ids: Vec<String>) -> Vec<ModelSpec> {
    let node = app.state::<AppStateHandle>().0.clone();
    blocking_value(node, move |node| node.describe_models(&model_ids)).await
}

// ─────────────────────────── memory (settings surface) ──────────────────────

#[tauri::command]
pub async fn list_memories(
    state: State<'_, AppStateHandle>,
    text: Option<String>,
) -> Result<Vec<MemoryRecord>, String> {
    let filter = MemoryListFilter {
        text: text.filter(|value| !value.trim().is_empty()),
        limit: 200,
        ..Default::default()
    };
    blocking(&state, move |node| node.memory_list(filter)).await
}

#[tauri::command]
pub async fn search_memories(
    state: State<'_, AppStateHandle>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<MemoryHit>, String> {
    let query = MemoryQuery {
        text: query,
        limit: limit.unwrap_or(10).clamp(1, 50),
        ..Default::default()
    };
    blocking(&state, move |node| node.memory_search(query)).await
}

#[tauri::command]
pub async fn get_memory_stats(state: State<'_, AppStateHandle>) -> Result<MemoryStats, String> {
    blocking(&state, |node| node.memory_stats()).await
}

#[tauri::command]
pub async fn add_memory(
    state: State<'_, AppStateHandle>,
    fact: String,
    scope: String,
) -> Result<MemoryAddOutcome, String> {
    let scope = match scope.as_str() {
        "user" => MemoryScope::User,
        "project" => MemoryScope::Project,
        other => {
            return Err(i18n::t_args(
                "Unknown memory scope: {scope}",
                &[("scope", other.to_string())],
            ));
        }
    };
    blocking(&state, move |node| {
        node.memory_add(MemoryAddRequest::new(fact, scope, SourceTrust::User))
    })
    .await
}

#[tauri::command]
pub async fn forget_memories(
    state: State<'_, AppStateHandle>,
    ids: Vec<String>,
) -> Result<ForgetReport, String> {
    if ids.is_empty() {
        return Err(i18n::t("No memory ids were provided"));
    }
    blocking(&state, move |node| {
        node.memory_forget(ForgetSelector::ids(ids), &i18n::t("Deleted from settings"))
    })
    .await
}
