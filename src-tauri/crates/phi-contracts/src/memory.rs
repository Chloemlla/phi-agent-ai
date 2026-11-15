//! Memory contracts: the host-provided principal, the thin memory interface
//! promised by `docs/40-runtime/context-and-memory.md` §7, and the storage
//! port implemented by adapters.
//!
//! The caller never supplies a scope that can widen access: `Principal` is
//! built by the host (node), and `search`/`list` only ever see records the
//! principal is allowed to read.

use serde::{Deserialize, Serialize};

use phi_domain::memory::{
    AdmissionRejection, Episode, EpisodeSource, MemoryRecord, MemoryScope, MemoryState, SourceTrust,
};

/// Host-derived identity of the current caller. Constructed by the shell, never
/// accepted from plugin or model input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Principal {
    pub user_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
}

impl Principal {
    pub fn local(user_id: impl Into<String>, project_id: Option<String>) -> Self {
        Self {
            user_id: user_id.into(),
            project_id,
            agent_id: Some("main".to_string()),
            run_id: None,
        }
    }

    pub fn in_run(&self, run_id: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.run_id = Some(run_id.into());
        next
    }
}

/// Retrieval request. `scopes` can only narrow what the principal may read.
#[derive(Clone, Debug)]
pub struct MemoryQuery {
    pub text: String,
    pub limit: usize,
    /// Empty means every scope the principal can see.
    pub scopes: Vec<MemoryScope>,
    /// Minimum BM25 score. Zero keeps any record that matched a query term.
    pub min_score: f64,
    /// Include invalidated/superseded records (audit and traceability views).
    pub include_inactive: bool,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            limit: 5,
            scopes: Vec::new(),
            min_score: 0.0,
            include_inactive: false,
        }
    }
}

/// Owner-level listing used by settings and the CLI. The user id is filled from
/// the principal; filters can only narrow the owner's own records.
#[derive(Clone, Debug, Default)]
pub struct MemoryListFilter {
    pub user_id: String,
    /// Substring search over the fact text (SQL LIKE, case-insensitive for ASCII).
    pub text: Option<String>,
    pub scopes: Vec<MemoryScope>,
    pub states: Vec<MemoryState>,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub total: u64,
    pub active: u64,
    pub invalidated: u64,
    pub superseded: u64,
    pub user_scope: u64,
    pub project_scope: u64,
    pub agent_scope: u64,
    pub run_scope: u64,
    pub episodes: u64,
    pub redacted: u64,
}

/// Optional raw source attached to a new fact.
#[derive(Clone, Debug)]
pub struct MemoryEpisodeInput {
    pub content: String,
    pub source: EpisodeSource,
    pub source_label: Option<String>,
    pub session_id: Option<String>,
}

/// A candidate fact. The scope selects which attribution fields the host fills
/// from the principal; callers cannot name another user or project.
#[derive(Clone, Debug)]
pub struct MemoryAddRequest {
    pub fact: String,
    pub scope: MemoryScope,
    pub source_trust: SourceTrust,
    pub confidence: Option<f64>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub episode: Option<MemoryEpisodeInput>,
    /// Optional human-readable note kept with the record.
    pub reason: Option<String>,
}

impl MemoryAddRequest {
    pub fn new(fact: impl Into<String>, scope: MemoryScope, source_trust: SourceTrust) -> Self {
        Self {
            fact: fact.into(),
            scope,
            source_trust,
            confidence: None,
            valid_from: None,
            valid_to: None,
            episode: None,
            reason: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MemoryAddOutcome {
    Admitted {
        record_id: String,
        redacted: bool,
    },
    Duplicate {
        record_id: String,
    },
    Rejected {
        candidate_id: String,
        reason: AdmissionRejection,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub record: MemoryRecord,
    pub score: f64,
    #[serde(default)]
    pub matched_terms: Vec<String>,
}

/// Which records `forget` removes. An empty selector without `all` is refused:
/// deleting everything must be an explicit act. Every selector is intersected
/// with the principal's user id.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetSelector {
    /// Explicitly forget everything the principal owns.
    #[serde(default)]
    pub all: bool,
    pub ids: Vec<String>,
    pub scopes: Vec<MemoryScope>,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
}

impl ForgetSelector {
    pub fn ids(ids: Vec<String>) -> Self {
        Self {
            ids,
            ..Default::default()
        }
    }

    pub fn all() -> Self {
        Self {
            all: true,
            ..Default::default()
        }
    }

    pub fn has_selector(&self) -> bool {
        !self.ids.is_empty()
            || !self.scopes.is_empty()
            || self.project_id.is_some()
            || self.run_id.is_some()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetReport {
    pub records: u64,
    pub episodes: u64,
}

/// Retrieval filter derived from a `Principal`. Visibility is encoded here,
/// in one place, and enforced by the store query.
#[derive(Clone, Debug)]
pub struct MemoryFilter {
    pub user_id: String,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub run_id: Option<String>,
    /// Narrowing only; empty means all visible scopes.
    pub scopes: Vec<MemoryScope>,
    pub include_inactive: bool,
}

/// The thin lifecycle interface promised by the design. Implemented by the
/// memory plugin; shells talk to it through `Services`.
///
/// Two visibility levels apply:
/// - `search` enforces execution-scope visibility derived from the principal
///   (user, project, agent, run).
/// - `list`, `get`, `episode`, `invalidate`, `supersede`, `correct` and
///   `forget` are host owner-management operations. They authorize on
///   `principal.user_id` and may inspect every memory the owner owns, across
///   projects and runs; the owner's settings/CLI surface is allowed to manage
///   what a run cannot see from another scope.
pub trait MemoryPort: Send + Sync {
    fn add(
        &self,
        principal: &Principal,
        request: MemoryAddRequest,
    ) -> Result<MemoryAddOutcome, String>;
    fn search(&self, principal: &Principal, query: &MemoryQuery) -> Result<Vec<MemoryHit>, String>;
    fn list(
        &self,
        principal: &Principal,
        filter: &MemoryListFilter,
    ) -> Result<Vec<MemoryRecord>, String>;
    fn get(&self, principal: &Principal, id: &str) -> Result<Option<MemoryRecord>, String>;
    fn episode(&self, principal: &Principal, id: &str) -> Result<Option<Episode>, String>;
    fn invalidate(&self, principal: &Principal, id: &str, reason: &str) -> Result<(), String>;
    fn supersede(
        &self,
        principal: &Principal,
        old_id: &str,
        request: MemoryAddRequest,
        reason: &str,
    ) -> Result<MemoryAddOutcome, String>;
    fn correct(
        &self,
        principal: &Principal,
        record_id: &str,
        corrected_fact: &str,
        reason: &str,
    ) -> Result<MemoryAddOutcome, String>;
    fn forget(
        &self,
        principal: &Principal,
        selector: ForgetSelector,
        reason: &str,
    ) -> Result<ForgetReport, String>;
    fn stats(&self, principal: &Principal) -> Result<MemoryStats, String>;
}

/// Storage port. The SQLite adapter implements this; tokenization and admission
/// stay in the plugin. Term-level access is deliberate: the backend owns BM25
/// scoring so a future vector backend can be swapped in without touching the
/// service layer.
pub trait MemoryStore: Send + Sync {
    /// Atomically persist a record, its search terms and the optional episode.
    fn insert(
        &self,
        record: &MemoryRecord,
        normalized_fact: &str,
        terms: &[String],
        episode: Option<&Episode>,
    ) -> Result<(), String>;

    /// Find an active record with the same normalized fact and attribution.
    fn find_duplicate(
        &self,
        normalized_fact: &str,
        record: &MemoryRecord,
    ) -> Result<Option<MemoryRecord>, String>;

    /// BM25 retrieval over records the filter permits. `now` is an RFC 3339
    /// timestamp used to exclude facts outside their valid interval.
    fn search(
        &self,
        terms: &[String],
        filter: &MemoryFilter,
        now: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, String>;

    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, String>;
    fn episode(&self, id: &str) -> Result<Option<Episode>, String>;
    fn list(&self, filter: &MemoryListFilter) -> Result<Vec<MemoryRecord>, String>;

    /// Mark a record invalidated. Returns false when the record does not exist.
    fn invalidate(&self, id: &str, at: &str, reason: &str) -> Result<bool, String>;

    /// Insert the replacement and mark the old record superseded, atomically.
    fn supersede(
        &self,
        old_id: &str,
        new_record: &MemoryRecord,
        normalized_fact: &str,
        terms: &[String],
        episode: Option<&Episode>,
        at: &str,
        reason: &str,
    ) -> Result<(), String>;

    /// Delete matching records, their search terms and linked episodes, plus
    /// episodes that belong to the same attribution scope. Returns counts.
    fn forget(&self, user_id: &str, selector: &ForgetSelector) -> Result<ForgetReport, String>;

    fn stats(&self, user_id: &str) -> Result<MemoryStats, String>;

    /// Health probe used by plugin activation.
    fn ping(&self) -> Result<(), String>;
}

/// Honest null backend: reads return nothing, writes fail. A missing memory
/// backend must degrade retrieval, never silently pretend to remember.
#[derive(Default)]
pub struct NullMemoryStore;

impl MemoryStore for NullMemoryStore {
    fn insert(
        &self,
        _record: &MemoryRecord,
        _normalized_fact: &str,
        _terms: &[String],
        _episode: Option<&Episode>,
    ) -> Result<(), String> {
        Err("memory store is not available".to_string())
    }

    fn find_duplicate(
        &self,
        _normalized_fact: &str,
        _record: &MemoryRecord,
    ) -> Result<Option<MemoryRecord>, String> {
        Ok(None)
    }

    fn search(
        &self,
        _terms: &[String],
        _filter: &MemoryFilter,
        _now: &str,
        _limit: usize,
    ) -> Result<Vec<MemoryHit>, String> {
        Ok(Vec::new())
    }

    fn get(&self, _id: &str) -> Result<Option<MemoryRecord>, String> {
        Ok(None)
    }

    fn episode(&self, _id: &str) -> Result<Option<Episode>, String> {
        Ok(None)
    }

    fn list(&self, _filter: &MemoryListFilter) -> Result<Vec<MemoryRecord>, String> {
        Ok(Vec::new())
    }

    fn invalidate(&self, _id: &str, _at: &str, _reason: &str) -> Result<bool, String> {
        Err("memory store is not available".to_string())
    }

    fn supersede(
        &self,
        _old_id: &str,
        _new_record: &MemoryRecord,
        _normalized_fact: &str,
        _terms: &[String],
        _episode: Option<&Episode>,
        _at: &str,
        _reason: &str,
    ) -> Result<(), String> {
        Err("memory store is not available".to_string())
    }

    fn forget(&self, _user_id: &str, _selector: &ForgetSelector) -> Result<ForgetReport, String> {
        Err("memory store is not available".to_string())
    }

    fn stats(&self, _user_id: &str) -> Result<MemoryStats, String> {
        Ok(MemoryStats::default())
    }

    fn ping(&self) -> Result<(), String> {
        Err("memory store is not available".to_string())
    }
}
