//! Optional model surfaces beyond the chat gateway: image generation, typed
//! decisions, and auxiliary text roles (session titles, memory maintenance,
//! summaries).
//!
//! All three are *service calls*: they never create a Turn, Run or Agent
//! instance. They are optional ports; a node without them degrades honestly.

use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

// ─────────────────────────── image generation ───────────────────────────

/// Full replacement input for `image.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageConfigInput {
    pub enabled: bool,
    /// `openai_images` or `gemini_images`.
    pub provider: String,
    pub base_url: String,
    /// Credential reference; `None` keeps the existing key.
    pub api_key: Option<String>,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub format: String,
    pub max_per_turn: u32,
    pub max_bytes: u64,
    pub egress_granted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStatus {
    pub configured: bool,
    pub enabled: bool,
    pub ready: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub format: String,
    pub max_per_turn: u32,
    pub egress_granted: bool,
    /// Localized reason when the surface is not ready.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRequest {
    pub prompt: String,
    pub size: Option<String>,
    pub count: u32,
    #[serde(default)]
    pub reference_assets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageAsset {
    pub asset_id: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub model: String,
    pub prompt_preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageUsage {
    pub images: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResponse {
    pub assets: Vec<ImageAsset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
}

/// Optional image backend. Implementations own their endpoint, credential and
/// asset storage; the model is always chosen by the user, never by an agent.
#[async_trait]
pub trait ImagePort: Send + Sync {
    fn status(&self) -> ImageStatus;
    fn configure(&self, input: ImageConfigInput) -> Result<ImageStatus, String>;
    async fn generate(
        &self,
        request: ImageRequest,
        session_id: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ImageResponse, String>;
    fn list(&self, session_id: Option<&str>) -> Vec<ImageAsset>;
    fn delete(&self, asset_id: &str) -> Result<bool, String>;
    fn bytes(&self, asset_id: &str) -> Result<Vec<u8>, String>;
    /// Metadata for an existing asset.
    fn asset(&self, asset_id: &str) -> Result<ImageAsset, String>;
    /// Copy a user-selected image into the asset store. The image plugin owns
    /// format sniffing and size limits so every shell uses the same rules.
    fn store_upload(&self, source: &Path, session_id: Option<&str>) -> Result<ImageAsset, String>;
    /// A `data:` URL suitable for an `<img>` preview. Bytes stay local.
    fn data_url(&self, asset_id: &str) -> Result<String, String>;
}

// ─────────────────────────── typed decisions ───────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DecisionConfigInput {
    pub enabled: bool,
    /// Full decisions endpoint URL supplied by the user.
    pub endpoint: String,
    /// Credential reference; `None` keeps the existing key.
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_ms: u64,
    pub triage_enabled: bool,
    pub triage_threshold: f32,
    pub triage_tools: Vec<String>,
    pub compaction_enabled: bool,
    pub compaction_threshold: f32,
    pub compaction_min_chars: usize,
    pub egress_granted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionStatus {
    pub configured: bool,
    pub enabled: bool,
    pub ready: bool,
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
    pub triage_enabled: bool,
    pub triage_threshold: f32,
    pub triage_tools: Vec<String>,
    pub compaction_enabled: bool,
    pub compaction_threshold: f32,
    pub compaction_min_chars: usize,
    pub egress_granted: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    Noul {
        instructions: String,
    },
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRequest {
    pub point: String,
    pub state: String,
    pub questions: BTreeMap<String, DecisionQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Noul {
        probability: f32,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f32>,
        confidence: f32,
    },
    Score {
        score: f32,
        distribution: Vec<f32>,
        confidence: f32,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionResponse {
    pub call_id: String,
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug)]
pub struct TriageConfig {
    pub enabled: bool,
    pub threshold: f32,
    pub tools: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub threshold: f32,
    pub min_chars: usize,
}

/// Typed judgment layer. Implementations must not generate prose or execute
/// tools; they return calibrated probabilities for code to branch on.
#[async_trait]
pub trait DecisionPort: Send + Sync {
    fn status(&self) -> DecisionStatus;
    fn configure(&self, input: DecisionConfigInput) -> Result<DecisionStatus, String>;
    fn triage(&self) -> TriageConfig;
    fn compaction(&self) -> CompactionConfig;
    async fn evaluate(
        &self,
        request: DecisionRequest,
        cancel: CancellationToken,
    ) -> Result<DecisionResponse, String>;
}

// ─────────────────────────── auxiliary text roles ───────────────────────────

pub const AUX_SESSION_TITLE: &str = "session_title";
pub const AUX_MEMORY_MAINTENANCE: &str = "memory_maintenance";
pub const AUX_SUMMARIZER: &str = "summarizer";

pub const AUX_ROLES: &[&str] = &[AUX_SESSION_TITLE, AUX_MEMORY_MAINTENANCE, AUX_SUMMARIZER];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuxiliaryRoleInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    pub egress_granted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxiliaryRoleView {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    pub egress_granted: bool,
    pub configured: bool,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceProposalView {
    pub op: String,
    pub ids: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceReport {
    pub trigger: String,
    pub dry_run: bool,
    pub proposals: u64,
    pub applied: u64,
    pub pending: Vec<MaintenanceProposalView>,
    #[serde(default)]
    pub skipped: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettleReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<MaintenanceReport>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait AuxiliaryPort: Send + Sync {
    fn roles(&self) -> Vec<AuxiliaryRoleView>;
    fn set_role(&self, role: &str, input: AuxiliaryRoleInput) -> Result<AuxiliaryRoleView, String>;
    fn clear_role(&self, role: &str) -> Result<bool, String>;
    fn title_ready(&self) -> bool;
    fn maintenance_ready(&self) -> bool;
    fn summarizer_ready(&self) -> bool;
    /// Suggest a title for a session whose title is still the default.
    async fn suggest_title(
        &self,
        session_id: &str,
        cancel: CancellationToken,
    ) -> Result<Option<String>, String>;
    /// Summarize text with the `summarizer` role. `None` when unbound.
    async fn summarize(
        &self,
        text: &str,
        max_chars: usize,
        cancel: CancellationToken,
    ) -> Result<Option<String>, String>;
    fn maintenance_due(&self, project_id: Option<&str>) -> bool;
    async fn maintain_memory(
        &self,
        project_id: Option<String>,
        dry_run: bool,
        trigger: &str,
        cancel: CancellationToken,
    ) -> Result<MaintenanceReport, String>;
}
