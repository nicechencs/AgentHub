//! Provider pool service — CRUD, import-live, and safe live switching.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use uuid::Uuid;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentConfig, AgentId, BackupKind, Provider, ProviderInput, ProviderSwitchResult,
};
use crate::services::{BackupService, ConnectionService};
use crate::storage::{Database, ProviderRepo};
use crate::utils::redact::redact_text;

/// Maximum Unicode scalar values allowed in a provider id.
pub const MAX_PROVIDER_ID_LEN: usize = 128;
/// Maximum Unicode scalar values allowed in a provider name.
pub const MAX_PROVIDER_NAME_LEN: usize = 256;

/// Business facade over [`ProviderRepo`].
pub struct ProviderService {
    repo: ProviderRepo,
    registry: AdapterRegistry,
    backup: Option<BackupService>,
    lock_dir: Option<PathBuf>,
    connections: ConnectionService,
}

impl ProviderService {
    /// Construct the provider-pool service without live-write orchestration.
    /// CRUD and import-live are available; [`Self::switch`] fails closed until
    /// a backup root is configured through [`Self::with_live`].
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, AdapterRegistry::default())
    }

    /// Inject adapters for tests or callers that only need CRUD/import-live.
    pub fn with_registry(db: Database, registry: AdapterRegistry) -> Self {
        Self {
            repo: ProviderRepo::new(db.clone()),
            registry,
            backup: None,
            lock_dir: None,
            connections: ConnectionService::new(db),
        }
    }

    /// Construct the full live-switch service with explicit shared
    /// dependencies and backup location.
    pub fn with_live(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        let lock_dir = backups_root.parent().unwrap_or(&backups_root).join("locks");
        Self {
            repo: ProviderRepo::new(db.clone()),
            backup: Some(BackupService::new(
                db.clone(),
                registry.clone(),
                backups_root,
            )),
            registry,
            lock_dir: Some(lock_dir),
            connections: ConnectionService::new(db),
        }
    }

    /// Deterministic list: [`AgentId::ALL`] order, then name, then id.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Provider>> {
        let mut items = self.repo.list(agent)?;
        sort_providers(&mut items);
        Ok(items)
    }

    /// Resolve by primary key id first; otherwise by exact name.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - Multiple name matches → [`AppError::InvalidArg`] (ambiguous)
    /// - Optional `agent` scopes both id and name lookup
    pub fn get(&self, id_or_name: &str, agent: Option<AgentId>) -> Result<Provider> {
        let key = id_or_name.trim();
        if key.is_empty() {
            return Err(AppError::InvalidArg(
                "provider id or name must not be empty".into(),
            ));
        }

        if let Some(p) = self.repo.get_by_id(key)? {
            if let Some(agent) = agent {
                if p.agent_id != agent {
                    return Err(AppError::NotFound(format!(
                        "provider not found: {key} (agent filter: {})",
                        agent.as_str()
                    )));
                }
            }
            return Ok(p);
        }

        let matches = self.repo.list_by_name(key, agent)?;
        match matches.len() {
            0 => Err(AppError::NotFound(format!("provider not found: {key}"))),
            1 => Ok(matches.into_iter().next().expect("len 1")),
            n => Err(AppError::InvalidArg(format!(
                "ambiguous provider name '{key}': found {n} providers; specify --agent or use id"
            ))),
        }
    }

    /// Create a new provider. Core owns timestamps.
    ///
    /// Duplicate id → [`AppError::InvalidArg`].
    pub fn create(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = self.create_inner(input);
        log_provider_op("create", agent, started, &result);
        result
    }

    fn create_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let now = now_ts();
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_provider(&row)?;
            Ok(created)
        } else {
            self.repo.create(&row)
        }
    }

    /// Update an existing provider by id. Core owns `updated_at`; preserves `created_at`.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - `agent_id` change → [`AppError::InvalidArg`]
    pub fn update(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = self.update_inner(input);
        log_provider_op("update", agent, started, &result);
        result
    }

    fn update_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            // Repo preserves the stored created_at; placeholder only.
            created_at: String::new(),
            updated_at: now_ts(),
        };
        if row.is_current {
            let (updated, _binding) = self.connections.update_and_activate_provider(&row)?;
            Ok(updated)
        } else {
            // Demote path: update + clear binding when this row was active.
            self.connections.update_provider_non_current(&row)
        }
    }

    /// Insert or update. On existing rows: preserve `created_at`, reject `agent_id` change.
    pub fn upsert(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = self.upsert_inner(input);
        log_provider_op("upsert", agent, started, &result);
        result
    }

    fn upsert_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let now = now_ts();
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            // Used only for the insert path; update path preserves stored created_at.
            created_at: now.clone(),
            updated_at: now,
        };
        if row.is_current {
            let (upserted, _binding) = self.connections.upsert_and_activate_provider(&row)?;
            Ok(upserted)
        } else {
            // Demote / plain upsert: clear binding only if it references this id.
            self.connections.upsert_provider_non_current(&row)
        }
    }

    /// Delete by primary key id.
    ///
    /// - Empty / invalid id → [`AppError::InvalidArg`]
    /// - Missing → [`AppError::NotFound`]
    pub fn delete(&self, id: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            validate_id(id)?;
            // Clear active binding in the same transaction when deleting the active row.
            self.connections.delete_provider(id, agent)
        })();
        log_provider_op("delete", agent, started, &result);
        result
    }

    /// Capture the agent's complete live provider config as a new current row.
    ///
    /// Secrets are preserved in the L1 provider pool; callers must use
    /// [`Provider::redacted`] before displaying/serializing the result.
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Provider> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
        log_provider_op("import", agent, started, &result);
        result
    }

    fn import_live_inner(&self, agent: AgentId, name: Option<&str>) -> Result<Provider> {
        let _lock = self.acquire_live_lock(agent)?;
        let adapter = self.adapter(agent)?;
        let live = adapter.read_config()?;
        ensure_config_agent(&live, agent)?;
        if live_config_is_empty(&live.raw) {
            return Err(AppError::NotFound(format!(
                "no live provider config found for agent {}",
                agent.as_str()
            )));
        }

        let display_name = name
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Imported {}", now_ts()));
        validate_name(&display_name)?;
        let input = ProviderInput {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            name: display_name,
            settings_config: live.raw,
            meta: serde_json::json!({ "source": "live" }),
            is_current: true,
        };
        // Use inner create so import is a single log op (not create + import).
        self.create_inner(&input)
    }

    /// Apply a saved provider to the live agent config.
    ///
    /// Order is fixed: validate/lock -> read live -> backfill old current ->
    /// snapshot -> atomic adapter write -> select target in the DB. Backup,
    /// apply, and final DB failures compensate earlier DB/live changes. A
    /// rollback-specific error reports compensation failure using error codes
    /// only, so adapter messages cannot expose provider secrets.
    pub fn switch(&self, id_or_name: &str, agent: AgentId) -> Result<ProviderSwitchResult> {
        let started = Instant::now();
        let result = self.switch_inner(id_or_name, agent);
        log_provider_op("switch", agent, started, &result);
        result
    }

    fn switch_inner(&self, id_or_name: &str, agent: AgentId) -> Result<ProviderSwitchResult> {
        let backup = self.backup.as_ref().ok_or_else(|| {
            AppError::Unsupported(
                "provider live switching requires an explicitly configured backup root".into(),
            )
        })?;
        let _lock = self.acquire_live_lock(agent)?.ok_or_else(|| {
            AppError::Unsupported("provider live switching is not configured".into())
        })?;

        let target = self.get(id_or_name, Some(agent))?;
        let adapter = self.adapter(agent)?;
        let live_before = adapter.read_config()?;
        ensure_config_agent(&live_before, agent)?;
        let current = self.repo.get_current(agent)?;

        let live_for_backfill =
            (!live_config_is_empty(&live_before.raw)).then_some(live_before.raw.clone());
        let backfilled_provider_id = current
            .as_ref()
            .filter(|_| live_for_backfill.is_some())
            .map(|provider| provider.id.clone());

        // If the selected row is already current, its backfilled live value is
        // authoritative and must not immediately be overwritten by stale L1.
        let target_raw = match (&current, &live_for_backfill) {
            (Some(current), Some(raw)) if current.id == target.id => raw.clone(),
            _ => target.settings_config.clone(),
        };
        let target_config = AgentConfig {
            agent,
            raw: target_raw,
        };

        // Persist the complete live value first. Later stages compensate this
        // row on failure, giving a deterministic write sequence:
        // backfill -> backup -> live apply -> final DB selection.
        let backfilled = match (&current, &live_for_backfill) {
            (Some(current), Some(raw)) => {
                Some(self.repo.backfill_current(current, raw, &now_ts())?)
            }
            _ => None,
        };
        let rollback_backfill = || match (&current, &backfilled) {
            (Some(original), Some(applied)) => self
                .repo
                .restore_backfill(original, &applied.updated_at)
                .err(),
            _ => None,
        };
        let expected_target_updated_at = backfilled
            .as_ref()
            .filter(|row| row.id == target.id)
            .map_or(target.updated_at.as_str(), |row| row.updated_at.as_str());

        let snapshot = match backup.snapshot(
            agent,
            BackupKind::AutoSwitch,
            Some(&format!("before provider switch to {}", target.id)),
        ) {
            Ok(record) => Some(record),
            Err(error) if error.code() == "not_found" => None,
            Err(error) => {
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, None, db_rollback));
            }
        };

        if let Err(error) = adapter.write_config(&target_config) {
            let live_rollback = adapter.write_config(&live_before).err();
            let db_rollback = rollback_backfill();
            return Err(compensated_switch_error(error, live_rollback, db_rollback));
        }
        let now = now_ts();
        // Single transaction: is_current + demote accounts + binding (B1 cleanup).
        let provider = match self.connections.activate_provider(
            agent,
            &target.id,
            expected_target_updated_at,
            &now,
        ) {
            Ok((provider, _binding)) => provider,
            Err(error) => {
                let live_rollback = adapter.write_config(&live_before).err();
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, live_rollback, db_rollback));
            }
        };

        Ok(ProviderSwitchResult {
            provider,
            backup: snapshot,
            backfilled_provider_id,
        })
    }

    /// Storage access for tests / future write paths (not used by list/show CLI).
    pub fn repo(&self) -> &ProviderRepo {
        &self.repo
    }

    fn adapter(&self, agent: AgentId) -> Result<std::sync::Arc<dyn crate::adapters::AgentAdapter>> {
        self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })
    }

    fn acquire_live_lock(&self, agent: AgentId) -> Result<Option<ProviderSwitchLock>> {
        self.lock_dir
            .as_deref()
            .map(|lock_dir| ProviderSwitchLock::acquire(lock_dir, agent))
            .transpose()
    }
}

fn log_provider_op<T>(op: &str, agent: AgentId, started: Instant, result: &Result<T>) {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(_) => {
            let msg = match op {
                "switch" => "switched provider",
                "delete" => "deleted provider",
                "create" => "created provider",
                "update" => "updated provider",
                "upsert" => "upserted provider",
                "import" => "imported provider",
                _ => "ok",
            };
            tracing::info!(
                module = targets::PROVIDER,
                op,
                agent = agent.as_str(),
                elapsed_ms,
                "{msg}"
            );
        }
        Err(err) => {
            let msg = redact_text(&err.to_string());
            tracing::error!(
                module = targets::PROVIDER,
                op,
                agent = agent.as_str(),
                code = err.code(),
                elapsed_ms,
                "{msg}"
            );
        }
    }
}

fn compensated_switch_error(
    primary: AppError,
    live_rollback: Option<AppError>,
    db_rollback: Option<AppError>,
) -> AppError {
    if live_rollback.is_none() && db_rollback.is_none() {
        return primary;
    }
    let live = live_rollback.as_ref().map_or("ok", AppError::code);
    let database = db_rollback.as_ref().map_or("ok", AppError::code);
    AppError::message(
        "provider.switch.rollback",
        format!(
            "provider switch failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}

fn now_ts() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

fn ensure_config_agent(config: &AgentConfig, expected: AgentId) -> Result<()> {
    if config.agent != expected {
        return Err(AppError::InvalidArg(format!(
            "adapter returned config for {}, expected {}",
            config.agent.as_str(),
            expected.as_str()
        )));
    }
    require_json_object(&config.raw, "live settings_config")
}

fn live_config_is_empty(raw: &serde_json::Value) -> bool {
    let Some(object) = raw.as_object() else {
        return false;
    };
    object.is_empty()
        || (object.get("format").and_then(|value| value.as_str()) == Some("toml")
            && object
                .get("content")
                .and_then(|value| value.as_str())
                .is_some_and(str::is_empty))
}

/// Conservative upper bound for a live provider switch/import.
/// Locks older than this are treated as abandoned even if the PID still
/// appears alive (PID reuse / hung process safety net).
const PROVIDER_LOCK_TTL: Duration = Duration::from_secs(30 * 60);

/// How many create/reclaim attempts after observing an existing lock file.
const PROVIDER_LOCK_ACQUIRE_ATTEMPTS: usize = 3;

/// Per-agent exclusive live-switch lock with owner metadata and stale recovery.
///
/// Lock file format (line-oriented, diagnostic-friendly):
/// ```text
/// pid=<os pid>
/// created_unix_ms=<epoch millis>
/// token=<uuid>
/// ```
#[derive(Debug)]
struct ProviderSwitchLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    /// Identity of this holder; Drop only unlinks when the file still carries it.
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockOwner {
    pid: u32,
    created_unix_ms: u64,
    token: String,
}

impl LockOwner {
    fn current() -> Self {
        Self {
            pid: std::process::id(),
            created_unix_ms: unix_now_ms(),
            token: Uuid::new_v4().to_string(),
        }
    }

    fn serialize(&self) -> String {
        format!(
            "pid={}\ncreated_unix_ms={}\ntoken={}\n",
            self.pid, self.created_unix_ms, self.token
        )
    }

    /// Parse owner metadata. Unknown keys are ignored for forward compatibility;
    /// missing/invalid required fields fail closed (not reclaimable).
    fn parse(raw: &str) -> Option<Self> {
        let mut pid = None;
        let mut created_unix_ms = None;
        let mut token = None;

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "pid" => {
                    pid = Some(value.parse::<u32>().ok()?);
                }
                "created_unix_ms" => {
                    created_unix_ms = Some(value.parse::<u64>().ok()?);
                }
                "token" => {
                    if value.is_empty() {
                        return None;
                    }
                    token = Some(value.to_string());
                }
                _ => {}
            }
        }

        Some(Self {
            pid: pid?,
            created_unix_ms: created_unix_ms?,
            token: token?,
        })
    }

    fn is_stale(&self) -> bool {
        if lock_age_ms(self.created_unix_ms) >= PROVIDER_LOCK_TTL.as_millis() as u64 {
            return true;
        }
        !process_is_alive(self.pid)
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.pid == other.pid
            && self.created_unix_ms == other.created_unix_ms
            && self.token == other.token
    }
}

impl ProviderSwitchLock {
    fn acquire(lock_dir: &Path, agent: AgentId) -> Result<Self> {
        std::fs::create_dir_all(lock_dir)?;
        let path = lock_dir.join(format!("provider-{}.lock", agent.as_str()));

        for _ in 0..PROVIDER_LOCK_ACQUIRE_ATTEMPTS {
            match Self::try_create(&path) {
                Ok(lock) => return Ok(lock),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !try_reclaim_stale_lock(&path)? {
                        return Err(lock_held_error(agent));
                    }
                    // Stale lock removed (or raced away); retry exclusive create.
                }
                Err(error) => return Err(error.into()),
            }
        }

        Err(lock_held_error(agent))
    }

    fn try_create(path: &Path) -> std::io::Result<Self> {
        let owner = LockOwner::current();
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        if let Err(error) = file.write_all(owner.serialize().as_bytes()) {
            // Close the handle first (Windows cannot unlink an open file), then
            // best-effort remove the exclusive-created path so a write failure
            // does not leave a permanent empty/partial lock behind.
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        // Best-effort durability so a crash mid-write is less likely to leave
        // an empty/partial owner record that another process must interpret.
        let _ = file.sync_all();
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            token: owner.token,
        })
    }
}

impl Drop for ProviderSwitchLock {
    fn drop(&mut self) {
        // Windows refuses to unlink an open file; close the handle first.
        drop(self.file.take());
        // Never delete a lock we no longer own (reclaimed / replaced).
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => {
                if LockOwner::parse(&raw).is_some_and(|owner| owner.token == self.token) {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
            Err(_) => {
                // Missing or unreadable: nothing safe to do.
            }
        }
    }
}

fn lock_held_error(agent: AgentId) -> AppError {
    AppError::message(
        "provider.lock",
        format!(
            "another provider switch is already running for agent {}",
            agent.as_str()
        ),
    )
}

/// Attempt to remove a lock file only when it is both stale and unchanged
/// between diagnosis and unlink (read-after-verify + identity token).
///
/// Returns `true` when the path is clear for a new exclusive create
/// (removed by us, already gone, or raced away). Returns `false` when the
/// lock is active or malformed (fail-closed: do not reclaim).
fn try_reclaim_stale_lock(path: &Path) -> Result<bool> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };

    let owner = match LockOwner::parse(&raw) {
        Some(owner) => owner,
        // Malformed / incomplete metadata: fail closed, never unlink.
        None => return Ok(false),
    };

    if !owner.is_stale() {
        return Ok(false);
    }

    // Re-read and require identical owner identity before unlinking so we do
    // not delete a lock that was just replaced or refreshed by another process.
    let raw_again = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    if raw_again != raw {
        return Ok(false);
    }
    let owner_again = match LockOwner::parse(&raw_again) {
        Some(owner) => owner,
        None => return Ok(false),
    };
    if !owner.same_identity(&owner_again) {
        return Ok(false);
    }

    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        // Someone else may hold the file open (Windows) or replaced it mid-flight.
        Err(_) => Ok(false),
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_age_ms(created_unix_ms: u64) -> u64 {
    unix_now_ms().saturating_sub(created_unix_ms)
}

/// Best-effort liveness probe. Prefer false negatives (assume alive) when the
/// OS check is inconclusive so reclaim falls back to TTL only.
fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(windows)]
    {
        windows_process_is_alive(pid)
    }

    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::process::{Command, Stdio};
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            // Unknown → treat as alive (do not reclaim on PID alone).
            .unwrap_or(true)
    }

    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        // No process probe: only TTL can mark the lock stale.
        true
    }
}

#[cfg(windows)]
fn windows_process_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, exit_code: *mut u32) -> i32;
        fn GetLastError() -> u32;
    }

    // PROCESS_QUERY_LIMITED_INFORMATION — enough for exit code, works across sessions.
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ACCESS_DENIED: u32 = 5;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // Access denied ⇒ process exists but is protected; treat as alive.
            // Any other open failure (invalid/exited pid) ⇒ not alive.
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut exit_code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == STILL_ACTIVE
    }
}

fn validate_provider_input(input: &ProviderInput) -> Result<()> {
    validate_id(&input.id)?;
    validate_name(&input.name)?;
    require_json_object(&input.settings_config, "settings_config")?;
    require_json_object(&input.meta, "meta")?;
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    validate_label(id, "provider id", MAX_PROVIDER_ID_LEN)
}

fn validate_name(name: &str) -> Result<()> {
    validate_label(name, "provider name", MAX_PROVIDER_NAME_LEN)
}

fn validate_label(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.is_empty() {
        return Err(AppError::InvalidArg(format!("{field} must not be empty")));
    }
    if value != value.trim() {
        return Err(AppError::InvalidArg(format!(
            "{field} must not have surrounding whitespace"
        )));
    }
    if value.chars().count() > max_chars {
        return Err(AppError::InvalidArg(format!(
            "{field} exceeds maximum length of {max_chars} characters"
        )));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidArg(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn require_json_object(value: &serde_json::Value, field: &str) -> Result<()> {
    if !value.is_object() {
        return Err(AppError::InvalidArg(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(())
}

fn agent_rank(id: AgentId) -> usize {
    AgentId::ALL
        .iter()
        .position(|a| *a == id)
        .unwrap_or(usize::MAX)
}

fn sort_providers(items: &mut [Provider]) {
    items.sort_by(|a, b| {
        agent_rank(a.agent_id)
            .cmp(&agent_rank(b.agent_id))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests;
