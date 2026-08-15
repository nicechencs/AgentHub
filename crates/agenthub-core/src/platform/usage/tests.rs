//! Usage platform unit tests (separate from production modules).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::usage::{
    builtin_usage_registry, collect_with_source_for_agent_id, UsageFileParser, UsageLineOutcome,
    UsageSource, UsageSourceRegistry,
};
use crate::platform::AgentKey;
use crate::services::UsageService;
use crate::storage::{Database, UsageRepo};

struct TestUsageSource {
    key: AgentKey,
    files: Vec<PathBuf>,
    discoveries: Arc<AtomicUsize>,
}

impl UsageSource for TestUsageSource {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        self.discoveries.fetch_add(1, Ordering::SeqCst);
        Ok(self.files.clone())
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        panic!("no-file source must not begin parsing")
    }
}

fn no_file_source(key: &str) -> (Arc<dyn UsageSource>, Arc<AtomicUsize>) {
    usage_source(key, Vec::new())
}

fn usage_source(key: &str, files: Vec<PathBuf>) -> (Arc<dyn UsageSource>, Arc<AtomicUsize>) {
    let discoveries = Arc::new(AtomicUsize::new(0));
    let source: Arc<dyn UsageSource> = Arc::new(TestUsageSource {
        key: AgentKey::parse(key).unwrap(),
        files,
        discoveries: Arc::clone(&discoveries),
    });
    (source, discoveries)
}

#[test]
fn builtin_covers_usage_agents_not_cursor() {
    let reg = builtin_usage_registry();
    assert!(reg.contains(AgentId::Claude));
    assert!(reg.contains(AgentId::Codex));
    assert!(reg.contains(AgentId::Kimi));
    assert!(reg.contains(AgentId::Grok));
    assert!(reg.contains(AgentId::Pi));
    assert!(reg.contains(AgentId::WorkBuddy));
    assert!(reg.contains(AgentId::Dsh));
    assert!(!reg.contains(AgentId::Cursor));
    assert_eq!(reg.supported_agents().len(), 7);
}

#[test]
fn unknown_agent_key_registers_queries_and_executes() {
    let unknown = AgentKey::parse("third-party-usage").unwrap();
    let (source, discoveries) = no_file_source(unknown.as_str());
    let mut reg = UsageSourceRegistry::new();
    reg.register(source).unwrap();

    let found = reg.get(&unknown).expect("unknown key must be queryable");
    assert_eq!(found.agent_key(), unknown);
    assert_eq!(reg.supported_agent_keys(), vec![unknown.clone()]);

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let service = UsageService::with_registry(db, reg);
    let stats = service.collect_agent_key(&unknown).unwrap();
    assert!(stats.events.is_empty());
    assert!(stats.cursors.is_empty());
    assert_eq!(discoveries.load(Ordering::SeqCst), 1);
}

#[test]
fn unregistered_agent_key_is_typed_unsupported() {
    let key = AgentKey::parse("unregistered-usage").unwrap();
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let service = UsageService::with_registry(db, UsageSourceRegistry::new());

    let error = match service.collect_agent_key(&key) {
        Ok(_) => panic!("unregistered usage key must be unsupported"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "unsupported");
    assert!(error.to_string().contains(key.as_str()));
}

#[test]
fn key_native_service_reports_legacy_persistence_boundary_for_discovered_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.jsonl");
    fs::write(&path, "{}\n").unwrap();

    let key = AgentKey::parse("third-party-with-data").unwrap();
    let (source, discoveries) = usage_source(key.as_str(), vec![path]);
    let mut reg = UsageSourceRegistry::new();
    reg.register(source).unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let service = UsageService::with_registry(db, reg);

    let error = match service.collect_agent_key(&key) {
        Ok(_) => panic!("discovered data must require a legacy persistence identity"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("legacy AgentId"));
    assert_eq!(discoveries.load(Ordering::SeqCst), 1);
}

#[test]
fn legacy_collect_delegates_through_injected_key_registry() {
    let (source, discoveries) = no_file_source(AgentId::Codex.as_str());
    let mut reg = UsageSourceRegistry::new();
    reg.register(source).unwrap();
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let service = UsageService::with_registry(db, reg);

    let result = service.collect(Some(AgentId::Codex)).unwrap();
    assert_eq!(discoveries.load(Ordering::SeqCst), 1);
    assert_eq!(result.agents.len(), 1);
    assert!(result.agents[0].supported);
}

#[test]
fn duplicate_agent_key_is_rejected_without_reordering() {
    let (first, _) = no_file_source("zeta-usage");
    let (duplicate, _) = no_file_source("zeta-usage");
    let (second, _) = no_file_source("alpha-usage");
    let mut reg = UsageSourceRegistry::new();

    reg.register(first).unwrap();
    let err = reg.register(duplicate).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("duplicate usage source key"));
    reg.register(second).unwrap();

    assert_eq!(
        reg.supported_agent_keys(),
        vec![
            AgentKey::parse("zeta-usage").unwrap(),
            AgentKey::parse("alpha-usage").unwrap(),
        ]
    );
}

#[test]
fn legacy_agent_id_helper_delegates_to_agent_key_lookup() {
    let reg = builtin_usage_registry();
    let key = AgentKey::from_agent_id(AgentId::Codex);
    let by_key = reg.get(&key).unwrap();
    let by_legacy = reg.get_agent_id(AgentId::Codex).unwrap();

    assert!(Arc::ptr_eq(&by_key, &by_legacy));
    assert!(reg.get_agent_id(AgentId::Cursor).is_none());
}

/// Parser that accepts any line as skipped (never fails mid-file).
struct SkipAllParser;

impl UsageFileParser for SkipAllParser {
    fn on_line(&mut self, _line: &str, _session_id: Option<&str>) -> UsageLineOutcome {
        UsageLineOutcome::Skipped
    }
}

struct MultiPathUsageSource {
    key: AgentKey,
    files: Vec<PathBuf>,
}

impl UsageSource for MultiPathUsageSource {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        Ok(self.files.clone())
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(SkipAllParser)
    }
}

/// Token layout repair is one-shot: first collect migrates, second is a no-op.
///
/// Uses an empty UsageSource registry so we never scan the live `~/.codex` tree
/// (which would re-fill rows and make the test slow/non-hermetic).
#[test]
fn token_layout_repair_runs_once_then_skips() {
    use crate::models::{UsageQuery, UsageRecord};
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db.clone());

    let seed = || {
        let row = UsageRecord {
            id: Uuid::new_v4().to_string(),
            agent_id: AgentId::Codex,
            account_id: None,
            model: "gpt-5.6-luna".into(),
            input_tokens: 750,
            output_tokens: 10,
            cache_tokens: 250,
            cost_usd: Some(0.01),
            session_id: Some("s1".into()),
            ts: "2026-08-07T00:00:00.000Z".into(),
            raw_hash: Some(format!("hash-{}", Uuid::new_v4())),
        };
        repo.insert_batch(&[row]).unwrap();
    };
    seed();
    assert_eq!(
        repo.query(&UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            model: None,
        })
        .unwrap()
        .len(),
        1
    );

    // Empty registry: repair still runs, but no live session scan.
    let service = UsageService::with_registry(db.clone(), UsageSourceRegistry::new());
    let _r1 = service.collect(Some(AgentId::Codex)).unwrap();
    assert_eq!(
        db.get_setting("usage_token_layout").unwrap().as_deref(),
        Some("3")
    );
    assert!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Codex),
                model: None,
            })
            .unwrap()
            .is_empty(),
        "layout repair must clear seeded rows"
    );

    // After migration, a new seed must survive the next collect.
    seed();
    let _r2 = service.collect(Some(AgentId::Codex)).unwrap();
    assert_eq!(
        db.get_setting("usage_token_layout").unwrap().as_deref(),
        Some("3")
    );
    assert_eq!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Codex),
                model: None,
            })
            .unwrap()
            .len(),
        1,
        "second collect must not wipe rows again"
    );
}

/// Grok parser rewrite is one-shot and must not wipe other agents.
#[test]
fn grok_parser_repair_clears_only_grok_rows_once() {
    use crate::models::{UsageQuery, UsageRecord};
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db.clone());
    db.set_setting("usage_token_layout", "3").unwrap();

    let seed = |agent: AgentId, hash: &str| {
        repo.insert_batch(&[UsageRecord {
            id: Uuid::new_v4().to_string(),
            agent_id: agent,
            account_id: None,
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 1,
            cache_tokens: 0,
            cost_usd: Some(0.01),
            session_id: Some("s1".into()),
            ts: "2026-08-07T00:00:00.000Z".into(),
            raw_hash: Some(hash.into()),
        }])
        .unwrap();
    };
    seed(AgentId::Grok, "grok-old");
    seed(AgentId::Claude, "claude-keep");

    let service = UsageService::with_registry(db.clone(), UsageSourceRegistry::new());
    let _ = service.collect(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        db.get_setting("usage_grok_parser").unwrap().as_deref(),
        Some("1")
    );
    assert!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Grok),
                model: None,
            })
            .unwrap()
            .is_empty(),
        "first collect must drop stale Grok rows"
    );
    assert_eq!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Claude),
                model: None,
            })
            .unwrap()
            .len(),
        1,
        "Grok parser repair must not wipe other agents"
    );

    seed(AgentId::Grok, "grok-new");
    let _ = service.collect(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Grok),
                model: None,
            })
            .unwrap()
            .len(),
        1,
        "second collect must not wipe Grok rows again"
    );
}

#[test]
fn usage_repo_clears_and_resets_one_agent() {
    use crate::models::UsageRecord;
    use crate::storage::UsageCursor;
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);

    let row = |agent: AgentId, hash: &str| UsageRecord {
        id: Uuid::new_v4().to_string(),
        agent_id: agent,
        account_id: None,
        model: "m".into(),
        input_tokens: 1,
        output_tokens: 1,
        cache_tokens: 0,
        cost_usd: Some(0.01),
        session_id: Some("s".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some(hash.into()),
    };
    repo.insert_batch(&[row(AgentId::Grok, "g"), row(AgentId::Claude, "c")])
        .unwrap();
    repo.insert_batch_and_cursors(
        &[],
        &[
            UsageCursor {
                path: "grok.jsonl".into(),
                agent_id: AgentId::Grok,
                byte_offset: 99,
                file_mtime: 1,
            },
            UsageCursor {
                path: "claude.jsonl".into(),
                agent_id: AgentId::Claude,
                byte_offset: 88,
                file_mtime: 1,
            },
        ],
    )
    .unwrap();

    assert_eq!(repo.clear_records_for_agent(AgentId::Grok).unwrap(), 1);
    assert_eq!(repo.reset_cursors_for_agent(AgentId::Grok).unwrap(), 1);

    let grok_left = repo
        .query(&crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Grok),
            model: None,
        })
        .unwrap();
    let claude_left = repo
        .query(&crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Claude),
            model: None,
        })
        .unwrap();
    assert!(grok_left.is_empty());
    assert_eq!(claude_left.len(), 1);
    assert_eq!(
        repo.get_cursor("grok.jsonl").unwrap().unwrap().byte_offset,
        0
    );
    assert_eq!(
        repo.get_cursor("claude.jsonl")
            .unwrap()
            .unwrap()
            .byte_offset,
        88
    );
}

/// Recompute must not peel non-cached Codex input (double-peel regression).
#[test]
fn recompute_costs_preserves_codex_billable_input() {
    use crate::models::UsageRecord;
    use crate::usage::codex_billable_tokens;
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    // Mark layout current so collect path is not required.
    db.set_setting("usage_token_layout", "3").unwrap();
    let repo = UsageRepo::new(db.clone());

    let row = UsageRecord {
        id: Uuid::new_v4().to_string(),
        agent_id: AgentId::Codex,
        account_id: None,
        model: "gpt-5.6-luna".into(),
        // Non-cached storage: full was 1000, cache 250 → billable 750
        input_tokens: 750,
        output_tokens: 10,
        cache_tokens: 250,
        cost_usd: Some(0.0),
        session_id: Some("s1".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("hash-peel".into()),
    };
    repo.insert_batch(&[row]).unwrap();

    let service = UsageService::new(db);
    // Multiple recompute passes must stay at 750 (old heuristic → 500 → 250 → 0).
    for _ in 0..5 {
        service.recompute_stored_costs().unwrap();
    }
    let rows = service
        .query(crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            model: None,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 750);
    assert_eq!(codex_billable_tokens(750, 250), (750, 250));
    assert!(rows[0].cost_usd.unwrap_or(0.0) > 0.0);
}

/// UPSERT overwrites token fields for the same dedupe key (repair path).
#[test]
fn usage_upsert_repairs_token_fields_on_conflict() {
    use crate::models::UsageRecord;
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);

    let id1 = Uuid::new_v4().to_string();
    let base = UsageRecord {
        id: id1.clone(),
        agent_id: AgentId::Codex,
        account_id: None,
        model: "gpt-5.6-luna".into(),
        input_tokens: 500, // wrong (double-peeled)
        output_tokens: 10,
        cache_tokens: 250,
        cost_usd: Some(0.001),
        session_id: Some("sess".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("same-hash".into()),
    };
    assert_eq!(repo.insert_batch(&[base.clone()]).unwrap(), 1);

    let fixed = UsageRecord {
        id: Uuid::new_v4().to_string(), // new id, same dedupe key
        input_tokens: 750,
        cost_usd: Some(0.01),
        ..base
    };
    // UPSERT should update in place (1 change).
    assert_eq!(repo.insert_batch(&[fixed]).unwrap(), 1);

    let rows = repo
        .query(&crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            model: None,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 750);
    assert!((rows[0].cost_usd.unwrap() - 0.01).abs() < 1e-9);
    // Primary key may stay the original id (ON CONFLICT does not replace id).
    assert_eq!(rows[0].id, id1);
}

/// Missing discovered paths must increment `failed` and not abort the whole collect.
#[test]
fn collect_missing_file_increments_failed_and_continues() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing-session.jsonl");
    let good = root.path().join("ok-session.jsonl");
    fs::write(&good, "{}\n").unwrap();

    let source = MultiPathUsageSource {
        key: AgentKey::from_agent_id(AgentId::Claude),
        files: vec![missing, good.clone()],
    };

    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let stats = collect_with_source_for_agent_id(&source, AgentId::Claude, &repo).unwrap();

    assert_eq!(
        stats.failed, 1,
        "missing path must count as one file-level failure"
    );
    assert_eq!(
        stats.cursors.len(),
        1,
        "good file must still produce a cursor"
    );
    assert_eq!(stats.cursors[0].path, good.to_string_lossy());
    // empty/uninteresting line → skipped inside the good file
    assert!(stats.skipped >= 1 || stats.events.is_empty());
}
