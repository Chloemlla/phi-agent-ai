use tauri::State;

use crate::app::AppStateHandle;
use phi_contracts::memory::{
    ForgetReport, ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryHit, MemoryListFilter,
    MemoryQuery, MemoryStats,
};
use phi_contracts::ports::{ModelStatus, RunStatus};
use phi_contracts::provider::{ModelSpec, ProviderInput, ProviderTestView, ProviderView};
use phi_domain::memory::{MemoryRecord, MemoryScope, SourceTrust};
use phi_domain::view::SessionSummary;
use phi_i18n as i18n;
use phi_node::{Bootstrap, SendResult, SessionDetail};

#[tauri::command]
pub fn get_bootstrap(state: State<'_, AppStateHandle>) -> Result<Bootstrap, String> {
    state.0.bootstrap()
}

#[tauri::command]
pub fn set_language(state: State<'_, AppStateHandle>, language: String) -> Result<String, String> {
    state.0.set_language(&language)
}

#[tauri::command]
pub fn set_workspace(state: State<'_, AppStateHandle>, path: String) -> Result<String, String> {
    state.0.set_workspace(&path)
}

#[tauri::command]
pub fn create_session(
    state: State<'_, AppStateHandle>,
    title: Option<String>,
) -> Result<SessionSummary, String> {
    state.0.create_session(title.as_deref())
}

#[tauri::command]
pub fn get_session(
    state: State<'_, AppStateHandle>,
    session_id: String,
) -> Result<SessionDetail, String> {
    state.0.get_session(&session_id)
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppStateHandle>,
    session_id: String,
    text: String,
    provider_id: Option<String>,
    workbench: Option<bool>,
    model: Option<String>,
    thinking: Option<String>,
) -> Result<SendResult, String> {
    let run_id = state.0.send_message_full(
        &session_id,
        &text,
        provider_id.as_deref(),
        workbench.unwrap_or(false),
        model.as_deref(),
        thinking.as_deref(),
    )?;
    Ok(SendResult { run_id })
}

#[tauri::command]
pub fn approve(
    state: State<'_, AppStateHandle>,
    approval_id: String,
    approved: bool,
    note: Option<String>,
) -> Result<(), String> {
    state.0.approve(&approval_id, approved, note)
}

#[tauri::command]
pub fn cancel_run(state: State<'_, AppStateHandle>, run_id: String) -> Result<bool, String> {
    Ok(state.0.cancel_run(&run_id))
}

#[tauri::command]
pub fn set_api_key(state: State<'_, AppStateHandle>, key: String) -> Result<ModelStatus, String> {
    state.0.set_api_key(&key)
}

#[tauri::command]
pub fn get_model_status(state: State<'_, AppStateHandle>) -> Result<ModelStatus, String> {
    Ok(state.0.model_status())
}

#[tauri::command]
pub fn get_run_status(state: State<'_, AppStateHandle>, run_id: String) -> Option<RunStatus> {
    state.0.run_status(&run_id)
}

// ─────────────────────────── providers (settings surface) ───────────────────

#[tauri::command]
pub fn list_providers(state: State<'_, AppStateHandle>) -> Vec<ProviderView> {
    state.0.provider_views()
}

#[tauri::command]
pub fn save_provider(
    state: State<'_, AppStateHandle>,
    input: ProviderInput,
) -> Result<ProviderView, String> {
    state.0.save_provider(input)
}

#[tauri::command]
pub fn remove_provider(
    state: State<'_, AppStateHandle>,
    provider_id: String,
) -> Result<bool, String> {
    state.0.remove_provider(&provider_id)
}

#[tauri::command]
pub fn set_active_provider(
    state: State<'_, AppStateHandle>,
    provider_id: String,
) -> Result<(), String> {
    state.0.set_active_provider(&provider_id)
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppStateHandle>,
    provider_id: Option<String>,
) -> Result<ProviderTestView, String> {
    Ok(state.0.test_provider(provider_id.as_deref()).await)
}

#[tauri::command]
pub async fn fetch_models(
    state: State<'_, AppStateHandle>,
    provider_id: Option<String>,
) -> Result<Vec<String>, String> {
    state.0.fetch_models(provider_id.as_deref()).await
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
pub fn describe_models(state: State<'_, AppStateHandle>, model_ids: Vec<String>) -> Vec<ModelSpec> {
    state.0.describe_models(&model_ids)
}

// ─────────────────────────── memory (settings surface) ──────────────────────

#[tauri::command]
pub fn list_memories(
    state: State<'_, AppStateHandle>,
    text: Option<String>,
) -> Result<Vec<MemoryRecord>, String> {
    state.0.memory_list(MemoryListFilter {
        text: text.filter(|value| !value.trim().is_empty()),
        limit: 200,
        ..Default::default()
    })
}

#[tauri::command]
pub fn search_memories(
    state: State<'_, AppStateHandle>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<MemoryHit>, String> {
    state.0.memory_search(MemoryQuery {
        text: query,
        limit: limit.unwrap_or(10).clamp(1, 50),
        ..Default::default()
    })
}

#[tauri::command]
pub fn get_memory_stats(state: State<'_, AppStateHandle>) -> Result<MemoryStats, String> {
    state.0.memory_stats()
}

#[tauri::command]
pub fn add_memory(
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
    state
        .0
        .memory_add(MemoryAddRequest::new(fact, scope, SourceTrust::User))
}

#[tauri::command]
pub fn forget_memories(
    state: State<'_, AppStateHandle>,
    ids: Vec<String>,
) -> Result<ForgetReport, String> {
    if ids.is_empty() {
        return Err(i18n::t("No memory ids were provided"));
    }
    state
        .0
        .memory_forget(ForgetSelector::ids(ids), &i18n::t("Deleted from settings"))
}
