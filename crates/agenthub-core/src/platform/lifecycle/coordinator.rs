//! LifecycleCoordinator — install-family operation template.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{AgentId, DetectStatus, InstallOutcome};
use crate::platform::detection::{builtin_detector_registry, DetectorRegistry};
use crate::platform::install::{builtin_install_registry, InstallContributionRegistry};
use crate::platform::AgentKey;
use crate::services::LiveWriteAuthority;
use crate::storage::{Database, OperationRepo};
use crate::utils::command_exec::CommandExecutor;
use crate::utils::paths::normalize_data_dir;
use crate::utils::redact::redact_text;

use super::executor::{BuiltinLifecycleInstallExecutor, LifecycleInstallExecutor};
use super::progress::{NullProgressSink, ProgressSink};
use super::types::{
    LifecycleError, LifecycleResult, OperationId, OperationKind, OperationStatus, OperationStep,
    ProgressEvent,
};

/// Process-local lock set for (agent_key) lifecycle ops — not distributed.
fn active_locks() -> &'static Mutex<HashSet<String>> {
    static LOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

struct AgentLifecycleLock {
    key: String,
}

impl AgentLifecycleLock {
    fn try_acquire(agent_key: &str) -> std::result::Result<Self, LifecycleError> {
        let key = agent_key.to_string();
        let mut g = active_locks().lock().map_err(|_| LifecycleError {
            code: "lifecycle.lock_poisoned",
            message: "lifecycle lock poisoned".into(),
        })?;
        if !g.insert(key.clone()) {
            return Err(LifecycleError::lock_held(&key));
        }
        Ok(Self { key })
    }
}

impl Drop for AgentLifecycleLock {
    fn drop(&mut self) {
        if let Ok(mut g) = active_locks().lock() {
            g.remove(&self.key);
        }
    }
}

/// Coordinates install/upgrade/uninstall/repair with operation records + redetect.
#[derive(Clone)]
pub struct LifecycleCoordinator {
    repo: OperationRepo,
    detectors: DetectorRegistry,
    installs: InstallContributionRegistry,
    executor: Arc<dyn LifecycleInstallExecutor>,
    /// The immutable data directory resolved by the owning AgentHub. Purge
    /// policy must never re-resolve this from process environment later.
    data_dir: Option<PathBuf>,
    /// A construction-time explicit data-dir failure. Keep the detail so a
    /// later purge reports why it is unavailable instead of silently falling
    /// back to an environment-derived guess.
    data_dir_error: Option<String>,
}

impl LifecycleCoordinator {
    // Referenced only from lifecycle `tests.rs`.
    #[allow(dead_code)]
    pub(crate) fn data_dir(&self) -> Option<&std::path::Path> {
        self.data_dir.as_deref()
    }

    #[allow(dead_code)]
    pub(crate) fn data_dir_error(&self) -> Option<&str> {
        self.data_dir_error.as_deref()
    }

    pub fn new(db: Database, registry: AdapterRegistry) -> Self {
        let executor = Arc::new(BuiltinLifecycleInstallExecutor::new(&db, registry));
        let data_dir = normalized_database_data_dir(&db);
        Self::with_registries_and_executor_and_optional_data_dir(
            db,
            builtin_detector_registry().clone(),
            builtin_install_registry().clone(),
            executor,
            data_dir,
            None,
        )
    }

    /// Composition-root constructor used by [`crate::AgentHub`].
    /// `data_dir` is the already-resolved path, including an explicit CLI
    /// override, and is retained immutably for the lifetime of this saga
    /// coordinator.
    pub fn new_with_data_dir(db: Database, registry: AdapterRegistry, data_dir: PathBuf) -> Self {
        let executor = Arc::new(BuiltinLifecycleInstallExecutor::new(&db, registry));
        Self::with_registries_and_data_dir(
            db,
            builtin_detector_registry().clone(),
            builtin_install_registry().clone(),
            executor,
            data_dir,
        )
    }

    /// Injectable composition root for tests and future key-native integrations.
    pub fn with_registries(
        db: Database,
        legacy_adapters: AdapterRegistry,
        detectors: DetectorRegistry,
        installs: InstallContributionRegistry,
    ) -> Self {
        let executor = Arc::new(BuiltinLifecycleInstallExecutor::new(&db, legacy_adapters));
        Self::with_registries_and_executor(db, detectors, installs, executor)
    }

    pub fn with_registries_and_executor(
        db: Database,
        detectors: DetectorRegistry,
        installs: InstallContributionRegistry,
        executor: Arc<dyn LifecycleInstallExecutor>,
    ) -> Self {
        let data_dir = normalized_database_data_dir(&db);
        Self::with_registries_and_executor_and_optional_data_dir(
            db, detectors, installs, executor, data_dir, None,
        )
    }

    /// Injectable composition root with an explicit immutable data directory.
    pub fn with_registries_and_data_dir(
        db: Database,
        detectors: DetectorRegistry,
        installs: InstallContributionRegistry,
        executor: Arc<dyn LifecycleInstallExecutor>,
        data_dir: PathBuf,
    ) -> Self {
        let (data_dir, data_dir_error) = match normalize_data_dir(&data_dir) {
            Ok(path) => (Some(path), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self::with_registries_and_executor_and_optional_data_dir(
            db,
            detectors,
            installs,
            executor,
            data_dir,
            data_dir_error,
        )
    }

    fn with_registries_and_executor_and_optional_data_dir(
        db: Database,
        detectors: DetectorRegistry,
        installs: InstallContributionRegistry,
        executor: Arc<dyn LifecycleInstallExecutor>,
        data_dir: Option<PathBuf>,
        data_dir_error: Option<String>,
    ) -> Self {
        Self {
            repo: OperationRepo::new(db),
            detectors,
            installs,
            executor,
            data_dir,
            data_dir_error,
        }
    }

    /// Mark leftover running operations after process restart (no auto-retry).
    pub fn interrupt_stale_running(db: &Database) -> Result<u64> {
        let repo = OperationRepo::new(db.clone());
        let n = repo.interrupt_all_running(&Utc::now().to_rfc3339())?;
        if n > 0 {
            logging::log_warn(
                targets::INSTALL,
                "lifecycle_interrupt",
                &format!("marked {n} running operations as interrupted after restart"),
            );
        }
        Ok(n)
    }

    pub fn list_operations(
        &self,
        agent: AgentId,
        limit: u32,
    ) -> Result<Vec<super::types::OperationRecord>> {
        let key = AgentKey::from_agent_id(agent);
        self.list_operations_key(&key, limit)
    }

    pub fn list_operations_key(
        &self,
        key: &AgentKey,
        limit: u32,
    ) -> Result<Vec<super::types::OperationRecord>> {
        self.repo.list_for_agent(key.as_str(), limit)
    }

    pub fn get_operation(&self, id: &str) -> Result<Option<super::types::OperationRecord>> {
        self.repo.get(id)
    }

    pub fn install_agent(
        &self,
        agent: AgentId,
        channel: &str,
        install_deps: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.install_agent_detailed(agent, channel, install_deps, executor, progress)
            .map(|r| r.outcome)
    }

    /// Install an agent and retain the coordinated result, including the
    /// key-native detector observation and operation id.
    pub fn install_agent_detailed(
        &self,
        agent: AgentId,
        channel: &str,
        install_deps: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.install_agent_key_detailed(&key, channel, install_deps, executor, progress)
    }

    pub fn install_agent_key(
        &self,
        key: &AgentKey,
        channel: &str,
        install_deps: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.install_agent_key_detailed(key, channel, install_deps, executor, progress)
            .map(|r| r.outcome)
    }

    /// Key-native install API. Unlike the compatibility façade above, this
    /// preserves `LifecycleResult::observed` for custom `AgentKey` values.
    pub fn install_agent_key_detailed(
        &self,
        key: &AgentKey,
        channel: &str,
        install_deps: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        let channel = channel.to_string();
        self.run(
            OperationKind::Install,
            key,
            progress,
            |lifecycle_executor, _detectors, installs, ex, sink, op_id| {
                sink_step(
                    sink,
                    op_id,
                    key,
                    OperationKind::Install,
                    OperationStep::Execute,
                    "running installer",
                );
                let contribution = installs
                    .get(key)
                    .ok_or_else(|| LifecycleError::unsupported(key, OperationKind::Install))?;
                lifecycle_executor.install(key, contribution.as_ref(), &channel, install_deps, ex)
            },
            executor,
        )
    }

    pub fn upgrade_agent(
        &self,
        agent: AgentId,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.upgrade_agent_detailed(agent, executor, progress)
            .map(|r| r.outcome)
    }

    /// Upgrade an agent and retain the coordinated result, including the
    /// key-native detector observation and operation id.
    pub fn upgrade_agent_detailed(
        &self,
        agent: AgentId,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.upgrade_agent_key_detailed(&key, executor, progress)
    }

    pub fn upgrade_agent_key(
        &self,
        key: &AgentKey,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.upgrade_agent_key_detailed(key, executor, progress)
            .map(|r| r.outcome)
    }

    /// Key-native upgrade API retaining detector observation and operation id.
    pub fn upgrade_agent_key_detailed(
        &self,
        key: &AgentKey,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        self.run(
            OperationKind::Upgrade,
            key,
            progress,
            |lifecycle_executor, _detectors, installs, ex, sink, op_id| {
                sink_step(
                    sink,
                    op_id,
                    key,
                    OperationKind::Upgrade,
                    OperationStep::Execute,
                    "running upgrade",
                );
                let contribution = installs
                    .get(key)
                    .ok_or_else(|| LifecycleError::unsupported(key, OperationKind::Upgrade))?;
                lifecycle_executor.upgrade(key, contribution.as_ref(), ex)
            },
            executor,
        )
    }

    pub fn uninstall_agent(
        &self,
        agent: AgentId,
        purge_config: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.uninstall_agent_detailed(agent, purge_config, executor, progress)
            .map(|r| r.outcome)
    }

    /// Uninstall an agent and retain the coordinated result, including the
    /// key-native detector observation and operation id.
    pub fn uninstall_agent_detailed(
        &self,
        agent: AgentId,
        purge_config: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.uninstall_agent_key_detailed(&key, purge_config, executor, progress)
    }

    pub fn uninstall_agent_key(
        &self,
        key: &AgentKey,
        purge_config: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<InstallOutcome> {
        self.uninstall_agent_key_detailed(key, purge_config, executor, progress)
            .map(|r| r.outcome)
    }

    /// Key-native uninstall API retaining detector observation and operation id.
    pub fn uninstall_agent_key_detailed(
        &self,
        key: &AgentKey,
        purge_config: bool,
        executor: &dyn CommandExecutor,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        self.run(
            OperationKind::Uninstall,
            key,
            progress,
            |lifecycle_executor, _detectors, installs, ex, sink, op_id| {
                sink_step(
                    sink,
                    op_id,
                    key,
                    OperationKind::Uninstall,
                    OperationStep::Execute,
                    "running uninstall",
                );
                let contribution = installs
                    .get(key)
                    .ok_or_else(|| LifecycleError::unsupported(key, OperationKind::Uninstall))?;
                let data_dir = if purge_config {
                    if let Some(error) = self.data_dir_error.as_deref() {
                        return Err(AppError::InvalidArg(format!(
                            "cannot purge config: invalid AgentHub data directory: {error}"
                        )));
                    }
                    self.data_dir.as_deref().ok_or_else(|| {
                        AppError::InvalidArg(
                            "cannot purge config: actual AgentHub data directory is unknown".into(),
                        )
                    })?
                } else {
                    // Builtin executors do not read this path when purge is
                    // disabled; retain compatibility for non-purge calls.
                    std::path::Path::new("")
                };
                lifecycle_executor.uninstall(key, contribution.as_ref(), purge_config, data_dir, ex)
            },
            executor,
        )
    }

    /// Repair: redetect only (no install scripts). Records observed state.
    pub fn repair_detect(
        &self,
        agent: AgentId,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.repair_detect_key(&key, progress)
    }

    /// Repair: redetect only (no install scripts). Records observed state.
    pub fn repair_detect_key(
        &self,
        key: &AgentKey,
        progress: Option<&mut dyn ProgressSink>,
    ) -> Result<LifecycleResult> {
        self.run(
            OperationKind::Repair,
            key,
            progress,
            |_lifecycle_executor, detectors, _installs, _ex, sink, op_id| {
                sink_step(
                    sink,
                    op_id,
                    key,
                    OperationKind::Repair,
                    OperationStep::Redetect,
                    "redetecting installation",
                );
                crate::services::agent_service::invalidate_detect_cache();
                let detect = detectors
                    .get(key)
                    .ok_or_else(|| LifecycleError::not_found(key.as_str()))?
                    .detect();
                let ok = detect.status == DetectStatus::Installed;
                Ok(InstallOutcome {
                    ok,
                    action: "agent_repair".into(),
                    logs: detect.notes.clone(),
                    message: if ok {
                        format!(
                            "{} detected installed ({})",
                            key.as_str(),
                            detect.version.clone().unwrap_or_else(|| "?".into())
                        )
                    } else {
                        format!("{} not found or broken", key.as_str())
                    },
                    agent: None,
                    runtime: None,
                    ..Default::default()
                })
            },
            &crate::utils::command_exec::SystemCommandExecutor,
        )
    }

    fn run(
        &self,
        kind: OperationKind,
        key: &AgentKey,
        progress: Option<&mut dyn ProgressSink>,
        execute: impl FnOnce(
            &dyn LifecycleInstallExecutor,
            &DetectorRegistry,
            &InstallContributionRegistry,
            &dyn CommandExecutor,
            &mut dyn ProgressSink,
            &OperationId,
        ) -> Result<InstallOutcome>,
        executor: &dyn CommandExecutor,
    ) -> Result<LifecycleResult> {
        // One operation id for this entire call — all progress events share it.
        let op_id = OperationId::new();
        let mut null = NullProgressSink;
        let sink: &mut dyn ProgressSink = match progress {
            Some(p) => p,
            None => &mut null,
        };

        // resolve
        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::Resolve,
            "resolving agent",
        );
        let detector = self
            .detectors
            .get(key)
            .ok_or_else(|| LifecycleError::not_found(key.as_str()))
            .map_err(|e| {
                let err = AppError::from(e.with_operation_id(&op_id));
                log_lifecycle_error("lifecycle_resolve", key, &op_id, kind, &err);
                err
            })?;
        if kind != OperationKind::Repair && !self.installs.contains_key(key) {
            let err =
                AppError::from(LifecycleError::unsupported(key, kind).with_operation_id(&op_id));
            log_lifecycle_error("lifecycle_unsupported", key, &op_id, kind, &err);
            return Err(err);
        }

        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::CapabilityCheck,
            "checking agent registration",
        );

        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::AcquireLock,
            "acquiring lock",
        );
        let _lock = AgentLifecycleLock::try_acquire(key.as_str()).map_err(|e| {
            let err = AppError::from(e.with_operation_id(&op_id));
            log_lifecycle_error("lifecycle_lock", key, &op_id, kind, &err);
            err
        })?;

        let started = Utc::now().to_rfc3339();
        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::CreateRecord,
            &format!("operation {}", op_id.as_str()),
        );
        self.repo
            .insert_running(
                op_id.as_str(),
                key.as_str(),
                kind,
                OperationStep::Preflight.as_str(),
                &started,
            )
            .map_err(|e| {
                let err = AppError::from(LifecycleError::audit_persist_failed(
                    &op_id,
                    OperationStep::CreateRecord.as_str(),
                    e,
                ));
                log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
                err
            })?;

        log_lifecycle_info(
            "lifecycle_start",
            key,
            &op_id,
            kind,
            None,
            "lifecycle operation started",
        );

        // preflight: adapter present (executors do env checks)
        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::Preflight,
            "preflight ok",
        );
        self.repo
            .update_step(op_id.as_str(), OperationStep::Plan.as_str())
            .map_err(|e| {
                let err = AppError::from(LifecycleError::audit_persist_failed(
                    &op_id,
                    OperationStep::Plan.as_str(),
                    e,
                ));
                log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
                err
            })?;
        sink_step(sink, &op_id, key, kind, OperationStep::Plan, "plan ready");
        self.repo
            .update_step(op_id.as_str(), OperationStep::Execute.as_str())
            .map_err(|e| {
                let err = AppError::from(LifecycleError::audit_persist_failed(
                    &op_id,
                    OperationStep::Execute.as_str(),
                    e,
                ));
                log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
                err
            })?;

        let exec_result = execute(
            self.executor.as_ref(),
            &self.detectors,
            &self.installs,
            executor,
            sink,
            &op_id,
        );

        // Always redetect for observed state (even if execute returned Err).
        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::Redetect,
            "redetecting",
        );
        let redetect_step_err = self
            .repo
            .update_step(op_id.as_str(), OperationStep::Redetect.as_str())
            .err();
        crate::services::agent_service::invalidate_detect_cache();
        let observed = Some(detector.detect());
        let observed_mapped = observed.clone();
        let obs_status = observed.as_ref().map(|d| match d.status {
            DetectStatus::Installed => "installed",
            DetectStatus::NotFound => "not_found",
        });
        let obs_ver = observed.as_ref().and_then(|d| d.version.clone());

        let exec_summary = match &exec_result {
            Ok(o) if o.ok => format!("execute succeeded: {}", redact_text(&o.message)),
            Ok(o) => format!(
                "execute completed with failure: {}",
                redact_text(&o.message)
            ),
            Err(e) => format!(
                "execute error ({}): {}",
                e.code(),
                redact_text(&e.to_string())
            ),
        };

        if let Some(e) = redetect_step_err {
            let err = AppError::from(LifecycleError::audit_partial_failure(
                &op_id,
                OperationStep::Redetect.as_str(),
                &exec_summary,
                obs_status,
                e,
            ));
            log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
            return Err(err);
        }

        let finished = Utc::now().to_rfc3339();
        let lifecycle = match exec_result {
            Ok(mut outcome) => {
                let status = if outcome.ok {
                    OperationStatus::Succeeded
                } else {
                    OperationStatus::Failed
                };
                let err_code = if outcome.ok {
                    None
                } else {
                    Some("install.failed")
                };
                let summary = redact_text(&outcome.message);
                self.repo
                    .finalize(
                        op_id.as_str(),
                        status,
                        Some(OperationStep::Finalize.as_str()),
                        err_code,
                        Some(&summary),
                        obs_status,
                        obs_ver.as_deref(),
                        &finished,
                    )
                    .map_err(|e| {
                        let err = AppError::from(LifecycleError::audit_partial_failure(
                            &op_id,
                            OperationStep::Finalize.as_str(),
                            &exec_summary,
                            obs_status,
                            e,
                        ));
                        log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
                        err
                    })?;
                // attach operation id to logs for audit (non-breaking)
                outcome
                    .logs
                    .push(format!("operation_id: {}", op_id.as_str()));
                LifecycleResult {
                    operation_id: op_id.clone(),
                    outcome,
                    observed: observed_mapped,
                }
            }
            Err(e) => {
                let summary = redact_text(&e.to_string());
                self.repo
                    .finalize(
                        op_id.as_str(),
                        OperationStatus::Failed,
                        Some(OperationStep::Finalize.as_str()),
                        Some(e.code()),
                        Some(&summary),
                        obs_status,
                        obs_ver.as_deref(),
                        &finished,
                    )
                    .map_err(|fe| {
                        let err = AppError::from(LifecycleError::audit_partial_failure(
                            &op_id,
                            OperationStep::Finalize.as_str(),
                            &exec_summary,
                            obs_status,
                            fe,
                        ));
                        // Prefer surfacing audit partial failure; still keep execute context
                        // in the audit message (exec_summary).
                        log_lifecycle_error("lifecycle_audit", key, &op_id, kind, &err);
                        err
                    })?;
                // Finalize succeeded: preserve original execute error (already audited in DB).
                log_lifecycle_error("lifecycle_execute", key, &op_id, kind, &e);
                return Err(e);
            }
        };

        let status = if lifecycle.outcome.ok {
            "succeeded"
        } else {
            "failed"
        };
        let msg = format!(
            "lifecycle {status}; observed={}; {}",
            obs_status.unwrap_or("unknown"),
            redact_text(&lifecycle.outcome.message)
        );
        if lifecycle.outcome.ok {
            log_lifecycle_info("lifecycle_finalize", key, &op_id, kind, Some(status), &msg);
        } else {
            // Execute body reported failure but coordination completed (DB finalized).
            let msg = redact_text(&msg);
            tracing::warn!(
                module = targets::INSTALL,
                code = "install.failed",
                op = "lifecycle_finalize",
                agent = key.as_str(),
                operation_id = op_id.as_str(),
                kind = kind.as_str(),
                status = status,
                observed = obs_status.unwrap_or("unknown"),
                "{msg}"
            );
        }

        sink_step(
            sink,
            &op_id,
            key,
            kind,
            OperationStep::Finalize,
            "operation finished",
        );
        Ok(lifecycle)
    }
}

fn sink_step(
    sink: &mut dyn ProgressSink,
    op_id: &OperationId,
    key: &AgentKey,
    kind: OperationKind,
    step: OperationStep,
    message: &str,
) {
    sink.on_progress(ProgressEvent {
        operation_id: op_id.as_str().to_string(),
        agent_key: key.as_str().to_string(),
        kind,
        step: step.as_str().to_string(),
        message: message.to_string(),
        percent: None,
    });
}

/// Resolve the data root from the same database authority used by the builtin
/// lifecycle executor. In-memory databases deliberately have no durable data
/// directory and therefore leave purge unavailable.
fn normalized_database_data_dir(db: &Database) -> Option<PathBuf> {
    let main_file = db
        .with_conn(|conn| {
            let path = conn.query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok(path)
        })
        .ok()?;
    let main_file = main_file.trim();
    if main_file.is_empty()
        || main_file == ":memory:"
        || main_file.starts_with("file:memdb")
        || main_file.starts_with("file::memory:")
    {
        return None;
    }

    let authority = LiveWriteAuthority::try_from_database(db).ok()?;
    normalize_data_dir(authority.data_root()).ok()
}

/// Structured ERROR for lifecycle coordination failures (`module=core.install`).
fn log_lifecycle_error(
    op: &str,
    key: &AgentKey,
    op_id: &OperationId,
    kind: OperationKind,
    err: &AppError,
) {
    let msg = redact_text(&err.to_string());
    tracing::error!(
        module = targets::INSTALL,
        code = err.code(),
        op = op,
        agent = key.as_str(),
        operation_id = op_id.as_str(),
        kind = kind.as_str(),
        "{msg}"
    );
}

/// Structured INFO milestone for lifecycle coordination (`module=core.install`).
fn log_lifecycle_info(
    op: &str,
    key: &AgentKey,
    op_id: &OperationId,
    kind: OperationKind,
    status: Option<&str>,
    msg: &str,
) {
    let msg = redact_text(msg);
    if let Some(status) = status {
        tracing::info!(
            module = targets::INSTALL,
            op = op,
            agent = key.as_str(),
            operation_id = op_id.as_str(),
            kind = kind.as_str(),
            status = status,
            "{msg}"
        );
    } else {
        tracing::info!(
            module = targets::INSTALL,
            op = op,
            agent = key.as_str(),
            operation_id = op_id.as_str(),
            kind = kind.as_str(),
            "{msg}"
        );
    }
}
