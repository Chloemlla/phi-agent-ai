//! Integration tests for the memory plugin: admission, visibility, lifecycle
//! and content-free audit events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use phi_adapters::memory_sqlite::SqliteMemoryStore;
use phi_contracts::memory::{
    ForgetSelector, MemoryAddOutcome, MemoryAddRequest, MemoryEpisodeInput, MemoryListFilter,
    MemoryPort, MemoryQuery, Principal,
};
use phi_domain::memory::{
    AdmissionRejection, EpisodeSource, MemoryScope, MemoryState, SourceTrust,
};
use phi_kernel::event_bus::EventBus;
use phi_kernel::ports::{ClockPort, IdPort, JournalPort, LogPort};
use phi_plugins::memory::MemoryPlugin;

#[derive(Default)]
struct MemJournal {
    lines: Mutex<Vec<String>>,
}

impl JournalPort for MemJournal {
    fn append(&self, line: &str) -> Result<(), String> {
        self.lines.lock().unwrap().push(line.to_string());
        Ok(())
    }
    fn read_all(&self) -> Result<Vec<String>, String> {
        Ok(self.lines.lock().unwrap().clone())
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
    fn monotonic_ms(&self) -> u64 {
        self.tick.load(Ordering::SeqCst)
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

fn harness() -> (Arc<MemoryPlugin>, Arc<MemJournal>) {
    let journal = Arc::new(MemJournal::default());
    let bus = Arc::new(EventBus::new(
        journal.clone(),
        Arc::new(TestClock {
            tick: AtomicU64::new(0),
        }),
        Arc::new(TestIds {
            tick: AtomicU64::new(0),
        }),
        Arc::new(SilentLog),
    ));
    let store = Arc::new(SqliteMemoryStore::open_in_memory().unwrap());
    let plugin = Arc::new(MemoryPlugin::new(
        store,
        Arc::new(TestClock {
            tick: AtomicU64::new(1000),
        }),
        Arc::new(TestIds {
            tick: AtomicU64::new(0),
        }),
        bus,
        Arc::new(SilentLog),
        "node_test",
    ));
    (plugin, journal)
}

fn principal(project: &str) -> Principal {
    Principal {
        user_id: "node_test".to_string(),
        project_id: Some(project.to_string()),
        agent_id: Some("main".to_string()),
        run_id: Some("run_1".to_string()),
    }
}

fn request(fact: &str, scope: MemoryScope, trust: SourceTrust) -> MemoryAddRequest {
    MemoryAddRequest::new(fact, scope, trust)
}

fn add(plugin: &MemoryPlugin, principal: &Principal, fact: &str, scope: MemoryScope) -> String {
    match plugin
        .add(principal, request(fact, scope, SourceTrust::User))
        .unwrap()
    {
        MemoryAddOutcome::Admitted { record_id, .. } => record_id,
        other => panic!("expected admission, got {other:?}"),
    }
}

fn search(plugin: &MemoryPlugin, principal: &Principal, text: &str) -> Vec<String> {
    plugin
        .search(
            principal,
            &MemoryQuery {
                text: text.to_string(),
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap()
        .into_iter()
        .map(|hit| hit.record.fact)
        .collect()
}

#[test]
fn admitted_fact_is_retrievable_with_a_receipt() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let id = add(&plugin, &p, "The user prefers dark mode", MemoryScope::User);
    let hits = plugin
        .search(
            &p,
            &MemoryQuery {
                text: "dark mode".to_string(),
                limit: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.id, id);
    assert_eq!(hits[0].record.source_trust, SourceTrust::User);
    assert!(hits[0].score > 0.0);
    assert!(!hits[0].matched_terms.is_empty());
    assert!(
        hits[0].record.episode_id.is_some(),
        "receipts trace to an episode"
    );
}

#[test]
fn admission_rejects_empty_secret_and_overlong_candidates() {
    let (plugin, _) = harness();
    let p = principal("p1");
    for (fact, expected) in [
        ("   ", AdmissionRejection::Empty),
        ("---", AdmissionRejection::Empty),
        ("sk-test0000000000000000", AdmissionRejection::Secret),
        (&"a".repeat(3000), AdmissionRejection::TooLong),
    ] {
        match plugin
            .add(&p, request(fact, MemoryScope::User, SourceTrust::User))
            .unwrap()
        {
            MemoryAddOutcome::Rejected { reason, .. } => assert_eq!(reason, expected),
            other => panic!("expected rejection, got {other:?}"),
        }
    }
    assert_eq!(plugin.stats(&p).unwrap().total, 0);
}

#[test]
fn secrets_inside_a_fact_are_redacted_before_storage() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let outcome = plugin
        .add(
            &p,
            request(
                "The staging key sk-test0000000000000000 is used for CI",
                MemoryScope::User,
                SourceTrust::User,
            ),
        )
        .unwrap();
    let id = match outcome {
        MemoryAddOutcome::Admitted {
            record_id,
            redacted,
        } => {
            assert!(redacted);
            record_id
        }
        other => panic!("expected admission, got {other:?}"),
    };
    let record = plugin.get(&p, &id).unwrap().unwrap();
    assert!(!record.fact.contains("sk-test"));
    assert!(record.fact.contains("[REDACTED:api-key]"));
    assert!(record.redacted);
    let episode = plugin
        .episode(&p, record.episode_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert!(!episode.content.contains("sk-test"));
}

#[test]
fn duplicate_facts_are_idempotent() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let first = add(&plugin, &p, "Use pnpm, not npm", MemoryScope::Project);
    match plugin
        .add(
            &p,
            request(
                "use   PNPM, not npm",
                MemoryScope::Project,
                SourceTrust::User,
            ),
        )
        .unwrap()
    {
        MemoryAddOutcome::Duplicate { record_id } => assert_eq!(record_id, first),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(plugin.stats(&p).unwrap().total, 1);
}

#[test]
fn visibility_is_enforced_by_scope_not_by_callers() {
    let (plugin, _) = harness();
    let p1 = principal("project_one");
    let p2 = principal("project_two");
    add(
        &plugin,
        &p1,
        "Project one deploys with Fly",
        MemoryScope::Project,
    );
    add(
        &plugin,
        &p2,
        "Project two deploys with Railway",
        MemoryScope::Project,
    );
    add(
        &plugin,
        &p1,
        "The user uses a dark theme",
        MemoryScope::User,
    );

    let one = search(&plugin, &p1, "deploys");
    assert_eq!(one.len(), 1);
    assert!(one[0].contains("Fly"));
    assert!(!one.iter().any(|fact| fact.contains("Railway")));

    // The user-scoped preference is visible from another project.
    let other_project = search(&plugin, &p2, "dark theme");
    assert_eq!(other_project.len(), 1);

    // A different user shares nothing, even with the same project id.
    let mut intruder = principal("project_one");
    intruder.user_id = "someone_else".to_string();
    assert!(search(&plugin, &intruder, "deploys").is_empty());
    assert!(search(&plugin, &intruder, "dark theme").is_empty());
}

#[test]
fn agent_scope_is_project_bound() {
    let (plugin, _) = harness();
    let p1 = principal("project_one");
    let p2 = principal("project_two");
    add(
        &plugin,
        &p1,
        "The agent should always run cargo fmt",
        MemoryScope::Agent,
    );
    assert_eq!(search(&plugin, &p1, "cargo fmt").len(), 1);
    assert!(
        search(&plugin, &p2, "cargo fmt").is_empty(),
        "agent memories must not cross project boundaries"
    );
}

#[test]
fn run_scope_is_only_visible_inside_its_run() {
    let (plugin, _) = harness();
    let p = principal("p1");
    add(
        &plugin,
        &p,
        "Temporary conclusion about caching",
        MemoryScope::Run,
    );
    let hits = plugin
        .search(
            &p,
            &MemoryQuery {
                text: "caching".to_string(),
                limit: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);

    let mut outside = p.clone();
    outside.run_id = None;
    let hits = plugin
        .search(
            &outside,
            &MemoryQuery {
                text: "caching".to_string(),
                limit: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn invalidation_and_correction_preserve_history() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let id = add(&plugin, &p, "The API port is 8080", MemoryScope::Project);
    assert!(matches!(
        plugin
            .add(
                &p,
                request("A second fact", MemoryScope::Project, SourceTrust::User)
            )
            .unwrap(),
        MemoryAddOutcome::Admitted { .. }
    ));
    plugin.invalidate(&p, &id, "port changed").unwrap();
    assert!(search(&plugin, &p, "8080").is_empty());
    let record = plugin.get(&p, &id).unwrap().unwrap();
    assert_eq!(record.state, MemoryState::Invalidated);
    assert_eq!(record.reason.as_deref(), Some("port changed"));

    let corrected = add(&plugin, &p, "The API port is 9090", MemoryScope::Project);
    match plugin
        .correct(&p, &corrected, "The API port is 9091", "typo")
        .unwrap()
    {
        MemoryAddOutcome::Admitted { record_id, .. } => {
            let old = plugin.get(&p, &corrected).unwrap().unwrap();
            assert_eq!(old.state, MemoryState::Superseded);
            assert_eq!(old.superseded_by.as_deref(), Some(record_id.as_str()));
            let new = plugin.get(&p, &record_id).unwrap().unwrap();
            assert_eq!(new.state, MemoryState::Active);
            assert_eq!(new.reason.as_deref(), Some("typo"));
        }
        other => panic!("expected correction, got {other:?}"),
    }
    let facts = search(&plugin, &p, "API port");
    assert!(facts.iter().any(|fact| fact.contains("9091")));
    assert!(!facts.iter().any(|fact| fact.contains("9090")));
}

#[test]
fn forget_removes_the_complete_memory_chain() {
    let (plugin, journal) = harness();
    let p = principal("p1");
    let mut req = request(
        "The deploy token is rotated weekly",
        MemoryScope::Project,
        SourceTrust::User,
    );
    req.episode = Some(MemoryEpisodeInput {
        content: "Full conversation excerpt about deployments".to_string(),
        source: EpisodeSource::UserInput,
        source_label: Some("test".to_string()),
        session_id: Some("sess_1".to_string()),
    });
    let id = match plugin.add(&p, req).unwrap() {
        MemoryAddOutcome::Admitted { record_id, .. } => record_id,
        other => panic!("expected admission, got {other:?}"),
    };
    let record = plugin.get(&p, &id).unwrap().unwrap();
    let episode_id = record.episode_id.clone().unwrap();

    let report = plugin
        .forget(&p, ForgetSelector::ids(vec![id.clone()]), "user asked")
        .unwrap();
    assert_eq!(report.records, 1);
    assert_eq!(report.episodes, 1);
    assert!(plugin.get(&p, &id).unwrap().is_none());
    assert!(plugin.episode(&p, &episode_id).unwrap().is_none());
    assert!(search(&plugin, &p, "deploy token").is_empty());
    assert_eq!(plugin.stats(&p).unwrap().total, 0);

    let lines = journal.read_all().unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("\"MemoryForgotten\""))
    );
}

#[test]
fn empty_forget_selector_is_refused() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let id = add(&plugin, &p, "The user prefers tabs", MemoryScope::User);
    assert!(
        plugin
            .forget(&p, ForgetSelector::default(), "oops")
            .is_err()
    );
    assert!(plugin.get(&p, &id).unwrap().is_some());
    let report = plugin.forget(&p, ForgetSelector::all(), "cleanup").unwrap();
    assert_eq!(report.records, 1);
}

#[test]
fn very_long_queries_are_bounded_and_still_work() {
    let (plugin, _) = harness();
    let p = principal("p1");
    add(
        &plugin,
        &p,
        "The deploy target is fly.io",
        MemoryScope::Project,
    );
    let mut query = String::from("deploy target");
    for index in 0..2000 {
        query.push_str(&format!(" filler{index}"));
    }
    let hits = plugin
        .search(
            &p,
            &MemoryQuery {
                text: query,
                limit: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].record.fact.contains("fly.io"));
}

#[test]
fn another_user_cannot_forget_or_read_records() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let id = add(&plugin, &p, "The user prefers tabs", MemoryScope::User);

    let mut other = p.clone();
    other.user_id = "other_user".to_string();
    assert!(plugin.get(&other, &id).unwrap().is_none());
    let report = plugin
        .forget(&other, ForgetSelector::ids(vec![id.clone()]), "nope")
        .unwrap();
    assert_eq!(report.records, 0);
    assert!(plugin.get(&p, &id).unwrap().is_some());

    let listed = plugin
        .list(
            &other,
            &MemoryListFilter {
                user_id: "node_test".to_string(),
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        listed.is_empty(),
        "listing must ignore caller-supplied user ids"
    );
}

#[test]
fn correction_keeps_run_attribution_without_a_run_principal() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let id = add(
        &plugin,
        &p,
        "Run finding: the cache key is wrong",
        MemoryScope::Run,
    );
    let mut outside = p.clone();
    outside.run_id = None;
    let corrected = match plugin
        .correct(
            &outside,
            &id,
            "Run finding: the cache key is user id",
            "fix",
        )
        .unwrap()
    {
        MemoryAddOutcome::Admitted { record_id, .. } => record_id,
        other => panic!("expected correction, got {other:?}"),
    };
    let record = plugin.get(&p, &corrected).unwrap().unwrap();
    assert_eq!(record.scope, MemoryScope::Run);
    assert_eq!(record.run_id.as_deref(), Some("run_1"));
}

#[test]
fn non_finite_confidence_is_rejected() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let mut req = request("uses pnpm", MemoryScope::User, SourceTrust::User);
    req.confidence = Some(f64::NAN);
    match plugin.add(&p, req).unwrap() {
        MemoryAddOutcome::Rejected { reason, .. } => {
            assert_eq!(reason, AdmissionRejection::InvalidConfidence);
        }
        other => panic!("expected rejection, got {other:?}"),
    }
    let mut req = request("uses bun", MemoryScope::User, SourceTrust::User);
    req.confidence = Some(f64::INFINITY);
    match plugin.add(&p, req).unwrap() {
        MemoryAddOutcome::Rejected { reason, .. } => {
            assert_eq!(reason, AdmissionRejection::InvalidConfidence);
        }
        other => panic!("expected rejection, got {other:?}"),
    }
    assert_eq!(plugin.stats(&p).unwrap().total, 0);
}

#[test]
fn concurrent_duplicate_writes_admit_exactly_one() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let mut handles = Vec::new();
    for _ in 0..8 {
        let plugin = plugin.clone();
        let p = p.clone();
        handles.push(std::thread::spawn(move || {
            plugin
                .add(
                    &p,
                    MemoryAddRequest::new(
                        "The user prefers pnpm",
                        MemoryScope::Project,
                        SourceTrust::User,
                    ),
                )
                .unwrap()
        }));
    }
    let mut admitted = 0;
    let mut duplicates = 0;
    for handle in handles {
        match handle.join().unwrap() {
            MemoryAddOutcome::Admitted { .. } => admitted += 1,
            MemoryAddOutcome::Duplicate { .. } => duplicates += 1,
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    assert_eq!(admitted, 1);
    assert_eq!(duplicates, 7);
    assert_eq!(plugin.stats(&p).unwrap().total, 1);
}

#[test]
fn external_content_cannot_claim_high_confidence() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let mut req = request(
        "A website claims the moon is cheese",
        MemoryScope::User,
        SourceTrust::External,
    );
    req.confidence = Some(0.99);
    let id = match plugin.add(&p, req).unwrap() {
        MemoryAddOutcome::Admitted { record_id, .. } => record_id,
        other => panic!("expected admission, got {other:?}"),
    };
    let record = plugin.get(&p, &id).unwrap().unwrap();
    assert!((record.confidence - 0.5).abs() < f64::EPSILON);
}

#[test]
fn expired_facts_are_not_retrieved() {
    let (plugin, _) = harness();
    let p = principal("p1");
    let mut req = request(
        "The sprint ends on Friday",
        MemoryScope::Project,
        SourceTrust::User,
    );
    req.valid_to = Some("2025-12-31T00:00:00Z".to_string());
    assert!(matches!(
        plugin.add(&p, req).unwrap(),
        MemoryAddOutcome::Admitted { .. }
    ));
    assert!(search(&plugin, &p, "sprint").is_empty());
}

#[test]
fn audit_events_never_contain_fact_text() {
    let (plugin, journal) = harness();
    let p = principal("p1");
    let fact = "The internal staging hostname is staging-owl";
    let id = add(&plugin, &p, fact, MemoryScope::User);
    plugin
        .forget(&p, ForgetSelector::ids(vec![id]), "done")
        .unwrap();
    for line in journal.read_all().unwrap() {
        assert!(
            !line.contains("staging-owl"),
            "journal must not retain memory content: {line}"
        );
    }
    let lines = journal.read_all().unwrap();
    assert!(lines.iter().any(|line| line.contains("MemoryAdmitted")));
    assert!(lines.iter().any(|line| line.contains("MemoryForgotten")));
}
