//! P13 / R07 / P2-1 open/closed validation via test-only `demo-agent`.
//!
//! Contributions live in `integrations/agents/demo_agent/`. This file only
//! drives the real catalog / lifecycle / configuration services.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;

use crate::error::{AppError, Result};
use crate::integrations::agents::demo_agent;
use crate::integrations::ProductionIntegrations;
use crate::models::{Capability, CapabilityLevel, DetectStatus, InstallOutcome};
use crate::platform::agent_catalog::{AgentCatalogService, AgentDescriptor, AgentKey};
use crate::platform::config::{
    builtin_config_registry, ConfigProjectorRegistry, ConfigurationService,
};
use crate::platform::detection::{builtin_detector_registry, DetectorRegistry};
use crate::platform::install::{
    builtin_install_registry, InstallContribution, InstallContributionRegistry,
};
use crate::platform::lifecycle::{
    LifecycleCoordinator, LifecycleInstallExecutor, OperationKind, OperationStatus, VecProgressSink,
};
use crate::storage::Database;
use crate::utils::command_exec::{CommandExecutor, ExecRequest, ExecResult};

const DEMO_KEY: &str = demo_agent::KEY;

fn demo_key() -> AgentKey {
    demo_agent::key()
}

fn test_configuration_database() -> Database {
    let dir = tempdir().expect("configuration authority tempdir");
    Database::open(&dir.path().join("configuration-authority.db")).expect("configuration db")
}

fn register_demo(installed: Arc<AtomicBool>) -> ProductionIntegrations {
    let mut bundle = ProductionIntegrations::empty();
    demo_agent::register(&mut bundle.as_context(), installed);
    bundle
}

// ── Test-only contributions (never registered in production) ─────────────────

/// Lifecycle install executor that flips the shared detector flag (no real install).
struct FakeLifecycleInstallExecutor {
    installed: Arc<AtomicBool>,
}

impl LifecycleInstallExecutor for FakeLifecycleInstallExecutor {
    fn install(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        channel: &str,
        _install_deps: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        // Touch the command executor so the platform path is exercised without
        // spawning a real process (NoopCommandExecutor returns immediately).
        let _ = command_executor.run(&ExecRequest {
            program: "demo-agent-fake-install".into(),
            args: vec![channel.to_string()],
            timeout: Duration::from_secs(1),
            max_output_bytes: 1024,
        });
        self.installed.store(true, Ordering::SeqCst);
        Ok(InstallOutcome {
            ok: true,
            action: "agent_install".into(),
            logs: vec![format!("demo install via {channel}")],
            message: format!("{} installed", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }

    fn upgrade(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        self.install(key, contribution, "npm", false, command_executor)
    }

    fn uninstall(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        _purge_config: bool,
        _actual_data_dir: &std::path::Path,
        _command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        self.installed.store(false, Ordering::SeqCst);
        Ok(InstallOutcome {
            ok: true,
            action: "agent_uninstall".into(),
            logs: vec!["demo uninstall".into()],
            message: format!("{} uninstalled", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }
}

/// No-side-effect command executor — never spawns or networks.
struct NoopCommandExecutor;

impl CommandExecutor for NoopCommandExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        ExecResult {
            command: req.program.clone(),
            exit_code: Some(0),
            timed_out: false,
            stdout: "demo noop".into(),
            stderr: String::new(),
            spawn_error: None,
        }
    }
}

// ── Descriptor / harness helpers ─────────────────────────────────────────────

fn demo_descriptor() -> AgentDescriptor {
    demo_agent::descriptor()
}

fn level_is_executable(level: CapabilityLevel) -> bool {
    matches!(level, CapabilityLevel::Full | CapabilityLevel::Partial)
}

fn level_is_non_executable(level: CapabilityLevel) -> bool {
    matches!(
        level,
        CapabilityLevel::Unsupported | CapabilityLevel::Planned
    )
}

/// Injectable platform stack for demo-agent — real services, fake install boundary.
fn open_demo_platform() -> (
    tempfile::TempDir,
    AgentKey,
    AgentCatalogService,
    DetectorRegistry,
    InstallContributionRegistry,
    ConfigProjectorRegistry,
    ConfigurationService,
    LifecycleCoordinator,
    Arc<AtomicBool>,
) {
    let dir = tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("demo-agent.db")).expect("db");
    let key = demo_key();
    let installed = Arc::new(AtomicBool::new(false));

    let catalog =
        AgentCatalogService::new(vec![demo_descriptor()]).expect("demo catalog injection");

    let bundle = register_demo(Arc::clone(&installed));
    let detectors = bundle.detectors.clone();
    let installs = bundle.install.clone();
    let config_reg = bundle.config.clone();
    let configuration = ConfigurationService::with_registry(db.clone(), config_reg.clone());

    let lifecycle = LifecycleCoordinator::with_registries_and_executor(
        db,
        detectors.clone(),
        installs.clone(),
        Arc::new(FakeLifecycleInstallExecutor {
            installed: Arc::clone(&installed),
        }),
    );

    (
        dir,
        key,
        catalog,
        detectors,
        installs,
        config_reg,
        configuration,
        lifecycle,
        installed,
    )
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn demo_agent_catalog_is_queryable_via_injection() {
    let catalog = AgentCatalogService::new(vec![demo_descriptor()]).expect("catalog");
    assert_eq!(catalog.len(), 1);
    let got = catalog.get_str(DEMO_KEY).expect("query by str");
    assert_eq!(got.display_name, "Demo Agent");
    assert_eq!(got.key.as_str(), DEMO_KEY);
    assert_eq!(got.config_schema_version, Some(1));
    assert_eq!(
        got.capabilities.get("configWrite").unwrap().level,
        CapabilityLevel::Full
    );
}

#[test]
fn demo_agent_registries_queryable_and_reject_duplicate_keys() {
    let key = demo_key();
    let mut bundle = register_demo(Arc::new(AtomicBool::new(false)));

    assert!(bundle.detectors.contains_key(&key));
    assert!(bundle.detectors.get(&key).is_some());
    assert!(bundle.install.contains_key(&key));
    assert_eq!(
        bundle.install.get(&key).unwrap().npm_package(),
        Some("@agenthub/demo-agent")
    );
    assert!(bundle.config.contains_key(&key));
    assert!(bundle.config.get(&key).is_some());

    let dup_installed = Arc::new(AtomicBool::new(false));
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        demo_agent::register(&mut bundle.as_context(), dup_installed);
    }));
    assert!(err.is_err(), "second register must reject duplicate keys");
}

#[test]
fn demo_agent_install_and_repair_via_real_lifecycle_path() {
    let (_dir, key, _catalog, _detectors, _installs, _config_reg, _cfg, lifecycle, installed) =
        open_demo_platform();

    let mut sink = VecProgressSink::default();
    let outcome = lifecycle
        .install_agent_key(&key, "npm", false, &NoopCommandExecutor, Some(&mut sink))
        .expect("install_agent_key through LifecycleCoordinator");

    assert!(outcome.ok, "install should succeed: {}", outcome.message);
    assert!(installed.load(Ordering::SeqCst));

    // Progress: non-empty operation_id, consistent across every event.
    assert!(!sink.events.is_empty(), "expected progress events");
    let op_id = sink.events[0].operation_id.clone();
    assert!(!op_id.is_empty(), "operation_id must be non-empty");
    for ev in &sink.events {
        assert_eq!(
            ev.operation_id, op_id,
            "all progress events must share one operation_id"
        );
        assert_eq!(ev.agent_key, DEMO_KEY);
    }
    assert!(
        outcome
            .logs
            .iter()
            .any(|l| l.contains(&format!("operation_id: {op_id}"))),
        "InstallOutcome logs should carry the same operation_id"
    );

    // Operation row: demo key + observed installed.
    let ops = lifecycle.list_operations_key(&key, 10).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].id, op_id);
    assert_eq!(ops[0].agent_key, DEMO_KEY);
    assert_eq!(ops[0].kind, OperationKind::Install);
    assert_eq!(ops[0].status, OperationStatus::Succeeded);
    assert_eq!(ops[0].observed_status.as_deref(), Some("installed"));

    // Repair redetect via real coordinator path — installed observed.
    let repaired = lifecycle
        .repair_detect_key(&key, None)
        .expect("repair_detect_key");
    assert_eq!(repaired.outcome.action, "agent_repair");
    assert_eq!(
        repaired.observed.as_ref().map(|o| o.status),
        Some(DetectStatus::Installed)
    );
    let repair_ops = lifecycle.list_operations_key(&key, 10).unwrap();
    assert!(repair_ops.iter().any(|op| {
        op.kind == OperationKind::Repair && op.observed_status.as_deref() == Some("installed")
    }));
}

#[test]
fn demo_agent_config_schema_and_validate_via_configuration_service() {
    let key = demo_key();
    let bundle = register_demo(Arc::new(AtomicBool::new(false)));
    let svc = ConfigurationService::with_registry(test_configuration_database(), bundle.config);

    let schema = svc.schema_for_agent_key(&key).expect("schema via service");
    assert_eq!(schema.agent_key.as_str(), DEMO_KEY);
    assert!(schema.fields.iter().any(|f| f.key == "greeting"));

    let validation = svc
        .validate_for_agent_key(
            &key,
            &BTreeMap::from([("greeting".into(), serde_json::json!("hi"))]),
        )
        .expect("validate via service");
    assert!(validation.ok);
}

#[test]
fn unregistered_optional_projector_returns_unsupported() {
    // Empty registry: demo-agent has no projector — optional contribution absent.
    let svc = ConfigurationService::with_registry(
        test_configuration_database(),
        ConfigProjectorRegistry::new(),
    );
    let key = demo_key();
    let err = svc
        .schema_for_agent_key(&key)
        .expect_err("must be unsupported");
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn demo_agent_descriptor_json_round_trip() {
    let d = demo_descriptor();
    let json = serde_json::to_string(&d).expect("serialize");
    let back: AgentDescriptor = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.key.as_str(), DEMO_KEY);
    assert_eq!(back.display_name, d.display_name);
    assert_eq!(back.integration_version, d.integration_version);
    assert_eq!(back.config_schema_version, d.config_schema_version);
    assert_eq!(back.capabilities, d.capabilities);
    assert_eq!(back.install_channels, d.install_channels);
}

#[test]
fn demo_agent_capability_surface_only_config_write_is_executable() {
    let d = demo_descriptor();
    for cap in Capability::ALL {
        let state = d
            .capabilities
            .get(cap.as_str())
            .unwrap_or_else(|| panic!("missing capability cell {}", cap.as_str()));
        match cap {
            Capability::ConfigWrite => {
                assert!(
                    level_is_executable(state.level),
                    "ConfigWrite (has projector handler) must be Full|Partial, got {:?}",
                    state.level
                );
            }
            _ => {
                assert!(
                    level_is_non_executable(state.level),
                    "{} must be Planned|Unsupported, got {:?}",
                    cap.as_str(),
                    state.level
                );
                assert!(
                    !level_is_executable(state.level),
                    "{} must be judged non-executable",
                    cap.as_str()
                );
            }
        }
    }
}

#[test]
fn production_registries_exclude_demo_agent() {
    let key = demo_key();

    let catalog = AgentCatalogService::builtin().expect("builtin catalog");
    assert!(
        catalog.get_str(DEMO_KEY).is_err(),
        "demo-agent must not appear in production catalog"
    );
    for d in catalog.list() {
        assert_ne!(d.key.as_str(), DEMO_KEY);
    }

    let detectors = builtin_detector_registry();
    assert!(
        !detectors.contains_key(&key),
        "production detector registry must not contain demo-agent"
    );

    let installs = builtin_install_registry();
    assert!(
        !installs.contains_key(&key),
        "production install registry must not contain demo-agent"
    );

    let configs = builtin_config_registry();
    assert!(
        !configs.contains_key(&key),
        "production config registry must not contain demo-agent"
    );
}
