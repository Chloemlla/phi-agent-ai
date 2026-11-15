//! Integration tests for the SQLite memory store.
//!
//! These exercise the public `MemoryStore` contract: BM25 ranking, hard
//! visibility boundaries, version links and full-chain deletion.

use phi_adapters::memory_sqlite::SqliteMemoryStore;
use phi_contracts::memory::{ForgetSelector, MemoryFilter, MemoryListFilter, MemoryStore};
use phi_domain::memory::{
    Episode, EpisodeSource, MemoryRecord, MemoryScope, MemoryState, SourceTrust, normalize_fact,
};

fn record(
    id: &str,
    fact: &str,
    scope: MemoryScope,
    user: &str,
    project: Option<&str>,
    run: Option<&str>,
) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        fact: fact.to_string(),
        scope,
        user_id: user.to_string(),
        project_id: project.map(|p| p.to_string()),
        agent_id: if scope == MemoryScope::User {
            None
        } else {
            Some("main".to_string())
        },
        run_id: run.map(|r| r.to_string()),
        source_trust: SourceTrust::User,
        confidence: 0.9,
        valid_from: None,
        valid_to: None,
        recorded_at: "2026-01-01T00:00:00.000Z".to_string(),
        invalidated_at: None,
        superseded_by: None,
        reason: None,
        state: MemoryState::Active,
        episode_id: None,
        redacted: false,
    }
}

fn episode(id: &str, user: &str) -> Episode {
    Episode {
        id: id.to_string(),
        content: "raw source".to_string(),
        source: EpisodeSource::UserInput,
        source_label: None,
        scope: MemoryScope::User,
        user_id: user.to_string(),
        project_id: None,
        agent_id: None,
        run_id: None,
        session_id: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        redacted: false,
    }
}

fn terms(fact: &str) -> Vec<String> {
    fact.split_whitespace()
        .map(|word| word.to_lowercase())
        .collect()
}

fn insert(store: &SqliteMemoryStore, record: &MemoryRecord) {
    store
        .insert(
            record,
            &normalize_fact(&record.fact),
            &terms(&record.fact),
            None,
        )
        .unwrap();
}

fn filter(user: &str, project: Option<&str>, run: Option<&str>) -> MemoryFilter {
    MemoryFilter {
        user_id: user.to_string(),
        project_id: project.map(|p| p.to_string()),
        agent_id: Some("main".to_string()),
        run_id: run.map(|r| r.to_string()),
        scopes: Vec::new(),
        include_inactive: false,
    }
}

const NOW: &str = "2026-01-03T00:00:00.000Z";

#[test]
fn ranks_documents_that_match_more_terms_higher() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record(
            "m1",
            "dark mode preference",
            MemoryScope::User,
            "u1",
            None,
            None,
        ),
    );
    insert(
        &store,
        &record(
            "m2",
            "dark theme with high contrast",
            MemoryScope::User,
            "u1",
            None,
            None,
        ),
    );
    insert(
        &store,
        &record(
            "m3",
            "deployment notes",
            MemoryScope::User,
            "u1",
            None,
            None,
        ),
    );
    let hits = store
        .search(
            &["dark".into(), "mode".into()],
            &filter("u1", None, None),
            NOW,
            10,
        )
        .unwrap();
    assert_eq!(hits[0].record.id, "m1");
    assert!(hits.iter().all(|hit| hit.record.id != "m3"));
    assert!(hits[0].score >= hits[1].score);
}

#[test]
fn visibility_never_leaks_across_users_or_projects() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record(
            "p1",
            "project one uses pnpm",
            MemoryScope::Project,
            "u1",
            Some("p1"),
            None,
        ),
    );
    insert(
        &store,
        &record(
            "p2",
            "project two uses pnpm",
            MemoryScope::Project,
            "u1",
            Some("p2"),
            None,
        ),
    );
    insert(
        &store,
        &record(
            "other",
            "another user likes pnpm",
            MemoryScope::User,
            "u2",
            None,
            None,
        ),
    );
    let hits = store
        .search(&["pnpm".into()], &filter("u1", Some("p1"), None), NOW, 10)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.id, "p1");
}

#[test]
fn invalidated_records_leave_default_search_but_stay_readable() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record("m1", "uses tabs", MemoryScope::User, "u1", None, None),
    );
    assert!(
        store
            .invalidate("m1", "2026-01-02T00:00:00.000Z", "outdated")
            .unwrap()
    );
    let hits = store
        .search(&["tabs".into()], &filter("u1", None, None), NOW, 10)
        .unwrap();
    assert!(hits.is_empty());
    let stored = store.get("m1").unwrap().unwrap();
    assert_eq!(stored.state, MemoryState::Invalidated);
    assert_eq!(stored.reason.as_deref(), Some("outdated"));
}

#[test]
fn supersede_links_versions_and_hides_the_old_fact() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record(
            "old",
            "package manager is npm",
            MemoryScope::User,
            "u1",
            None,
            None,
        ),
    );
    let new = record(
        "new",
        "package manager is pnpm",
        MemoryScope::User,
        "u1",
        None,
        None,
    );
    store
        .supersede(
            "old",
            &new,
            &normalize_fact(&new.fact),
            &terms(&new.fact),
            None,
            "2026-01-02T00:00:00.000Z",
            "switched tooling",
        )
        .unwrap();
    let old = store.get("old").unwrap().unwrap();
    assert_eq!(old.state, MemoryState::Superseded);
    assert_eq!(old.superseded_by.as_deref(), Some("new"));
    let hits = store
        .search(&["npm".into()], &filter("u1", None, None), NOW, 10)
        .unwrap();
    assert!(hits.is_empty(), "superseded facts must not be retrieved");
}

#[test]
fn forget_erases_fact_index_and_episode() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    let mut rec = record(
        "m1",
        "secret-ish preference",
        MemoryScope::User,
        "u1",
        None,
        None,
    );
    rec.episode_id = Some("e1".to_string());
    store
        .insert(
            &rec,
            &normalize_fact(&rec.fact),
            &terms(&rec.fact),
            Some(&episode("e1", "u1")),
        )
        .unwrap();

    let report = store
        .forget("u1", &ForgetSelector::ids(vec!["m1".into()]))
        .unwrap();
    assert_eq!(report.records, 1);
    assert_eq!(report.episodes, 1);
    assert!(store.get("m1").unwrap().is_none());
    assert!(store.episode("e1").unwrap().is_none());
    assert_eq!(store.stats("u1").unwrap().total, 0);
    let hits = store
        .search(&["preference".into()], &filter("u1", None, None), NOW, 10)
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn forget_requires_an_explicit_selector() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    let mut rec = record("m1", "user one fact", MemoryScope::User, "u1", None, None);
    rec.episode_id = Some("e1".to_string());
    store
        .insert(
            &rec,
            &normalize_fact(&rec.fact),
            &terms(&rec.fact),
            Some(&episode("e1", "u1")),
        )
        .unwrap();
    assert!(store.forget("u1", &ForgetSelector::default()).is_err());
    assert!(store.get("m1").unwrap().is_some());
    let report = store.forget("u1", &ForgetSelector::all()).unwrap();
    assert_eq!(report.records, 1);
    assert_eq!(report.episodes, 1);
    assert!(store.get("m1").unwrap().is_none());
    assert!(store.episode("e1").unwrap().is_none());
}

#[test]
fn forget_is_scoped_to_the_owner() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record("m1", "user one fact", MemoryScope::User, "u1", None, None),
    );
    let report = store
        .forget("u2", &ForgetSelector::ids(vec!["m1".into()]))
        .unwrap();
    assert_eq!(report.records, 0);
    assert!(store.get("m1").unwrap().is_some());
}

#[test]
fn duplicate_detection_uses_attribution() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    let first = record(
        "m1",
        "Use pnpm",
        MemoryScope::Project,
        "u1",
        Some("p1"),
        None,
    );
    store
        .insert(
            &first,
            &normalize_fact(&first.fact),
            &terms(&first.fact),
            None,
        )
        .unwrap();
    let same = record(
        "m2",
        "use  PNPM",
        MemoryScope::Project,
        "u1",
        Some("p1"),
        None,
    );
    let found = store
        .find_duplicate(&normalize_fact(&same.fact), &same)
        .unwrap();
    assert_eq!(found.unwrap().id, "m1");
    let other_project = record(
        "m3",
        "use pnpm",
        MemoryScope::Project,
        "u1",
        Some("p2"),
        None,
    );
    let found = store
        .find_duplicate(&normalize_fact(&other_project.fact), &other_project)
        .unwrap();
    assert!(found.is_none());
}

#[test]
fn time_filters_exclude_expired_facts() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    let mut rec = record(
        "m1",
        "on call this week",
        MemoryScope::User,
        "u1",
        None,
        None,
    );
    rec.valid_to = Some("2026-01-02T00:00:00.000Z".to_string());
    insert(&store, &rec);
    let hits = store
        .search(&["call".into()], &filter("u1", None, None), NOW, 10)
        .unwrap();
    assert!(hits.is_empty());
    let hits = store
        .search(
            &["call".into()],
            &filter("u1", None, None),
            "2026-01-01T00:00:00.000Z",
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn validity_comparison_is_chronological_across_precision() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    let mut rec = record(
        "m1",
        "on call this week",
        MemoryScope::User,
        "u1",
        None,
        None,
    );
    rec.valid_to = Some("2026-01-01T08:00:00.100Z".to_string());
    insert(&store, &rec);
    // 0.5 ms after the expiry, expressed with microsecond precision. A lexical
    // comparison of the raw strings would wrongly keep the fact.
    let hits = store
        .search(
            &["call".into()],
            &filter("u1", None, None),
            "2026-01-01T08:00:00.100500+00:00",
            10,
        )
        .unwrap();
    assert!(hits.is_empty());
    let hits = store
        .search(
            &["call".into()],
            &filter("u1", None, None),
            "2026-01-01T08:00:00.099999Z",
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn stats_count_scopes_and_states() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record("u", "user fact", MemoryScope::User, "u1", None, None),
    );
    insert(
        &store,
        &record(
            "p",
            "project fact",
            MemoryScope::Project,
            "u1",
            Some("p1"),
            None,
        ),
    );
    insert(
        &store,
        &record(
            "r",
            "run fact",
            MemoryScope::Run,
            "u1",
            Some("p1"),
            Some("run1"),
        ),
    );
    store
        .invalidate("u", "2026-01-02T00:00:00.000Z", "old")
        .unwrap();
    let stats = store.stats("u1").unwrap();
    assert_eq!(stats.total, 3);
    assert_eq!(stats.active, 2);
    assert_eq!(stats.invalidated, 1);
    assert_eq!(stats.user_scope, 1);
    assert_eq!(stats.project_scope, 1);
    assert_eq!(stats.run_scope, 1);
}

#[test]
fn read_only_store_refuses_writes() {
    let dir = std::env::temp_dir().join(format!("phi-memory-{}", uuid::Uuid::new_v4()));
    let path = dir.join("memory.db");
    {
        let writable = SqliteMemoryStore::open(&path, true).unwrap();
        insert(
            &writable,
            &record("m1", "persisted fact", MemoryScope::User, "u1", None, None),
        );
    }
    let reader = SqliteMemoryStore::open(&path, false).unwrap();
    assert!(reader.get("m1").unwrap().is_some());
    let rec = record("m2", "new fact", MemoryScope::User, "u1", None, None);
    assert!(
        reader
            .insert(&rec, &normalize_fact(&rec.fact), &terms(&rec.fact), None)
            .is_err()
    );
    drop(reader);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_filters_by_text_scope_and_state() {
    let store = SqliteMemoryStore::open_in_memory().unwrap();
    insert(
        &store,
        &record(
            "m1",
            "Use pnpm for installs",
            MemoryScope::User,
            "u1",
            None,
            None,
        ),
    );
    insert(
        &store,
        &record(
            "m2",
            "Deploy on Fridays",
            MemoryScope::Project,
            "u1",
            Some("p1"),
            None,
        ),
    );
    let listed = store
        .list(&MemoryListFilter {
            user_id: "u1".to_string(),
            text: Some("pnpm".to_string()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "m1");
    let listed = store
        .list(&MemoryListFilter {
            user_id: "u1".to_string(),
            scopes: vec![MemoryScope::Project],
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "m2");
}
