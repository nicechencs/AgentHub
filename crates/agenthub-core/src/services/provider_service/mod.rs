//! Provider pool service — CRUD, import-live, and safe live switching.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::ProviderService`].

mod compensate;
mod live;
mod lock;
mod pool;
mod switch_saga;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::targets;
#[allow(unused_imports)]
use crate::models::BackupKind;
use crate::models::{AdapterBindingHealNotice, AgentConfig, AgentId, Provider, ProviderInput};
use crate::services::{
    AdapterSecretResolver, BackupService, ConnectionService, LiveWriteAuthority,
};
use crate::storage::{Database, ProviderRepo};
use crate::utils::redact::redact_text;
use crate::utils::redact::{api_key_tail, secret_tail_from_masked_preview};

pub use live::ProviderLiveConfigSnapshot;
pub use lock::ProviderLiveSagaGuard;

/// Maximum Unicode scalar values allowed in a provider id.
pub const MAX_PROVIDER_ID_LEN: usize = 128;
/// Maximum Unicode scalar values allowed in a provider name.
pub const MAX_PROVIDER_NAME_LEN: usize = 256;

/// Business facade over [`ProviderRepo`].
#[derive(Clone)]
pub struct ProviderService {
    pub(super) db: Database,
    pub(super) repo: ProviderRepo,
    pub(super) registry: AdapterRegistry,
    pub(super) backup: Option<BackupService>,
    pub(super) authority: LiveWriteAuthority,
    pub(super) connections: ConnectionService,
    pub(super) secret_resolver: AdapterSecretResolver,
    pub(super) adapter_binding_heals: Arc<Mutex<Vec<AdapterBindingHealNotice>>>,
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
            db: db.clone(),
            repo: ProviderRepo::new(db.clone()),
            registry,
            backup: None,
            authority: LiveWriteAuthority::from_database(&db),
            connections: ConnectionService::new(db.clone()),
            secret_resolver: AdapterSecretResolver::new(db),
            adapter_binding_heals: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Construct the full live-switch service with explicit shared
    /// dependencies and backup location.
    pub fn with_live(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self {
            db: db.clone(),
            repo: ProviderRepo::new(db.clone()),
            backup: Some(BackupService::new(
                db.clone(),
                registry.clone(),
                backups_root,
            )),
            registry,
            authority: LiveWriteAuthority::from_database(&db),
            connections: ConnectionService::new(db.clone()),
            secret_resolver: AdapterSecretResolver::new(db),
            adapter_binding_heals: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Storage access for tests. Production reads should use [`Self::get_by_id`]
    /// / [`Self::get_current`] instead of reaching into the repository.
    pub fn repo(&self) -> &ProviderRepo {
        &self.repo
    }
}

pub(super) fn switch_write_last4(provider: &Provider) -> String {
    provider
        .meta
        .get("secretTail")
        .and_then(|value| value.as_str())
        .and_then(logged_last4)
        .or_else(|| api_key_tail(&provider.settings_config))
        .or_else(|| secret_tail_from_masked_preview(&provider.name))
        .unwrap_or_default()
}

fn logged_last4(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(tail) = secret_tail_from_masked_preview(trimmed) {
        return Some(tail);
    }
    if trimmed.len() == 4 && trimmed.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Some(format!("**{trimmed}"));
    }
    None
}

pub(super) fn log_switch_write(agent: AgentId, path: &str, last4: &str) {
    tracing::info!(
        module = targets::PROVIDER,
        op = "switch_write",
        agent = agent.as_str(),
        path,
        last4,
        "wrote live config"
    );
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

pub(super) fn now_ts() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

pub(super) fn is_placeholder_import_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.starts_with("Imported ") && trimmed.len() > "Imported ".len()
}

pub(super) fn ensure_config_agent(config: &AgentConfig, expected: AgentId) -> Result<()> {
    if config.agent != expected {
        return Err(AppError::InvalidArg(format!(
            "adapter returned config for {}, expected {}",
            config.agent.as_str(),
            expected.as_str()
        )));
    }
    require_json_object(&config.raw, "live settings_config")
}

pub(super) fn live_config_is_empty(raw: &serde_json::Value) -> bool {
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

pub(super) fn validate_provider_input(input: &ProviderInput) -> Result<()> {
    validate_id(&input.id)?;
    validate_name(&input.name)?;
    require_json_object(&input.settings_config, "settings_config")?;
    require_json_object(&input.meta, "meta")?;
    Ok(())
}

pub(super) fn validate_id(id: &str) -> Result<()> {
    validate_label(id, "provider id", MAX_PROVIDER_ID_LEN)
}

pub(super) fn validate_name(name: &str) -> Result<()> {
    validate_label(name, "provider name", MAX_PROVIDER_NAME_LEN)
}

pub(super) fn validate_label(value: &str, field: &str, max_chars: usize) -> Result<()> {
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

pub(super) fn require_json_object(value: &serde_json::Value, field: &str) -> Result<()> {
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

pub(super) fn sort_providers(items: &mut [Provider]) {
    items.sort_by(|a, b| {
        agent_rank(a.agent_id)
            .cmp(&agent_rank(b.agent_id))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
}
