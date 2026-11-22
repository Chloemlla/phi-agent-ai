//! Memory retrieval golden set.
//!
//! The fixture in `tests/fixtures/memory_golden.json` is a seed corpus plus
//! queries with expected and forbidden facts. Deterministic checks (scope
//! leakage, fixture integrity) run on every CI pass; the quality baseline is
//! marked `#[ignore]` and is run explicitly, non-blocking, per
//! `docs/70-quality/evaluation.md` (MVP maintains a baseline without blocking
//! merges).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use phi_adapters::memory_sqlite::SqliteMemoryStore;
use phi_contracts::memory::{
    MemoryAddOutcome, MemoryAddRequest, MemoryPort, MemoryQuery, Principal,
};
use phi_domain::memory::{MemoryScope, SourceTrust};
use phi_kernel::event_bus::EventBus;
use phi_kernel::ports::{ClockPort, IdPort, JournalPort, LogPort};
use phi_plugins::memory::MemoryPlugin;

#[derive(Deserialize)]
struct Fixture {
    version: u32,
    facts: Vec<FactDef>,
    queries: Vec<QueryDef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FactDef {
    key: String,
    fact: String,
    scope: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    invalidated: bool,
    #[serde(default)]
    valid_to: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
}

#[derive(Deserialize)]
struct QueryDef {
    query: String,
    project: String,
    #[serde(default)]
    expected: Vec<String>,
    #[serde(default)]
    forbidden: Vec<String>,
}

#[derive(Default)]
struct MemJournal;

impl JournalPort for MemJournal {
    fn append(&self, _line: &str) -> Result<(), String> {
        Ok(())
    }
    fn read_all(&self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}

struct TestClock {
    tick: AtomicU64,
}

impl ClockPort for TestClock {
    fn now_rfc3339(&self) -> String {
        let millis = self.tick.fetch_add(1, Ordering::SeqCst);
        format!("2026-01-01T00:00:00.{:03}Z", millis.min(999))
    }
}

struct TestIds {
    tick: AtomicU64,
}

impl IdPort for TestIds {
    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", self.tick.fetch_add(1, Ordering::SeqCst))
    }
}

struct SilentLog;

impl LogPort for SilentLog {
    fn info(&self, _msg: &str) {}
    fn warn(&self, _msg: &str) {}
    fn error(&self, _msg: &str) {}
}

fn plugin() -> Arc<MemoryPlugin> {
    let bus = Arc::new(EventBus::new(
        Arc::new(MemJournal),
        Arc::new(TestClock {
            tick: AtomicU64::new(0),
        }),
        Arc::new(TestIds {
            tick: AtomicU64::new(10_000),
        }),
        Arc::new(SilentLog),
    ));
    let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
    Arc::new(MemoryPlugin::new(
        store,
        Arc::new(TestClock {
            tick: AtomicU64::new(1000),
        }),
        Arc::new(TestIds {
            tick: AtomicU64::new(0),
        }),
        bus,
        Arc::new(SilentLog),
        "golden_user",
    ))
}

fn principal(project: &str) -> Principal {
    Principal {
        user_id: "golden_user".to_string(),
        project_id: Some(format!("/golden/{project}")),
        agent_id: Some("main".to_string()),
        run_id: Some(format!("run-{project}")),
    }
}

fn parse_scope(raw: &str) -> MemoryScope {
    match raw {
        "project" => MemoryScope::Project,
        "agent" => MemoryScope::Agent,
        "run" => MemoryScope::Run,
        _ => MemoryScope::User,
    }
}

fn admitted_id(outcome: MemoryAddOutcome) -> String {
    match outcome {
        MemoryAddOutcome::Admitted { record_id, .. } => record_id,
        other => panic!("expected admitted memory, got {other:?}"),
    }
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/memory_golden.json"))
        .expect("memory golden fixture must be valid JSON")
}

#[test]
fn golden_fixture_is_well_formed() {
    let fixture = fixture();
    assert_eq!(fixture.version, 1);

    let keys: HashSet<&str> = fixture.facts.iter().map(|fact| fact.key.as_str()).collect();
    assert_eq!(keys.len(), fixture.facts.len(), "fact keys must be unique");

    for fact in &fixture.facts {
        assert!(
            ["user", "project", "agent", "run"].contains(&fact.scope.as_str()),
            "unknown scope in fixture: {}",
            fact.scope
        );
        if let Some(target) = &fact.supersedes {
            assert!(
                keys.contains(target.as_str()),
                "unknown supersede target {target}"
            );
        }
    }

    assert!(!fixture.queries.is_empty());
    for query in &fixture.queries {
        assert!(
            !query.expected.is_empty() || !query.forbidden.is_empty(),
            "query has neither expectations nor forbidden facts: {}",
            query.query
        );
        for key in query.expected.iter().chain(query.forbidden.iter()) {
            assert!(
                keys.contains(key.as_str()),
                "unknown fact key in query: {key}"
            );
        }
    }
}

#[test]
#[ignore = "quality baseline; run explicitly with --ignored"]
fn golden_retrieval_meets_baseline() {
    let fixture = fixture();
    let plugin = plugin();

    let mut ids: HashMap<String, String> = HashMap::new();
    for fact in &fixture.facts {
        let project = fact.project.as_deref().unwrap_or("p1");
        let principal = principal(project);
        if let Some(old_key) = &fact.supersedes {
            let old_id = ids
                .get(old_key)
                .unwrap_or_else(|| panic!("supersede target {old_key} was not added first"));
            let outcome = plugin
                .correct(&principal, old_id, &fact.fact, "golden supersede")
                .unwrap();
            ids.insert(fact.key.clone(), admitted_id(outcome));
            continue;
        }
        let mut request =
            MemoryAddRequest::new(&fact.fact, parse_scope(&fact.scope), SourceTrust::User);
        request.valid_to = fact.valid_to.clone();
        let outcome = plugin.add(&principal, request).unwrap();
        let id = admitted_id(outcome);
        if fact.invalidated {
            plugin
                .invalidate(&principal, &id, "golden invalidation")
                .unwrap();
        }
        ids.insert(fact.key.clone(), id);
    }

    let id_to_key: HashMap<&str, &str> = ids
        .iter()
        .map(|(key, id)| (id.as_str(), key.as_str()))
        .collect();

    let mut recall_sum = 0.0;
    let mut recall_count = 0usize;
    let mut mrr_sum = 0.0;
    let mut leakage = 0usize;
    let mut returned_total = 0usize;
    let mut relevant_returned = 0usize;

    for query in &fixture.queries {
        let principal = principal(&query.project);
        let hits = plugin
            .search(
                &principal,
                &MemoryQuery {
                    text: query.query.clone(),
                    limit: 5,
                    ..Default::default()
                },
            )
            .unwrap();
        let returned: Vec<&str> = hits
            .iter()
            .filter_map(|hit| id_to_key.get(hit.record.id.as_str()).copied())
            .collect();
        returned_total += returned.len();

        for forbidden in &query.forbidden {
            if returned.contains(&forbidden.as_str()) {
                leakage += 1;
                eprintln!(
                    "LEAK: query {:?} returned forbidden fact {forbidden}",
                    query.query
                );
            }
        }
        if query.expected.is_empty() {
            continue;
        }
        recall_count += 1;
        let found = query
            .expected
            .iter()
            .filter(|key| returned.contains(&key.as_str()))
            .count();
        relevant_returned += found;
        recall_sum += found as f64 / query.expected.len() as f64;
        if let Some(position) = returned
            .iter()
            .position(|key| query.expected.iter().any(|expected| expected == key))
        {
            mrr_sum += 1.0 / (position as f64 + 1.0);
        } else {
            eprintln!(
                "MISS: query {:?} expected {:?}, returned {:?}",
                query.query, query.expected, returned
            );
        }
    }

    let recall = recall_sum / recall_count as f64;
    let mrr = mrr_sum / recall_count as f64;
    let noise = 1.0 - relevant_returned as f64 / returned_total.max(1) as f64;
    println!(
        "memory golden: queries={recall_count} recall@5={recall:.3} mrr={mrr:.3} \
         leakage={leakage} returned={returned_total} noise={noise:.3}"
    );

    assert_eq!(leakage, 0, "memory visibility must never leak facts");
    assert!(
        recall >= 0.80,
        "recall@5 {recall:.3} fell below the 0.80 baseline"
    );
    assert!(mrr >= 0.55, "MRR {mrr:.3} fell below the 0.55 baseline");
}
