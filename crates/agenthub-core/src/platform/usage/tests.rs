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
    assert!(reg.contains(AgentId::Zcode));
    assert!(!reg.contains(AgentId::Cursor));
    assert_eq!(reg.supported_agents().len(), 8);
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
            cache_read_tokens: 250,
            cache_write_tokens: 0,
            cost_usd: Some(0.01),
            session_id: Some("s1".into()),
            ts: "2026-08-07T00:00:00.000Z".into(),
            raw_hash: Some(format!("hash-{}", Uuid::new_v4())),
            fast: false,
        };
        repo.insert_batch(&[row]).unwrap();
    };
    seed();
    assert_eq!(
        repo.query(&UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            model: None,
            ..Default::default()
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
        Some("5")
    );
    assert!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Codex),
                model: None,
                ..Default::default()
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
        Some("5")
    );
    assert_eq!(
        service
            .query(UsageQuery {
                days: 30,
                agent_id: Some(AgentId::Codex),
                model: None,
                ..Default::default()
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
    db.set_setting("usage_token_layout", "5").unwrap();

    let seed = |agent: AgentId, hash: &str| {
        repo.insert_batch(&[UsageRecord {
            id: Uuid::new_v4().to_string(),
            agent_id: agent,
            account_id: None,
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: Some(0.01),
            session_id: Some("s1".into()),
            ts: "2026-08-07T00:00:00.000Z".into(),
            raw_hash: Some(hash.into()),
            fast: false,
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: Some(0.01),
        session_id: Some("s".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some(hash.into()),
        fast: false,
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
            ..Default::default()
        })
        .unwrap();
    let claude_left = repo
        .query(&crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Claude),
            model: None,
            ..Default::default()
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
    db.set_setting("usage_token_layout", "5").unwrap();
    let repo = UsageRepo::new(db.clone());

    let row = UsageRecord {
        id: Uuid::new_v4().to_string(),
        agent_id: AgentId::Codex,
        account_id: None,
        model: "gpt-5.6-luna".into(),
        // Non-cached storage: full was 1000, cache 250 → billable 750
        input_tokens: 750,
        output_tokens: 10,
        cache_read_tokens: 250,
        cache_write_tokens: 0,
        cost_usd: Some(0.0),
        session_id: Some("s1".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("hash-peel".into()),
        fast: false,
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
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 750);
    assert_eq!(codex_billable_tokens(750, 250), (750, 250));
    assert!(rows[0].cost_usd.unwrap_or(0.0) > 0.0);
}

#[test]
fn recompute_keeps_codex_fast_multiplier() {
    use crate::models::UsageRecord;
    use crate::usage::{estimate_cost_usd_for_agent, CostTokens};
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    db.set_setting("usage_token_layout", "5").unwrap();
    let repo = UsageRepo::new(db.clone());

    let row = UsageRecord {
        id: Uuid::new_v4().to_string(),
        agent_id: AgentId::Codex,
        account_id: None,
        model: "gpt-5.6-sol".into(),
        input_tokens: 100_000,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: Some(0.0),
        session_id: Some("s1".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("hash-fast".into()),
        fast: true,
    };
    repo.insert_batch(&[row]).unwrap();

    let service = UsageService::new(db);
    service.recompute_stored_costs().unwrap();
    let rows = service
        .query(crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            model: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].fast);
    let expected = estimate_cost_usd_for_agent(
        AgentId::Codex,
        "gpt-5.6-sol",
        CostTokens {
            input: 100_000,
            fast: true,
            ..CostTokens::default()
        },
        None,
    );
    assert!(
        (rows[0].cost_usd.unwrap_or(0.0) - expected).abs() < 1e-9,
        "got {:?} want {expected}",
        rows[0].cost_usd
    );
    assert!((expected - 0.8).abs() < 0.01, "expected {expected}");
}

#[test]
fn recompute_preserves_log_cost_for_unknown_model() {
    use crate::models::UsageRecord;
    use uuid::Uuid;

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    db.set_setting("usage_token_layout", "5").unwrap();
    let repo = UsageRepo::new(db.clone());

    let row = UsageRecord {
        id: Uuid::new_v4().to_string(),
        agent_id: AgentId::Grok,
        account_id: None,
        model: "no-such-vendor/definitely-not-priced-zzz".into(),
        input_tokens: 10,
        output_tokens: 2,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: Some(1.23),
        session_id: Some("s1".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("hash-log-cost".into()),
        fast: false,
    };
    repo.insert_batch(&[row]).unwrap();

    let service = UsageService::new(db);
    service.recompute_stored_costs().unwrap();
    let rows = service
        .query(crate::models::UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Grok),
            model: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!((rows[0].cost_usd.unwrap_or(0.0) - 1.23).abs() < 1e-9);
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
        cache_read_tokens: 250,
        cache_write_tokens: 0,
        cost_usd: Some(0.001),
        session_id: Some("sess".into()),
        ts: "2026-08-07T00:00:00.000Z".into(),
        raw_hash: Some("same-hash".into()),
        fast: false,
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
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 750);
    assert_eq!(rows[0].cache_read_tokens, 250);
    assert_eq!(rows[0].cache_write_tokens, 0);
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

#[test]
fn collect_and_parser_health_skip_uninstalled_and_hidden_agents() {
    let (claude, claude_n) = no_file_source(AgentId::Claude.as_str());
    let (codex, codex_n) = no_file_source(AgentId::Codex.as_str());
    let (grok, grok_n) = no_file_source(AgentId::Grok.as_str());
    let mut reg = UsageSourceRegistry::new();
    reg.register(claude).unwrap();
    reg.register(codex).unwrap();
    reg.register(grok).unwrap();

    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    // Only Grok is installed && !hidden — Claude uninstalled, Codex hidden.
    let service = UsageService::with_visible_installed(db, reg, [AgentId::Grok]);

    let result = service.collect(None).unwrap();
    assert_eq!(
        claude_n.load(Ordering::SeqCst),
        0,
        "uninstalled must not be scanned"
    );
    assert_eq!(
        codex_n.load(Ordering::SeqCst),
        0,
        "hidden must not be scanned"
    );
    assert_eq!(
        grok_n.load(Ordering::SeqCst),
        1,
        "visible installed must be scanned"
    );
    assert_eq!(result.agents.len(), 1);
    assert_eq!(result.agents[0].agent_id, AgentId::Grok);

    let health = service.parser_health().unwrap();
    assert_eq!(
        health.iter().map(|h| h.agent_id).collect::<Vec<_>>(),
        vec![AgentId::Grok]
    );

    let skipped = service.collect(Some(AgentId::Claude)).unwrap();
    assert_eq!(claude_n.load(Ordering::SeqCst), 0);
    assert!(skipped.agents.is_empty());
}

#[test]
fn collect_without_scope_still_walks_registered_agents() {
    let (claude, claude_n) = no_file_source(AgentId::Claude.as_str());
    let (grok, grok_n) = no_file_source(AgentId::Grok.as_str());
    let mut reg = UsageSourceRegistry::new();
    reg.register(claude).unwrap();
    reg.register(grok).unwrap();
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let service = UsageService::with_registry(db, reg);

    service.collect(None).unwrap();
    assert_eq!(claude_n.load(Ordering::SeqCst), 1);
    assert_eq!(grok_n.load(Ordering::SeqCst), 1);
}

fn recent_ts(hours_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(hours_ago))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn overview_row(
    agent: AgentId,
    model: &str,
    input: i64,
    output: i64,
    cache: i64,
    cost: f64,
    ts: &str,
    hash: &str,
) -> crate::models::UsageRecord {
    crate::models::UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: agent,
        account_id: None,
        model: model.into(),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache,
        cache_write_tokens: 0,
        cost_usd: Some(cost),
        session_id: Some("s".into()),
        ts: ts.into(),
        raw_hash: Some(hash.into()),
        fast: false,
    }
}

#[test]
fn usage_overview_sums_and_groups_by_agent_or_model() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let ts = recent_ts(2);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 100, 20, 10, 1.5, &ts, "h1"),
        overview_row(AgentId::Claude, "sonnet", 50, 5, 0, 0.5, &ts, "h2"),
        overview_row(AgentId::Kimi, "k2", 30, 3, 2, 0.25, &ts, "h3"),
    ])
    .unwrap();

    let all = repo.overview(7, None, None, None, &[]).unwrap();
    assert_eq!(all.metrics.billable_input, 180);
    assert_eq!(all.metrics.output, 28);
    assert_eq!(all.metrics.cache_read, 12);
    assert_eq!(all.metrics.cache_write, 0);
    assert!((all.metrics.cost_usd - 2.25).abs() < 1e-9);
    assert_eq!(
        all.distribution
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>(),
        vec!["claude", "kimi"]
    );
    assert_eq!(all.distribution[0].tokens, 185);
    assert_eq!(all.distribution[0].billable_input, 150);
    assert_eq!(all.distribution[1].tokens, 35);
    assert_eq!(all.models, vec!["k2", "opus", "sonnet"]);

    let claude = repo
        .overview(7, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert_eq!(claude.metrics.billable_input, 150);
    assert_eq!(
        claude
            .distribution
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>(),
        vec!["opus", "sonnet"]
    );
    assert_eq!(claude.distribution[0].tokens, 130);
    assert_eq!(claude.models, vec!["opus", "sonnet"]);

    let opus = repo
        .overview(7, Some(AgentId::Claude), Some("opus"), None, &[])
        .unwrap();
    assert_eq!(opus.metrics.billable_input, 100);
    assert_eq!(opus.distribution.len(), 1);
    assert_eq!(opus.distribution[0].key, "opus");
    assert_eq!(
        opus.models,
        vec!["opus", "sonnet"],
        "models list ignores the selected model filter"
    );
}

#[test]
fn usage_overview_splits_cache_read_and_write() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let ts = recent_ts(1);
    repo.insert_batch(&[crate::models::UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: AgentId::Claude,
        account_id: None,
        model: "opus".into(),
        input_tokens: 100,
        output_tokens: 20,
        cache_read_tokens: 40,
        cache_write_tokens: 25,
        cost_usd: Some(1.0),
        session_id: Some("s".into()),
        ts: ts.clone(),
        raw_hash: Some("split".into()),
        fast: false,
    }])
    .unwrap();

    let all = repo.overview(7, None, None, None, &[]).unwrap();
    assert_eq!(all.metrics.billable_input, 100);
    assert_eq!(all.metrics.output, 20);
    assert_eq!(all.metrics.cache_read, 40);
    assert_eq!(all.metrics.cache_write, 25);
    assert_eq!(all.distribution[0].tokens, 185);
    assert_eq!(all.distribution[0].cache_read, 40);
    assert_eq!(all.distribution[0].cache_write, 25);

    let rows = repo
        .query(&crate::models::UsageQuery {
            days: 7,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows[0].cache_read_tokens, 40);
    assert_eq!(rows[0].cache_write_tokens, 25);
}

#[test]
fn usage_query_honors_limit_and_since() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let older = recent_ts(20);
    let newer = recent_ts(1);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 1, 1, 0, 0.1, &older, "old"),
        overview_row(AgentId::Claude, "opus", 2, 2, 0, 0.2, &newer, "new-a"),
        overview_row(AgentId::Claude, "opus", 3, 3, 0, 0.3, &newer, "new-b"),
    ])
    .unwrap();

    let limited = repo
        .query(&crate::models::UsageQuery {
            days: 2,
            agent_id: Some(AgentId::Claude),
            limit: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(limited.len(), 2);

    let since = recent_ts(5);
    let since_rows = repo
        .query(&crate::models::UsageQuery {
            days: 2,
            agent_id: Some(AgentId::Claude),
            since: Some(since.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(since_rows.len(), 2);
    assert!(since_rows.iter().all(|r| r.ts >= since));
}

#[test]
fn usage_trend_filters_by_model_and_since() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let older = recent_ts(20);
    let newer = recent_ts(1);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 100, 10, 0, 1.0, &newer, "opus"),
        overview_row(AgentId::Claude, "sonnet", 50, 5, 0, 0.5, &newer, "sonnet"),
        overview_row(AgentId::Claude, "opus", 20, 2, 0, 0.2, &older, "old-opus"),
    ])
    .unwrap();

    fn claude_tokens(points: &[crate::models::UsageTrendPoint]) -> i64 {
        points
            .iter()
            .map(|p| p.0.get("claude").and_then(|v| v.as_i64()).unwrap_or(0))
            .sum()
    }

    let all = repo
        .trend(2, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert_eq!(
        claude_tokens(&all),
        187,
        "input+cache+output for all models in the 2-day window"
    );

    let opus = repo
        .trend(2, Some(AgentId::Claude), Some("opus"), None, &[])
        .unwrap();
    assert_eq!(claude_tokens(&opus), 132);

    let since = recent_ts(5);
    let recent = repo
        .trend(
            2,
            Some(AgentId::Claude),
            Some("opus"),
            Some(since.as_str()),
            &[],
        )
        .unwrap();
    assert_eq!(claude_tokens(&recent), 110);
}

#[test]
fn usage_overview_and_query_exclude_hidden_agents() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let ts = recent_ts(1);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 100, 10, 0, 1.0, &ts, "c"),
        overview_row(AgentId::Kimi, "k2", 999, 9, 0, 9.0, &ts, "k"),
    ])
    .unwrap();

    let hidden = [AgentId::Kimi];
    let overview = repo.overview(7, None, None, None, &hidden).unwrap();
    assert_eq!(overview.metrics.billable_input, 100);
    assert_eq!(overview.distribution.len(), 1);
    assert_eq!(overview.distribution[0].key, "claude");
    assert_eq!(overview.models, vec!["opus"]);

    let rows = repo
        .query(&crate::models::UsageQuery {
            days: 7,
            limit: Some(1),
            exclude_agent_ids: hidden.to_vec(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].agent_id, AgentId::Claude);
}

#[test]
fn usage_trend_days1_rolling_includes_20h_unless_since_clips() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let older = recent_ts(20);
    let newer = recent_ts(1);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 20, 2, 0, 0.2, &older, "old"),
        overview_row(AgentId::Claude, "opus", 100, 10, 0, 1.0, &newer, "new"),
    ])
    .unwrap();

    fn claude_tokens(points: &[crate::models::UsageTrendPoint]) -> i64 {
        points
            .iter()
            .map(|p| p.0.get("claude").and_then(|v| v.as_i64()).unwrap_or(0))
            .sum()
    }

    fn point_dates(points: &[crate::models::UsageTrendPoint]) -> Vec<String> {
        points
            .iter()
            .filter_map(|p| p.0.get("date").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    }

    fn local_hour(ts: &str) -> String {
        chrono::DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:00")
            .to_string()
    }

    let rolling = repo
        .trend(1, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert_eq!(
        claude_tokens(&rolling),
        132,
        "days=1 without since is rolling 24h, so a 20h-ago row stays"
    );
    assert!(
        (20..=28).contains(&rolling.len()),
        "days=1 fills hourly buckets across the 24h window, got {}",
        rolling.len()
    );
    for ts in [&older, &newer] {
        assert!(
            point_dates(&rolling).contains(&local_hour(ts)),
            "trend hour is local %Y-%m-%d %H:00 of the row, not a UTC prefix"
        );
    }

    let since = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("resolvable local midnight")
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let today = repo
        .trend(1, Some(AgentId::Claude), None, Some(since.as_str()), &[])
        .unwrap();
    let expected_today = [(&older, 22_i64), (&newer, 110)]
        .into_iter()
        .filter(|(ts, _)| *ts >= &since)
        .map(|(_, tokens)| tokens)
        .sum::<i64>();
    assert_eq!(claude_tokens(&today), expected_today);
    for ts in [&older, &newer] {
        if ts.as_str() >= since.as_str() {
            assert!(point_dates(&today).contains(&local_hour(ts)));
        }
    }
}

#[test]
fn usage_trend_days7_fills_empty_local_days() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let ts = recent_ts(1);
    repo.insert_batch(&[overview_row(
        AgentId::Claude,
        "opus",
        10,
        1,
        0,
        0.1,
        &ts,
        "one-day",
    )])
    .unwrap();

    let points = repo
        .trend(7, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert!(
        (7..=9).contains(&points.len()),
        "7-day window fills empty local days so the categorical axis spans the range, got {}",
        points.len()
    );
    for p in &points {
        let date = p.0.get("date").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            date.len() == 10 && date.as_bytes().get(4) == Some(&b'-'),
            "days>1 stays YYYY-MM-DD, got {date}"
        );
    }
}

#[test]
fn usage_trend_includes_cache_tokens() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let ts = recent_ts(1);
    repo.insert_batch(&[overview_row(
        AgentId::Claude,
        "opus",
        100,
        20,
        30,
        1.0,
        &ts,
        "c-cache",
    )])
    .unwrap();

    let points = repo
        .trend(7, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    let tokens: i64 = points
        .iter()
        .map(|p| p.0.get("claude").and_then(|v| v.as_i64()).unwrap_or(0))
        .sum();
    assert_eq!(tokens, 150, "trend tokens match overview distribution");

    let overview = repo
        .overview(7, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert_eq!(overview.distribution[0].tokens, 150);
}

#[test]
fn usage_trend_by_model_includes_tokens_and_cost() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    let older = recent_ts(20);
    let newer = recent_ts(1);
    repo.insert_batch(&[
        overview_row(AgentId::Claude, "opus", 100, 10, 0, 1.0, &newer, "opus"),
        overview_row(AgentId::Claude, "sonnet", 50, 5, 0, 0.5, &newer, "sonnet"),
        overview_row(AgentId::Kimi, "opus", 20, 2, 0, 0.2, &older, "old-opus"),
    ])
    .unwrap();

    fn sum_i64(points: &[crate::models::UsageTrendPoint], key: &str) -> i64 {
        points
            .iter()
            .map(|p| p.0.get(key).and_then(|v| v.as_i64()).unwrap_or(0))
            .sum()
    }
    fn sum_f64(points: &[crate::models::UsageTrendPoint], key: &str) -> f64 {
        points
            .iter()
            .map(|p| p.0.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0))
            .sum()
    }

    let all = repo.trend_by_model(2, None, None, None, &[]).unwrap();
    assert_eq!(sum_i64(&all, "opus"), 132);
    assert_eq!(sum_i64(&all, "sonnet"), 55);
    assert_eq!(sum_i64(&all, "claude"), 0, "model grouping must not use agent ids");
    assert!((sum_f64(&all, "__cost__:opus") - 1.2).abs() < 1e-9);
    assert!((sum_f64(&all, "__cost__:sonnet") - 0.5).abs() < 1e-9);

    let claude_only = repo
        .trend_by_model(2, Some(AgentId::Claude), None, None, &[])
        .unwrap();
    assert_eq!(sum_i64(&claude_only, "opus"), 110);
    assert_eq!(sum_i64(&claude_only, "sonnet"), 55);

    let opus_only = repo
        .trend_by_model(2, None, Some("opus"), None, &[])
        .unwrap();
    assert_eq!(sum_i64(&opus_only, "opus"), 132);
    assert_eq!(sum_i64(&opus_only, "sonnet"), 0);
}

#[test]
fn since_filter_matches_offset_and_z_as_same_instant() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("usage.db")).unwrap();
    let repo = UsageRepo::new(db);
    repo.insert_batch(&[overview_row(
        AgentId::Claude,
        "opus",
        5,
        1,
        0,
        0.1,
        "2026-08-20T12:00:00+00:00",
        "offset-ts",
    )])
    .unwrap();

    let rows = repo
        .query(&crate::models::UsageQuery {
            days: 365,
            since: Some("2026-08-20T12:00:00.000Z".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
}
