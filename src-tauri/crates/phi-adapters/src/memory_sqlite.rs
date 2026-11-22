//! SQLite-backed memory store.
//!
//! One file holds facts (`memories`), raw source fragments (`episodes`) and a
//! small inverted index (`memory_terms`) used by BM25. Memory text never goes
//! into the session journal, so `forget` can erase the complete chain by
//! deleting rows from this database.
//!
//! The schema is versioned with `PRAGMA user_version`; a mismatch fails loudly
//! instead of silently writing into an unknown layout.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params, params_from_iter};

use phi_contracts::memory::{
    ForgetReport, ForgetSelector, MemoryFilter, MemoryHit, MemoryListFilter, MemoryStats,
    MemoryStore,
};
use phi_domain::memory::{
    Episode, EpisodeSource, MemoryRecord, MemoryScope, MemoryState, SourceTrust,
};

const SCHEMA_VERSION: i64 = 1;
const VISIBILITY_PROJECT: &str = "project";
const VISIBILITY_AGENT: &str = "agent";
const VISIBILITY_RUN: &str = "run";

/// Store all timestamps in one canonical UTC-millisecond form. Lexical order
/// then equals chronological order regardless of the caller's precision or
/// offset syntax (`Z` versus `+00:00`, 3 versus 9 fractional digits).
fn canonical_timestamp(raw: &str) -> String {
    DateTime::parse_from_rfc3339(raw)
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|_| raw.to_string())
}

const RECORD_COLUMNS: &str = "m.id, m.fact, m.scope, m.user_id, m.project_id, m.agent_id, \
    m.run_id, m.source_trust, m.confidence, m.valid_from, m.valid_to, m.recorded_at, \
    m.invalidated_at, m.superseded_by, m.reason, m.state, m.episode_id, m.redacted";

pub struct SqliteMemoryStore {
    conn: Mutex<Connection>,
}

impl SqliteMemoryStore {
    /// Open (or create) a database. A read-only open never creates the file and
    /// never writes: `forget`, `add` and schema migration fail honestly.
    pub fn open(path: impl AsRef<Path>, writable: bool) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if writable {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("memory store: cannot create data directory: {e}"))?;
            }
        } else if !path.exists() {
            return Err("memory store: database does not exist".to_string());
        }
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let conn = Connection::open_with_flags(&path, flags)
            .map_err(|e| format!("memory store: cannot open database: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("memory store: cannot set busy timeout: {e}"))?;
        if writable {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;",
            )
            .map_err(|e| format!("memory store: cannot configure database: {e}"))?;
            init_schema(&conn)?;
        } else {
            conn.execute_batch("PRAGMA query_only = ON;")
                .map_err(|e| format!("memory store: cannot open read-only database: {e}"))?;
            let version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| format!("memory store: cannot read schema version: {e}"))?;
            if version != SCHEMA_VERSION {
                return Err(format!(
                    "memory store: unsupported schema version {version} (expected {SCHEMA_VERSION})"
                ));
            }
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Ephemeral store for tests and for an explicitly disabled persistent
    /// backend. Lives only as long as the process.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("memory store: cannot open in-memory database: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("memory store: cannot configure database: {e}"))?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("memory store: cannot read schema version: {e}"))?;
    if version == 0 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS episodes (
                 id           TEXT PRIMARY KEY,
                 content      TEXT NOT NULL,
                 source       TEXT NOT NULL,
                 source_label TEXT,
                 scope        TEXT NOT NULL,
                 user_id      TEXT NOT NULL,
                 project_id   TEXT,
                 agent_id     TEXT,
                 run_id       TEXT,
                 session_id   TEXT,
                 created_at   TEXT NOT NULL,
                 redacted     INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS episodes_owner
                 ON episodes(user_id, scope, project_id, run_id);

             CREATE TABLE IF NOT EXISTS memories (
                 id              TEXT PRIMARY KEY,
                 fact            TEXT NOT NULL,
                 normalized_fact TEXT NOT NULL,
                 scope           TEXT NOT NULL,
                 user_id         TEXT NOT NULL,
                 project_id      TEXT,
                 agent_id        TEXT,
                 run_id          TEXT,
                 source_trust    TEXT NOT NULL,
                 confidence      REAL NOT NULL,
                 valid_from      TEXT,
                 valid_to        TEXT,
                 recorded_at     TEXT NOT NULL,
                 invalidated_at  TEXT,
                 superseded_by   TEXT,
                 reason          TEXT,
                 state           TEXT NOT NULL,
                 episode_id      TEXT REFERENCES episodes(id) ON DELETE SET NULL,
                 redacted        INTEGER NOT NULL DEFAULT 0,
                 term_count      INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS memories_owner
                 ON memories(user_id, scope, project_id, agent_id, run_id, state);
             CREATE INDEX IF NOT EXISTS memories_duplicate
                 ON memories(user_id, scope, project_id, agent_id, run_id, normalized_fact, state);
             CREATE INDEX IF NOT EXISTS memories_recorded
                 ON memories(recorded_at DESC);

             CREATE TABLE IF NOT EXISTS memory_terms (
                 term      TEXT NOT NULL,
                 memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
                 tf        INTEGER NOT NULL,
                 PRIMARY KEY(term, memory_id)
             );
             CREATE INDEX IF NOT EXISTS memory_terms_memory ON memory_terms(memory_id);
             PRAGMA user_version = 1;
             COMMIT;",
        )
        .map_err(|e| format!("memory store: cannot initialize schema: {e}"))?;
    } else if version != SCHEMA_VERSION {
        return Err(format!(
            "memory store: unsupported schema version {version} (expected {SCHEMA_VERSION})"
        ));
    }
    Ok(())
}

fn insert_episode(conn: &Connection, episode: &Episode) -> Result<(), String> {
    conn.execute(
        "INSERT INTO episodes (id, content, source, source_label, scope, user_id, project_id,
             agent_id, run_id, session_id, created_at, redacted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            episode.id,
            episode.content,
            episode.source.as_str(),
            episode.source_label,
            episode.scope.as_str(),
            episode.user_id,
            episode.project_id,
            episode.agent_id,
            episode.run_id,
            episode.session_id,
            canonical_timestamp(&episode.created_at),
            episode.redacted,
        ],
    )
    .map_err(|e| format!("memory store: cannot insert episode: {e}"))?;
    Ok(())
}

fn insert_record(
    conn: &Connection,
    record: &MemoryRecord,
    normalized_fact: &str,
    terms: &[String],
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO memories (id, fact, normalized_fact, scope, user_id, project_id, agent_id,
             run_id, source_trust, confidence, valid_from, valid_to, recorded_at, invalidated_at,
             superseded_by, reason, state, episode_id, redacted, term_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
             ?18, ?19, ?20)",
        params![
            record.id,
            record.fact,
            normalized_fact,
            record.scope.as_str(),
            record.user_id,
            record.project_id,
            record.agent_id,
            record.run_id,
            record.source_trust.as_str(),
            record.confidence,
            record.valid_from.as_deref().map(canonical_timestamp),
            record.valid_to.as_deref().map(canonical_timestamp),
            canonical_timestamp(&record.recorded_at),
            record.invalidated_at.as_deref().map(canonical_timestamp),
            record.superseded_by,
            record.reason,
            record.state.as_str(),
            record.episode_id,
            record.redacted,
            terms.len() as i64,
        ],
    )
    .map_err(|e| format!("memory store: cannot insert memory: {e}"))?;

    let mut counts: HashMap<&str, i64> = HashMap::new();
    for term in terms {
        *counts.entry(term.as_str()).or_insert(0) += 1;
    }
    let mut statement = conn
        .prepare("INSERT INTO memory_terms (term, memory_id, tf) VALUES (?1, ?2, ?3)")
        .map_err(|e| format!("memory store: cannot prepare index insert: {e}"))?;
    for (term, tf) in counts {
        statement
            .execute(params![term, record.id, tf])
            .map_err(|e| format!("memory store: cannot index memory: {e}"))?;
    }
    Ok(())
}

fn scope_from(raw: &str) -> MemoryScope {
    match raw {
        VISIBILITY_PROJECT => MemoryScope::Project,
        VISIBILITY_AGENT => MemoryScope::Agent,
        VISIBILITY_RUN => MemoryScope::Run,
        _ => MemoryScope::User,
    }
}

fn trust_from(raw: &str) -> SourceTrust {
    match raw {
        "system" => SourceTrust::System,
        "tool" => SourceTrust::Tool,
        "model" => SourceTrust::Model,
        "external" => SourceTrust::External,
        _ => SourceTrust::User,
    }
}

fn state_from(raw: &str) -> MemoryState {
    match raw {
        "invalidated" => MemoryState::Invalidated,
        "superseded" => MemoryState::Superseded,
        _ => MemoryState::Active,
    }
}

fn episode_source_from(raw: &str) -> EpisodeSource {
    match raw {
        "tool_result" => EpisodeSource::ToolResult,
        "model_output" => EpisodeSource::ModelOutput,
        "external" => EpisodeSource::External,
        _ => EpisodeSource::UserInput,
    }
}

fn read_record(row: &Row, offset: usize) -> rusqlite::Result<MemoryRecord> {
    Ok(MemoryRecord {
        id: row.get(offset)?,
        fact: row.get(offset + 1)?,
        scope: scope_from(&row.get::<_, String>(offset + 2)?),
        user_id: row.get(offset + 3)?,
        project_id: row.get(offset + 4)?,
        agent_id: row.get(offset + 5)?,
        run_id: row.get(offset + 6)?,
        source_trust: trust_from(&row.get::<_, String>(offset + 7)?),
        confidence: row.get(offset + 8)?,
        valid_from: row.get(offset + 9)?,
        valid_to: row.get(offset + 10)?,
        recorded_at: row.get(offset + 11)?,
        invalidated_at: row.get(offset + 12)?,
        superseded_by: row.get(offset + 13)?,
        reason: row.get(offset + 14)?,
        state: state_from(&row.get::<_, String>(offset + 15)?),
        episode_id: row.get(offset + 16)?,
        redacted: row.get(offset + 17)?,
    })
}

fn read_episode(row: &Row) -> rusqlite::Result<Episode> {
    Ok(Episode {
        id: row.get(0)?,
        content: row.get(1)?,
        source: episode_source_from(&row.get::<_, String>(2)?),
        source_label: row.get(3)?,
        scope: scope_from(&row.get::<_, String>(4)?),
        user_id: row.get(5)?,
        project_id: row.get(6)?,
        agent_id: row.get(7)?,
        run_id: row.get(8)?,
        session_id: row.get(9)?,
        created_at: row.get(10)?,
        redacted: row.get(11)?,
    })
}

fn push_visibility_clause(filter: &MemoryFilter, alias: &str, params: &mut Vec<Value>) -> String {
    let user = Value::Text(filter.user_id.clone());
    let mut clauses = vec![format!("({alias}.scope = 'user' AND {alias}.user_id = ?)")];
    params.push(user.clone());
    if let Some(project_id) = &filter.project_id {
        clauses.push(format!(
            "({alias}.scope = 'project' AND {alias}.user_id = ? AND {alias}.project_id = ?)"
        ));
        params.push(user.clone());
        params.push(Value::Text(project_id.clone()));
    }
    if let Some(agent_id) = &filter.agent_id {
        // Agent knowledge is project-bound: the same agent definition used in
        // another project must not inherit this project's memories.
        clauses.push(format!(
            "({alias}.scope = 'agent' AND {alias}.user_id = ? AND {alias}.agent_id = ? \
             AND {alias}.project_id IS ?)"
        ));
        params.push(user.clone());
        params.push(Value::Text(agent_id.clone()));
        params.push(match &filter.project_id {
            Some(project_id) => Value::Text(project_id.clone()),
            None => Value::Null,
        });
    }
    if let Some(run_id) = &filter.run_id {
        clauses.push(format!(
            "({alias}.scope = 'run' AND {alias}.user_id = ? AND {alias}.run_id = ?)"
        ));
        params.push(user.clone());
        params.push(Value::Text(run_id.clone()));
    }
    format!("({})", clauses.join(" OR "))
}

fn push_scope_clause(scopes: &[MemoryScope], alias: &str, params: &mut Vec<Value>) -> String {
    if scopes.is_empty() {
        return String::new();
    }
    let placeholders = vec!["?"; scopes.len()].join(", ");
    for scope in scopes {
        params.push(Value::Text(scope.as_str().to_string()));
    }
    format!(" AND {alias}.scope IN ({placeholders})")
}

fn push_time_clause(now: &str, alias: &str, params: &mut Vec<Value>) -> String {
    params.push(Value::Text(now.to_string()));
    params.push(Value::Text(now.to_string()));
    format!(
        " AND ({alias}.valid_from IS NULL OR {alias}.valid_from <= ?) \
         AND ({alias}.valid_to IS NULL OR {alias}.valid_to > ?)"
    )
}

fn push_state_clause(filter: &MemoryFilter, alias: &str) -> String {
    if filter.include_inactive {
        String::new()
    } else {
        format!(" AND {alias}.state = 'active'")
    }
}

fn bm25(idf: f64, tf: f64, doc_len: f64, avg_len: f64) -> f64 {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let norm = K1 * (1.0 - B + B * doc_len / avg_len.max(1.0));
    idf * tf * (K1 + 1.0) / (tf + norm)
}

impl MemoryStore for SqliteMemoryStore {
    fn insert(
        &self,
        record: &MemoryRecord,
        normalized_fact: &str,
        terms: &[String],
        episode: Option<&Episode>,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| format!("memory store: cannot start transaction: {e}"))?;
        if let Some(episode) = episode {
            insert_episode(&tx, episode)?;
        }
        insert_record(&tx, record, normalized_fact, terms)?;
        tx.commit()
            .map_err(|e| format!("memory store: cannot commit memory: {e}"))?;
        Ok(())
    }

    fn find_duplicate(
        &self,
        normalized_fact: &str,
        record: &MemoryRecord,
    ) -> Result<Option<MemoryRecord>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM memories m
                 WHERE m.user_id = ?1 AND m.scope = ?2 AND m.normalized_fact = ?3
                   AND m.state = 'active' AND m.project_id IS ?4 AND m.agent_id IS ?5
                   AND m.run_id IS ?6
                 ORDER BY m.recorded_at DESC LIMIT 1"
            ),
            params![
                record.user_id,
                record.scope.as_str(),
                normalized_fact,
                record.project_id,
                record.agent_id,
                record.run_id,
            ],
            |row| read_record(row, 0),
        )
        .optional()
        .map_err(|e| format!("memory store: cannot check duplicates: {e}"))
    }

    fn search(
        &self,
        terms: &[String],
        filter: &MemoryFilter,
        now: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, String> {
        if terms.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let now = canonical_timestamp(now);
        let now = now.as_str();
        let mut query_terms: Vec<String> = Vec::new();
        for term in terms {
            if !query_terms.contains(term) {
                query_terms.push(term.clone());
            }
        }

        let conn = self.conn.lock().unwrap();

        // Corpus statistics use the same visibility and validity rules as the
        // candidate query, so IDF reflects what the principal can actually see.
        let mut stats_params: Vec<Value> = Vec::new();
        let visibility = push_visibility_clause(filter, "m", &mut stats_params);
        let mut stats_sql = format!(
            "SELECT COUNT(*), COALESCE(AVG(m.term_count), 0) FROM memories m
             WHERE {visibility}"
        );
        stats_sql.push_str(&push_state_clause(filter, "m"));
        stats_sql.push_str(&push_time_clause(now, "m", &mut stats_params));
        stats_sql.push_str(&push_scope_clause(&filter.scopes, "m", &mut stats_params));
        let (corpus_size, average_len): (f64, f64) = conn
            .query_row(&stats_sql, params_from_iter(stats_params.iter()), |row| {
                Ok((row.get::<_, i64>(0)? as f64, row.get::<_, f64>(1)?))
            })
            .map_err(|e| format!("memory store: cannot read corpus statistics: {e}"))?;
        if corpus_size < 1.0 {
            return Ok(Vec::new());
        }

        let mut params: Vec<Value> = Vec::new();
        let placeholders = vec!["?"; query_terms.len()].join(", ");
        for term in &query_terms {
            params.push(Value::Text(term.clone()));
        }
        let visibility = push_visibility_clause(filter, "m", &mut params);
        let mut sql = format!(
            "SELECT {RECORD_COLUMNS}, t.term, t.tf, m.term_count
             FROM memory_terms t JOIN memories m ON m.id = t.memory_id
             WHERE t.term IN ({placeholders}) AND {visibility}"
        );
        sql.push_str(&push_state_clause(filter, "m"));
        sql.push_str(&push_time_clause(now, "m", &mut params));
        sql.push_str(&push_scope_clause(&filter.scopes, "m", &mut params));

        let mut statement = conn
            .prepare(&sql)
            .map_err(|e| format!("memory store: cannot prepare search: {e}"))?;
        let rows = statement
            .query_map(params_from_iter(params.iter()), |row| {
                let record = read_record(row, 0)?;
                let term: String = row.get(18)?;
                let tf: i64 = row.get(19)?;
                let term_count: i64 = row.get(20)?;
                Ok((record, term, tf, term_count))
            })
            .map_err(|e| format!("memory store: search failed: {e}"))?;

        struct Candidate {
            record: MemoryRecord,
            doc_len: f64,
            frequencies: HashMap<String, f64>,
        }
        let mut candidates: HashMap<String, Candidate> = HashMap::new();
        for row in rows {
            let (record, term, tf, term_count) =
                row.map_err(|e| format!("memory store: cannot read search row: {e}"))?;
            let candidate = candidates
                .entry(record.id.clone())
                .or_insert_with(|| Candidate {
                    record,
                    doc_len: term_count as f64,
                    frequencies: HashMap::new(),
                });
            candidate.frequencies.insert(term, tf as f64);
        }

        let mut document_frequency: HashMap<&str, f64> = HashMap::new();
        for term in &query_terms {
            let df = candidates
                .values()
                .filter(|candidate| candidate.frequencies.contains_key(term))
                .count() as f64;
            if df > 0.0 {
                document_frequency.insert(term.as_str(), df);
            }
        }

        let mut hits = Vec::new();
        for (_, candidate) in candidates {
            let mut score = 0.0;
            let mut matched_terms = Vec::new();
            for term in &query_terms {
                let tf = candidate.frequencies.get(term).copied().unwrap_or(0.0);
                if tf <= 0.0 {
                    continue;
                }
                let df = document_frequency
                    .get(term.as_str())
                    .copied()
                    .unwrap_or(0.0);
                if df <= 0.0 {
                    continue;
                }
                let idf = (1.0 + (corpus_size - df + 0.5) / (df + 0.5)).ln();
                score += bm25(idf, tf, candidate.doc_len, average_len);
                matched_terms.push(term.clone());
            }
            if score > 0.0 {
                hits.push(MemoryHit {
                    record: candidate.record,
                    score,
                    matched_terms,
                });
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.record.recorded_at.cmp(&a.record.recorded_at))
                .then_with(|| a.record.id.cmp(&b.record.id))
        });
        hits.truncate(limit);
        Ok(hits)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!("SELECT {RECORD_COLUMNS} FROM memories m WHERE m.id = ?1"),
            params![id],
            |row| read_record(row, 0),
        )
        .optional()
        .map_err(|e| format!("memory store: cannot read memory: {e}"))
    }

    fn episode(&self, id: &str) -> Result<Option<Episode>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, content, source, source_label, scope, user_id, project_id, agent_id,
                    run_id, session_id, created_at, redacted
             FROM episodes WHERE id = ?1",
            params![id],
            read_episode,
        )
        .optional()
        .map_err(|e| format!("memory store: cannot read episode: {e}"))
    }

    fn list(&self, filter: &MemoryListFilter) -> Result<Vec<MemoryRecord>, String> {
        let conn = self.conn.lock().unwrap();
        let mut params: Vec<Value> = vec![Value::Text(filter.user_id.clone())];
        let mut sql = format!("SELECT {RECORD_COLUMNS} FROM memories m WHERE m.user_id = ?");
        if let Some(text) = &filter.text {
            let escaped = text
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            sql.push_str(" AND m.fact LIKE ? ESCAPE '\\'");
            params.push(Value::Text(format!("%{escaped}%")));
        }
        sql.push_str(&push_scope_clause(&filter.scopes, "m", &mut params));
        if !filter.states.is_empty() {
            let placeholders = vec!["?"; filter.states.len()].join(", ");
            sql.push_str(&format!(" AND m.state IN ({placeholders})"));
            for state in &filter.states {
                params.push(Value::Text(state.as_str().to_string()));
            }
        }
        if let Some(project_id) = &filter.project_id {
            sql.push_str(" AND m.project_id = ?");
            params.push(Value::Text(project_id.clone()));
        }
        if let Some(run_id) = &filter.run_id {
            sql.push_str(" AND m.run_id = ?");
            params.push(Value::Text(run_id.clone()));
        }
        sql.push_str(" ORDER BY m.recorded_at DESC, m.id LIMIT ? OFFSET ?");
        let limit = if filter.limit == 0 {
            -1
        } else {
            filter.limit as i64
        };
        params.push(Value::Integer(limit));
        params.push(Value::Integer(filter.offset as i64));

        let mut statement = conn
            .prepare(&sql)
            .map_err(|e| format!("memory store: cannot prepare list: {e}"))?;
        let rows = statement
            .query_map(params_from_iter(params.iter()), |row| read_record(row, 0))
            .map_err(|e| format!("memory store: cannot list memories: {e}"))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| format!("memory store: cannot read memory row: {e}"))?);
        }
        Ok(records)
    }

    fn invalidate(&self, id: &str, at: &str, reason: &str) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE memories SET state = 'invalidated', invalidated_at = ?2, reason = ?3
                 WHERE id = ?1 AND state = 'active'",
                params![id, canonical_timestamp(at), reason],
            )
            .map_err(|e| format!("memory store: cannot invalidate memory: {e}"))?;
        Ok(changed > 0)
    }

    fn supersede(
        &self,
        old_id: &str,
        new_record: &MemoryRecord,
        normalized_fact: &str,
        terms: &[String],
        episode: Option<&Episode>,
        at: &str,
        reason: &str,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| format!("memory store: cannot start transaction: {e}"))?;
        let changed = tx
            .execute(
                "UPDATE memories SET state = 'superseded', invalidated_at = ?2,
                     superseded_by = ?3, reason = ?4
                 WHERE id = ?1 AND state = 'active'",
                params![old_id, canonical_timestamp(at), new_record.id, reason],
            )
            .map_err(|e| format!("memory store: cannot supersede memory: {e}"))?;
        if changed == 0 {
            return Err("memory store: record not found or already inactive".to_string());
        }
        if let Some(episode) = episode {
            insert_episode(&tx, episode)?;
        }
        insert_record(&tx, new_record, normalized_fact, terms)?;
        tx.commit()
            .map_err(|e| format!("memory store: cannot commit supersede: {e}"))?;
        Ok(())
    }

    fn forget(&self, user_id: &str, selector: &ForgetSelector) -> Result<ForgetReport, String> {
        if !selector.has_selector() && !selector.all {
            return Err(
                "memory store: refusing to forget everything without an explicit selector"
                    .to_string(),
            );
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| format!("memory store: cannot start transaction: {e}"))?;

        let mut params: Vec<Value> = vec![Value::Text(user_id.to_string())];
        let mut where_sql = String::from("user_id = ?");
        if !selector.ids.is_empty() {
            let placeholders = vec!["?"; selector.ids.len()].join(", ");
            where_sql.push_str(&format!(" AND id IN ({placeholders})"));
            for id in &selector.ids {
                params.push(Value::Text(id.clone()));
            }
        }
        if !selector.scopes.is_empty() {
            let placeholders = vec!["?"; selector.scopes.len()].join(", ");
            where_sql.push_str(&format!(" AND scope IN ({placeholders})"));
            for scope in &selector.scopes {
                params.push(Value::Text(scope.as_str().to_string()));
            }
        }
        if let Some(project_id) = &selector.project_id {
            where_sql.push_str(" AND project_id = ?");
            params.push(Value::Text(project_id.clone()));
        }
        if let Some(run_id) = &selector.run_id {
            where_sql.push_str(" AND run_id = ?");
            params.push(Value::Text(run_id.clone()));
        }

        let mut episode_ids: Vec<String> = Vec::new();
        {
            let mut statement = tx
                .prepare(&format!(
                    "SELECT id, episode_id FROM memories WHERE {where_sql}"
                ))
                .map_err(|e| format!("memory store: cannot prepare forget: {e}"))?;
            let rows = statement
                .query_map(params_from_iter(params.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(|e| format!("memory store: cannot select memories to forget: {e}"))?;
            for row in rows {
                let (_, episode_id) =
                    row.map_err(|e| format!("memory store: cannot read forget row: {e}"))?;
                if let Some(episode_id) = episode_id {
                    if !episode_ids.contains(&episode_id) {
                        episode_ids.push(episode_id);
                    }
                }
            }
        }

        tx.execute(
            &format!("DELETE FROM memory_terms WHERE memory_id IN (SELECT id FROM memories WHERE {where_sql})"),
            params_from_iter(params.iter()),
        )
        .map_err(|e| format!("memory store: cannot delete index rows: {e}"))?;
        let records =
            tx.execute(
                &format!("DELETE FROM memories WHERE {where_sql}"),
                params_from_iter(params.iter()),
            )
            .map_err(|e| format!("memory store: cannot delete memories: {e}"))? as u64;

        // Linked episodes first, then episodes that match the same attribution
        // scope (defensive: an episode without a surviving record is still part
        // of the chain the user asked to erase).
        let mut episodes = 0u64;
        for episode_id in &episode_ids {
            episodes += tx
                .execute("DELETE FROM episodes WHERE id = ?1", params![episode_id])
                .map_err(|e| format!("memory store: cannot delete episode: {e}"))?
                as u64;
        }
        let mut episode_params: Vec<Value> = vec![Value::Text(user_id.to_string())];
        let mut episode_where = String::from("user_id = ?");
        if !selector.scopes.is_empty() {
            let placeholders = vec!["?"; selector.scopes.len()].join(", ");
            episode_where.push_str(&format!(" AND scope IN ({placeholders})"));
            for scope in &selector.scopes {
                episode_params.push(Value::Text(scope.as_str().to_string()));
            }
        }
        if let Some(project_id) = &selector.project_id {
            episode_where.push_str(" AND project_id = ?");
            episode_params.push(Value::Text(project_id.clone()));
        }
        if let Some(run_id) = &selector.run_id {
            episode_where.push_str(" AND run_id = ?");
            episode_params.push(Value::Text(run_id.clone()));
        }
        let has_attribution_selector = !selector.scopes.is_empty()
            || selector.project_id.is_some()
            || selector.run_id.is_some();
        // A pure id selector must not widen into "all episodes of the user";
        // only linked episodes are part of that chain. `all` and attribution
        // selectors also erase episodes that match the same scope.
        if selector.all || has_attribution_selector {
            episodes += tx
                .execute(
                    &format!("DELETE FROM episodes WHERE {episode_where}"),
                    params_from_iter(episode_params.iter()),
                )
                .map_err(|e| format!("memory store: cannot delete episodes: {e}"))?
                as u64;
        }

        tx.commit()
            .map_err(|e| format!("memory store: cannot commit forget: {e}"))?;
        Ok(ForgetReport { records, episodes })
    }

    fn stats(&self, user_id: &str) -> Result<MemoryStats, String> {
        let conn = self.conn.lock().unwrap();
        let stats = conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(state = 'active'), 0),
                        COALESCE(SUM(state = 'invalidated'), 0),
                        COALESCE(SUM(state = 'superseded'), 0),
                        COALESCE(SUM(scope = 'user'), 0),
                        COALESCE(SUM(scope = 'project'), 0),
                        COALESCE(SUM(scope = 'agent'), 0),
                        COALESCE(SUM(scope = 'run'), 0),
                        COALESCE(SUM(redacted), 0)
                 FROM memories WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(MemoryStats {
                        total: row.get::<_, i64>(0)? as u64,
                        active: row.get::<_, i64>(1)? as u64,
                        invalidated: row.get::<_, i64>(2)? as u64,
                        superseded: row.get::<_, i64>(3)? as u64,
                        user_scope: row.get::<_, i64>(4)? as u64,
                        project_scope: row.get::<_, i64>(5)? as u64,
                        agent_scope: row.get::<_, i64>(6)? as u64,
                        run_scope: row.get::<_, i64>(7)? as u64,
                        redacted: row.get::<_, i64>(8)? as u64,
                        episodes: 0,
                    })
                },
            )
            .map_err(|e| format!("memory store: cannot read statistics: {e}"))?;
        let episodes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("memory store: cannot count episodes: {e}"))?;
        Ok(MemoryStats {
            episodes: episodes as u64,
            ..stats
        })
    }

    fn ping(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT 1", [], |_| Ok(()))
            .map_err(|e| format!("memory store: ping failed: {e}"))
    }
}
