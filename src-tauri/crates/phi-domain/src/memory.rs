//! Memory entities: episodes (raw source) and memory records (derived facts).
//!
//! Invariants from `docs/20-domain/domain-model.md` §7:
//! - Invalidation is not deletion: `Invalidated` and `Superseded` records stay
//!   queryable for audit but are excluded from default retrieval.
//! - Every record traces back to an episode and a source trust level.
//! - Attribution (where a fact comes from) and visibility (who may read it)
//!   are separate; visibility is enforced by the host, never by caller input.
//!
//! Timestamps are RFC 3339 UTC strings with millisecond precision, so lexical
//! comparison is also chronological comparison.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a fact came from. Used for attribution and retrieval ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    User,
    Project,
    Agent,
    Run,
}

impl MemoryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryScope::User => "user",
            MemoryScope::Project => "project",
            MemoryScope::Agent => "agent",
            MemoryScope::Run => "run",
        }
    }
}

/// Trust of the original source. Higher ranks are more trustworthy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SourceTrust {
    /// The user said it directly.
    User,
    /// Host or configuration supplied it.
    System,
    /// A tool returned it.
    Tool,
    /// The model inferred it.
    Model,
    /// Unverified external content (web pages, third-party documents).
    External,
}

impl SourceTrust {
    pub fn rank(self) -> u8 {
        match self {
            SourceTrust::User => 4,
            SourceTrust::System => 3,
            SourceTrust::Tool => 2,
            SourceTrust::Model => 1,
            SourceTrust::External => 0,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SourceTrust::User => "user",
            SourceTrust::System => "system",
            SourceTrust::Tool => "tool",
            SourceTrust::Model => "model",
            SourceTrust::External => "external",
        }
    }

    /// External content can never be presented as confirmed knowledge.
    pub fn caps_confidence(self, confidence: f64) -> f64 {
        match self {
            SourceTrust::External => confidence.min(0.5),
            SourceTrust::Model => confidence.min(0.8),
            _ => confidence,
        }
    }
}

/// Lifecycle state of a fact. `Invalidated` and `Superseded` are terminal for
/// retrieval but remain stored for traceability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MemoryState {
    Active,
    Invalidated,
    Superseded,
}

impl MemoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryState::Active => "active",
            MemoryState::Invalidated => "invalidated",
            MemoryState::Superseded => "superseded",
        }
    }
}

/// A derived fact: the source of truth for what the agent believes.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub fact: String,
    pub scope: MemoryScope,
    /// Owner. Every record belongs to exactly one local principal.
    pub user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub source_trust: SourceTrust,
    pub confidence: f64,
    /// When the fact became true in the world.
    #[serde(default)]
    pub valid_from: Option<String>,
    /// When the fact stopped being true in the world.
    #[serde(default)]
    pub valid_to: Option<String>,
    /// When the system learned the fact.
    pub recorded_at: String,
    /// When the system stopped trusting the record (transaction time).
    #[serde(default)]
    pub invalidated_at: Option<String>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Why the record was invalidated or superseded.
    #[serde(default)]
    pub reason: Option<String>,
    pub state: MemoryState,
    #[serde(default)]
    pub episode_id: Option<String>,
    /// True when the stored fact had secrets redacted before persistence.
    #[serde(default)]
    pub redacted: bool,
}

/// Raw source fragment behind a fact. Immutable once written.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub content: String,
    pub source: EpisodeSource,
    #[serde(default)]
    pub source_label: Option<String>,
    pub scope: MemoryScope,
    pub user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub created_at: String,
    /// True when secrets were redacted before persistence.
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeSource {
    UserInput,
    ToolResult,
    ModelOutput,
    External,
}

impl EpisodeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            EpisodeSource::UserInput => "user_input",
            EpisodeSource::ToolResult => "tool_result",
            EpisodeSource::ModelOutput => "model_output",
            EpisodeSource::External => "external",
        }
    }
}

/// Stable reason codes for admission rejection. The message shown to a human
/// is localized separately; the code is data and safe to test against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionRejection {
    Empty,
    TooLong,
    MissingScope,
    InvalidValidity,
    InvalidConfidence,
    Secret,
    Duplicate,
}

impl AdmissionRejection {
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionRejection::Empty => "empty",
            AdmissionRejection::TooLong => "too_long",
            AdmissionRejection::MissingScope => "missing_scope",
            AdmissionRejection::InvalidValidity => "invalid_validity",
            AdmissionRejection::InvalidConfidence => "invalid_confidence",
            AdmissionRejection::Secret => "secret",
            AdmissionRejection::Duplicate => "duplicate",
        }
    }
}

/// Normalized form used for duplicate detection. Case-folded, whitespace
/// collapsed, and stripped. Punctuation is kept: "port 8080" and "port 8080."
/// are the same fact to a human, but stripping punctuation can merge distinct
/// technical strings, so the MVP errs toward keeping records separate.
pub fn normalize_fact(fact: &str) -> String {
    let mut out = String::with_capacity(fact.len());
    let mut pending_space = false;
    for ch in fact.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

/// A fact is meaningful when it contains at least one letter or digit. Pure
/// punctuation, emoji or whitespace carries no retrievable fact.
pub fn has_meaningful_content(fact: &str) -> bool {
    fact.chars().any(|ch| ch.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_case_and_whitespace() {
        assert_eq!(
            normalize_fact("  User\tprefers   Dark   Mode "),
            "user prefers dark mode"
        );
        assert_eq!(normalize_fact("你好　世界"), "你好 世界");
    }

    #[test]
    fn confidence_is_capped_for_untrusted_sources() {
        assert_eq!(SourceTrust::External.caps_confidence(0.9), 0.5);
        assert_eq!(SourceTrust::Model.caps_confidence(0.95), 0.8);
        assert_eq!(SourceTrust::User.caps_confidence(0.9), 0.9);
    }

    #[test]
    fn punctuation_only_facts_are_not_meaningful() {
        assert!(!has_meaningful_content(" --- ... "));
        assert!(has_meaningful_content("uses pnpm"));
        assert!(has_meaningful_content("偏好深色主题"));
    }
}
