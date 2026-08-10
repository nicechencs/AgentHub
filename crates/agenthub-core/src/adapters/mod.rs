//! AgentAdapter trait + registry. One module per agent.

mod claude;
mod codex;
pub(crate) mod cursor;
mod grok;
mod kimi;
mod pi;
pub mod pi_auth;
pub(crate) mod workbuddy;

// Codex OAuth PKCE historically stored flat token bundles; adapters + oauth finish
// both need the same conversion into live `auth_json` shape.
pub(crate) use codex::normalize_oauth_credentials as normalize_codex_oauth_credentials;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityLevel, CapabilityState,
    DetectResult, InstallChannel, LiveAccount, RunOptions, RunSpec,
};
use crate::utils::atomic::atomic_write;
use crate::utils::redact::{mask_secret_preview, redact_text};

pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> AgentId;
    fn detect(&self) -> DetectResult;
    fn install_channels(&self) -> Vec<InstallChannel>;
    fn read_config(&self) -> Result<AgentConfig>;
    /// Atomically replace the agent's live provider configuration.
    ///
    /// Adapters that do not implement a safe writer must fail closed.
    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        Err(AppError::Unsupported(format!(
            "live config writes are not supported for {}",
            self.id().as_str()
        )))
    }
    fn read_auth(&self) -> Result<AuthState>;

    /// Read live file credentials into an opaque snapshot for the account pool.
    ///
    /// Must fail closed with [`AppError::Unsupported`] when credentials cannot
    /// be reliably located (never guess paths).
    fn read_account(&self) -> Result<LiveAccount> {
        Err(AppError::Unsupported(format!(
            "account read is not supported for {}",
            self.id().as_str()
        )))
    }

    /// Atomically apply stored account credentials to live files.
    fn apply_account(&self, _account: &LiveAccount) -> Result<()> {
        Err(AppError::Unsupported(format!(
            "account apply is not supported for {}",
            self.id().as_str()
        )))
    }

    /// Build an API-key live snapshot for `account add-apikey` (no live write).
    fn build_api_key_account(&self, _api_key: &str) -> Result<LiveAccount> {
        Err(AppError::Unsupported(format!(
            "API key accounts are not supported for {}",
            self.id().as_str()
        )))
    }

    /// Authorization fingerprint: same "ticket" only (for pool dedupe).
    ///
    /// Same person with two different OAuth grants must return **different** keys.
    /// Same live re-import must return the **same** key. See `docs/account-authorization-pool.md`.
    fn authorization_key(
        &self,
        kind: AccountKind,
        credentials: &serde_json::Value,
    ) -> Option<String> {
        default_authorization_key(kind, credentials)
    }

    /// Identity label for UI grouping only — never used for dedupe/delete.
    fn identity_label(
        &self,
        kind: AccountKind,
        credentials: &serde_json::Value,
        label_hint: Option<&str>,
    ) -> Option<String> {
        default_identity_label(kind, credentials, label_hint)
    }

    fn skills_dir(&self) -> Option<PathBuf>;
    fn live_backup_paths(&self) -> Vec<PathBuf>;

    /// Build a non-interactive headless run command for this agent.
    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec>;

    /// Declared capability for this agent. Exhaustive match required — no `_ =>`.
    fn capability(&self, cap: Capability) -> CapabilityState;
}

/// Metadata extracted from JSON credential envelopes without retaining any
/// credential values. It is intentionally limited to token presence and
/// expiry state so auth probes cannot leak secrets.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AuthCredentialMetadata {
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub has_api_key: bool,
    pub access_expired: Option<bool>,
    pub refresh_expired: Option<bool>,
    pub has_identity: bool,
}

/// Return an opaque file revision derived from non-secret filesystem metadata.
///
/// Credential bytes are never read or hashed.  The canonical path is only an
/// input to the hash, so it is never exposed to clients.  In addition to the
/// full mtime precision and length, include platform file identity/change
/// metadata: a same-length atomic replacement can otherwise retain a coarse
/// timestamp and evade the optimistic live-switch check.
pub(crate) fn auth_file_revision(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let fingerprint_input = format!(
        "auth-file-revision-v2\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        normalized.to_string_lossy(),
        modified.as_secs(),
        modified.subsec_nanos(),
        metadata.len(),
        auth_file_identity(path, &metadata),
    );
    Some(format!("file:sha256:{}", sha256_hex(&fingerprint_input)))
}

/// Combine several opaque file revisions without exposing their paths or
/// metadata.  Callers retain the input order where that order is meaningful.
pub(crate) fn auth_files_revision(paths: &[&Path]) -> Option<String> {
    let revisions: Vec<String> = paths
        .iter()
        .filter_map(|path| auth_file_revision(path))
        .collect();
    (!revisions.is_empty()).then(|| {
        format!(
            "files:sha256:{}",
            sha256_hex(&format!(
                "auth-files-revision-v2\u{0}{}",
                revisions.join("\u{0}")
            ))
        )
    })
}

#[cfg(unix)]
fn auth_file_identity(_path: &Path, metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;

    format!(
        "unix:{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    )
}

#[cfg(windows)]
fn auth_file_identity(path: &Path, metadata: &std::fs::Metadata) -> String {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            file: *mut std::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let fallback = || {
        format!(
            "windows:fallback:{}:{}:{}",
            metadata.creation_time(),
            metadata.last_write_time(),
            metadata.len(),
        )
    };
    let Ok(file) = std::fs::File::open(path) else {
        return fallback();
    };
    let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::zeroed();
    // `file` owns a valid handle for the duration of this call and the buffer
    // is correctly sized for the documented Win32 structure.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
    if ok == 0 {
        return fallback();
    }
    let information = unsafe { information.assume_init() };
    let file_index =
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low);
    format!(
        "windows:{}:{}:{}:{}",
        information.volume_serial_number,
        file_index,
        metadata.creation_time(),
        metadata.last_write_time(),
    )
}

#[cfg(not(any(unix, windows)))]
fn auth_file_identity(_path: &Path, metadata: &std::fs::Metadata) -> String {
    // Keep a metadata-only fallback for less common targets.  mtime precision
    // and length are already part of the enclosing fingerprint.
    format!(
        "fallback:{}:{}",
        metadata.len(),
        metadata.permissions().readonly()
    )
}

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(input.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Inspect a credential JSON object recursively. Only key names and whether
/// a non-empty value exists are retained; token values are dropped immediately.
pub(crate) fn inspect_auth_credentials(value: &serde_json::Value) -> AuthCredentialMetadata {
    fn visit(value: &serde_json::Value, out: &mut AuthCredentialMetadata) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (raw_key, value) in object {
            let key = raw_key.to_ascii_lowercase().replace(['-', '.'], "_");
            let non_empty = value.as_str().map(str::trim).is_some_and(|s| !s.is_empty());
            match key.as_str() {
                "access" | "access_token" | "accesstoken" | "id_token" | "idtoken" => {
                    out.has_access_token |= non_empty;
                    if let Some(expired) = value_expired(value) {
                        out.access_expired = Some(expired);
                    }
                }
                "refresh" | "refresh_token" | "refreshtoken" => {
                    out.has_refresh_token |= non_empty;
                    if let Some(expired) = value_expired(value) {
                        out.refresh_expired = Some(expired);
                    }
                }
                "expires" | "expires_at" | "expiresat" => {
                    if let Some(expired) = value_expired(value) {
                        out.access_expired = Some(expired);
                    }
                }
                "refresh_expires" | "refresh_expires_at" | "refreshexpiresat" => {
                    if let Some(expired) = value_expired(value) {
                        out.refresh_expired = Some(expired);
                    }
                }
                "api_key" | "apikey" | "openai_api_key" | "key" => {
                    out.has_api_key |= non_empty;
                }
                "email" | "email_address" | "emailaddress" | "user_id" | "userid"
                | "account_id" | "accountid" | "sub" | "name" => {
                    out.has_identity |= non_empty;
                }
                _ => {}
            }
            visit(value, out);
        }
    }

    let mut out = AuthCredentialMetadata::default();
    visit(value, &mut out);
    out
}

/// Derive OAuth health from only explicit token and expiry evidence.
///
/// A refresh token is considered renewable unless its own expiry is explicitly
/// known to have passed. If the refresh token is explicitly expired, an access
/// token that is still valid (or whose expiry is unknown) remains configured;
/// it becomes `NeedsLogin` when the access token is also known expired or is
/// absent altogether.
pub(crate) fn oauth_auth_health(metadata: AuthCredentialMetadata) -> crate::models::AuthHealth {
    use crate::models::AuthHealth;

    match (
        metadata.has_access_token,
        metadata.access_expired,
        metadata.has_refresh_token,
        metadata.refresh_expired,
    ) {
        (false, _, _, Some(true)) => AuthHealth::NeedsLogin,
        (_, Some(true), true, Some(true)) | (_, Some(true), false, _) => AuthHealth::NeedsLogin,
        (_, _, true, Some(false) | None) => AuthHealth::Renewable,
        _ => AuthHealth::Configured,
    }
}

fn value_expired(value: &serde_json::Value) -> Option<bool> {
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    match value {
        serde_json::Value::Number(number) => {
            let timestamp = number.as_i64()?;
            let timestamp = if timestamp.unsigned_abs() > 1_000_000_000_000 {
                timestamp / 1000
            } else {
                timestamp
            };
            Some(timestamp <= now_secs as i64)
        }
        serde_json::Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            if let Ok(number) = text.parse::<i64>() {
                let number = if number.unsigned_abs() > 1_000_000_000_000 {
                    number / 1000
                } else {
                    number
                };
                return Some(number <= now_secs as i64);
            }
            let timestamp = chrono::DateTime::parse_from_rfc3339(text).ok()?.timestamp();
            Some(timestamp <= now_secs as i64)
        }
        _ => None,
    }
}

/// Shared default for [`AgentAdapter::authorization_key`].
///
/// - ApiKey: hash of `api_key`
/// - OAuth: refresh_token hash → access-like token hash → full credentials hash
///
/// Never uses email/user_id (identity ≠ authorization).
pub fn default_authorization_key(
    kind: AccountKind,
    credentials: &serde_json::Value,
) -> Option<String> {
    match kind {
        AccountKind::ApiKey => {
            let key = extract_api_key(credentials)?;
            Some(format!("apikey:sha256:{}", short_sha(&key)))
        }
        AccountKind::Oauth => {
            if let Some(refresh) =
                find_string_field(credentials, &["refresh_token", "refreshToken", "refresh"])
            {
                return Some(format!("oauth:refresh_sha:{}", short_sha(&refresh)));
            }
            if let Some(access) = find_string_field(
                credentials,
                &[
                    "access_token",
                    "accessToken",
                    "access",
                    "id_token",
                    "idToken",
                    // Grok / some OIDC bodies store the bearer under `key`
                    "key",
                ],
            ) {
                return Some(format!("oauth:access_sha:{}", short_sha(&access)));
            }
            let raw = serde_json::to_string(credentials).ok()?;
            Some(format!("oauth:cred_sha:{}", short_sha(&raw)))
        }
    }
}

/// Shared default for [`AgentAdapter::identity_label`] (display only).
pub fn default_identity_label(
    _kind: AccountKind,
    credentials: &serde_json::Value,
    label_hint: Option<&str>,
) -> Option<String> {
    if let Some(s) = find_string_field(
        credentials,
        &[
            "email",
            "email_address",
            "emailAddress",
            "user_id",
            "userId",
            "principal_id",
            "principalId",
            "sub",
            "account_id",
            "accountId",
            "account_uuid",
        ],
    ) {
        return Some(s);
    }
    label_hint
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn extract_api_key(credentials: &serde_json::Value) -> Option<String> {
    credentials
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn short_sha(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    // 16 hex chars is enough to avoid collisions in a local pool
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Find first non-empty string for any of `keys` at top-level, under `body`,
/// under `body.tokens`, or one level of provider-keyed objects under `body`.
fn find_string_field(credentials: &serde_json::Value, keys: &[&str]) -> Option<String> {
    fn from_map(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(s) = obj
                .get(*key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(s.to_string());
            }
        }
        None
    }

    if let Some(obj) = credentials.as_object() {
        if let Some(s) = from_map(obj, keys) {
            return Some(s);
        }
    }
    let body = credentials.get("body")?;
    if let Some(obj) = body.as_object() {
        if let Some(s) = from_map(obj, keys) {
            return Some(s);
        }
        if let Some(tokens) = obj.get("tokens").and_then(|v| v.as_object()) {
            if let Some(s) = from_map(tokens, keys) {
                return Some(s);
            }
        }
        for nested in obj.values() {
            if let Some(nobj) = nested.as_object() {
                if let Some(s) = from_map(nobj, keys) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Process-wide adapter registry (same set as [`register_all`]).
///
/// Used by hot paths that only need capability lookups without rebuilding the map.
fn shared_registry() -> &'static AdapterRegistry {
    static REGISTRY: OnceLock<AdapterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(register_all)
}

/// Whether this agent supports structured stream output (matrix cell).
pub fn supports_structured_stream(agent: AgentId) -> bool {
    shared_registry()
        .get(agent)
        .map(|a| a.capability(Capability::StructuredStream).is_usable())
        .unwrap_or(false)
}

/// ProcessMode + capability matrix lookup (models cannot import adapters).
pub fn wants_structured_for(mode: crate::models::ProcessMode, agent: AgentId) -> bool {
    mode.wants_structured(supports_structured_stream(agent))
}

/// Pretty-print a JSON object, atomically write it, then re-read and verify.
///
/// Shared by account credential writers (Kimi / Grok / …) so verify logic cannot drift.
pub(crate) fn write_verified_json_object(path: &Path, body: &serde_json::Value) -> Result<()> {
    if !body.is_object() {
        return Err(AppError::InvalidArg(
            "credentials body must be a JSON object".into(),
        ));
    }
    let mut bytes = serde_json::to_vec_pretty(body)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    let written = std::fs::read_to_string(path)?;
    let parsed: serde_json::Value = serde_json::from_str(&written)?;
    if &parsed != body {
        tracing::warn!(
            module = targets::ACCOUNT,
            op = "write_verified_json",
            path = %path.display(),
            "JSON verification failed after write"
        );
        return Err(AppError::message(
            "account.verify",
            "credentials file verification failed after write",
        ));
    }
    tracing::debug!(
        module = targets::ACCOUNT,
        op = "write_verified_json",
        path = %path.display(),
        "verified JSON write ok"
    );
    Ok(())
}

/// Trim and reject empty API keys (shared by `build_api_key_account` impls).
pub(crate) fn require_api_key(api_key: &str) -> Result<&str> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("API key must not be empty".into()));
    }
    Ok(key)
}

/// Build a pool `LiveAccount` for an API key (caller supplies credentials + extras).
pub(crate) fn api_key_live_account(
    agent: AgentId,
    key: &str,
    credentials: serde_json::Value,
    label_kind: &str,
    extra: serde_json::Value,
) -> LiveAccount {
    LiveAccount {
        agent,
        kind: AccountKind::ApiKey,
        credentials,
        label_hint: Some(format!("{} ({label_kind})", mask_secret_preview(key))),
        extra,
    }
}

pub(crate) fn write_json_config(path: &Path, config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Claude {
        return Err(crate::error::AppError::InvalidArg(format!(
            "config agent mismatch: expected claude, got {}",
            config.agent.as_str()
        )));
    }
    if !config.raw.is_object() {
        return Err(crate::error::AppError::InvalidArg(
            "Claude settings_config must be a JSON object".into(),
        ));
    }

    let mut bytes = serde_json::to_vec_pretty(&config.raw)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

pub(crate) fn write_toml_config(
    expected: AgentId,
    path: &Path,
    config: &AgentConfig,
) -> Result<()> {
    if config.agent != expected {
        return Err(crate::error::AppError::InvalidArg(format!(
            "config agent mismatch: expected {}, got {}",
            expected.as_str(),
            config.agent.as_str()
        )));
    }
    let object = config.raw.as_object().ok_or_else(|| {
        crate::error::AppError::InvalidArg("TOML settings_config must be a JSON object".into())
    })?;
    if object.get("format").and_then(|value| value.as_str()) != Some("toml") {
        return Err(crate::error::AppError::InvalidArg(
            "TOML settings_config.format must equal 'toml'".into(),
        ));
    }
    // AgentHub: `content`; dual-shape alias: `config`
    let desired = object
        .get("content")
        .or_else(|| object.get("config"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            crate::error::AppError::InvalidArg(
                "TOML settings_config.content (or config) must be a string".into(),
            )
        })?;

    let live = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let merged = merge_toml_provider_config(expected, &live, desired)?;
    crate::utils::atomic::atomic_write(path, merged.as_bytes())
}

fn merge_toml_provider_config(expected: AgentId, live: &str, desired: &str) -> Result<String> {
    use toml_edit::DocumentMut;

    let leading_trivia = leading_toml_trivia(live);
    let mut live_doc = if live.trim().is_empty() {
        DocumentMut::new()
    } else {
        live.parse::<DocumentMut>().map_err(|error| {
            crate::error::AppError::InvalidArg(format!(
                "existing {} TOML config is invalid: {error}",
                expected.as_str()
            ))
        })?
    };
    let desired_doc = desired.parse::<DocumentMut>().map_err(|error| {
        crate::error::AppError::InvalidArg(format!(
            "target {} TOML settings_config is invalid: {error}",
            expected.as_str()
        ))
    })?;

    for key in managed_toml_provider_keys(expected)? {
        live_doc.as_table_mut().remove(key);
    }
    for (key, item) in desired_doc.iter() {
        live_doc.as_table_mut().insert(key, item.clone());
    }

    let rendered = live_doc.to_string();
    if leading_trivia.is_empty() || rendered.starts_with(leading_trivia) {
        Ok(rendered)
    } else {
        Ok(format!("{leading_trivia}{rendered}"))
    }
}

fn leading_toml_trivia(input: &str) -> &str {
    let mut end = 0;
    for segment in input.split_inclusive('\n') {
        let line = segment.trim_end_matches(&['\r', '\n'][..]);
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            end += segment.len();
        } else {
            break;
        }
    }
    &input[..end]
}

fn managed_toml_provider_keys(agent: AgentId) -> Result<&'static [&'static str]> {
    match agent {
        AgentId::Codex => Ok(&[
            "model",
            "review_model",
            "model_provider",
            "model_reasoning_effort",
            "model_reasoning_summary",
            "model_verbosity",
            "model_providers",
            // provider / relay common top-level flags
            "disable_response_storage",
            "preferred_auth_method",
            "network_access",
            "windows_wsl_setup_acknowledged",
            // features.goals / responses_websockets_v2 等随供应商切换整表替换
            "features",
        ]),
        AgentId::Kimi => Ok(&["default_model", "default_provider", "providers"]),
        AgentId::Grok => Ok(&["models", "model", "base_url", "api_key", "env_key"]),
        AgentId::Claude | AgentId::Pi | AgentId::WorkBuddy | AgentId::Cursor => {
            Err(crate::error::AppError::InvalidArg(format!(
                "{} provider config is JSON, not TOML",
                agent.display_name()
            )))
        }
    }
}

#[derive(Clone)]
pub struct AdapterRegistry {
    adapters: HashMap<AgentId, Arc<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn AgentAdapter>) {
        self.adapters.insert(adapter.id(), adapter);
    }

    pub fn get(&self, id: AgentId) -> Option<Arc<dyn AgentAdapter>> {
        self.adapters.get(&id).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn AgentAdapter>> {
        AgentId::ALL
            .iter()
            .filter_map(|id| self.adapters.get(id).cloned())
            .collect()
    }

    /// Global capability matrix for GUI / CLI / docs.
    pub fn matrix(&self) -> BTreeMap<AgentId, BTreeMap<Capability, CapabilityState>> {
        let mut out = BTreeMap::new();
        for adapter in self.all() {
            let mut row = BTreeMap::new();
            for cap in Capability::ALL {
                row.insert(cap, adapter.capability(cap));
            }
            out.insert(adapter.id(), row);
        }
        out
    }

    /// Gate a call site on a declared capability. Partial is allowed through.
    pub fn require(&self, agent: AgentId, cap: Capability) -> Result<Arc<dyn AgentAdapter>> {
        use crate::logging::targets;

        let adapter = self.get(agent).ok_or_else(|| {
            AppError::NotFound(format!("adapter not registered: {}", agent.as_str()))
        })?;
        let state = adapter.capability(cap);
        match state.level {
            CapabilityLevel::Full => Ok(adapter),
            CapabilityLevel::Partial => {
                tracing::debug!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "partial",
                    reason = state.reason.unwrap_or(""),
                    "capability allowed with degradation"
                );
                Ok(adapter)
            }
            CapabilityLevel::Unsupported => {
                let reason = state.reason.unwrap_or("未提供原因");
                tracing::warn!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "unsupported",
                    reason,
                    "capability blocked"
                );
                Err(AppError::Unsupported(format!(
                    "{} 不支持{}：{}",
                    agent.display_name(),
                    cap.label(),
                    reason
                )))
            }
            CapabilityLevel::Planned => {
                let reason = state.reason.unwrap_or("路线图项");
                tracing::info!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "planned",
                    reason,
                    "capability not wired yet"
                );
                Err(AppError::Unsupported(format!(
                    "{}的{}尚未接入 AgentHub：{}",
                    agent.display_name(),
                    cap.label(),
                    reason
                )))
            }
        }
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        register_all()
    }
}

pub fn register_all() -> AdapterRegistry {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(claude::ClaudeAdapter));
    reg.register(Arc::new(codex::CodexAdapter));
    reg.register(Arc::new(kimi::KimiAdapter));
    reg.register(Arc::new(grok::GrokAdapter));
    reg.register(Arc::new(pi::PiAdapter));
    reg.register(Arc::new(workbuddy::WorkBuddyAdapter));
    reg.register(Arc::new(cursor::CursorAdapter));
    reg
}

/// Shared detect helper used by adapters.
///
/// Platform matrix:
/// - PATH / `which` with Win suffixes (`.cmd` / `.exe` / bare)
/// - well-known install dirs that do **not** require the GUI process PATH
///   (Tauri often inherits a stale PATH after installs)
pub(crate) fn detect_binary(
    agent: AgentId,
    candidates: &[&str],
    version_args: &[&str],
    channel_hint: Option<&str>,
    env_ready: bool,
) -> DetectResult {
    use crate::models::DetectStatus;
    use which::which;

    let mut names: Vec<String> = Vec::new();
    for base in candidates {
        for n in expand_binary_names(base) {
            if !names.iter().any(|e| e.eq_ignore_ascii_case(&n)) {
                names.push(n);
            }
        }
    }

    for name in &names {
        if let Ok(path) = which(name) {
            let channel = infer_channel(&path, channel_hint);
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect_binary",
                agent = agent.as_str(),
                via = "path",
                channel = %channel,
                path = %path.display(),
                "agent binary resolved on PATH"
            );
            return finish_detect(
                agent,
                path,
                version_args,
                Some(channel.as_str()),
                env_ready,
                false,
            );
        }
    }

    // Well-known dirs: works when GUI PATH is incomplete after native/npm install.
    for (path, channel) in well_known_bin_paths(agent) {
        if path.is_file() {
            // PATH miss but disk hit — common after install without AgentHub restart.
            tracing::info!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect_binary",
                agent = agent.as_str(),
                via = "well_known",
                channel = channel,
                path = %path.display(),
                "agent binary found outside process PATH (well-known dir); restart may refresh PATH"
            );
            return finish_detect(agent, path, version_args, Some(channel), env_ready, true);
        }
    }

    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "detect_binary",
        agent = agent.as_str(),
        candidates = ?names,
        "agent binary not found on PATH or well-known dirs"
    );

    DetectResult {
        agent,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready,
        notes: vec![NOT_FOUND_FIREFIGHTING_NOTE.into()],
    }
}

/// Surfaced in DetectResult.notes and searchable in doctor / GUI when binary is missing.
pub(crate) const NOT_FOUND_FIREFIGHTING_NOTE: &str =
    "binary not on PATH and not found in well-known install dirs; \
     if you just installed, restart AgentHub or click re-detect after PATH refresh";

/// Expand a base command name with platform-typical suffixes.
fn expand_binary_names(base: &str) -> Vec<String> {
    let mut out = vec![base.to_string()];
    #[cfg(windows)]
    {
        // npm global shims are often `name.cmd`; native bins are `name.exe`.
        if !base.ends_with(".cmd") && !base.ends_with(".exe") && !base.ends_with(".ps1") {
            out.push(format!("{base}.cmd"));
            out.push(format!("{base}.exe"));
            out.push(format!("{base}.ps1"));
        }
    }
    out
}

/// Allowlisted install locations (platform × agent). Channel is `npm` or `native`.
fn well_known_bin_paths(agent: AgentId) -> Vec<(PathBuf, &'static str)> {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return Vec::new();
    };
    let name = agent.as_str();
    let mut paths: Vec<(PathBuf, &'static str)> = Vec::new();

    // Shared helpers: native home bins + npm global prefix shims.
    let push_native = |paths: &mut Vec<(PathBuf, &'static str)>, dir: PathBuf| {
        #[cfg(windows)]
        {
            paths.push((dir.join(format!("{name}.exe")), "native"));
        }
        paths.push((dir.join(name), "native"));
    };
    let push_npm = |paths: &mut Vec<(PathBuf, &'static str)>, dir: PathBuf| {
        #[cfg(windows)]
        {
            paths.push((dir.join(format!("{name}.cmd")), "npm"));
            paths.push((dir.join(format!("{name}.ps1")), "npm"));
            paths.push((dir.join(format!("{name}.exe")), "npm"));
        }
        paths.push((dir.join(name), "npm"));
    };

    match agent {
        AgentId::Claude => {
            push_native(&mut paths, home.join(".local").join("bin"));
            // npm global (Windows AppData\Roaming\npm, macOS common prefixes)
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
        }
        AgentId::Codex => {
            // Codex is primarily npm; native install may also land under ~/.local/bin.
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            // Some native/codex layouts
            push_native(&mut paths, home.join(".codex").join("bin"));
        }
        AgentId::Kimi => {
            push_native(&mut paths, home.join(".kimi-code").join("bin"));
            push_native(&mut paths, home.join(".kimi").join("bin"));
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
        }
        AgentId::Grok => {
            push_native(&mut paths, home.join(".grok").join("bin"));
            push_native(&mut paths, home.join(".local").join("bin"));
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
        }
        AgentId::Pi => {
            // Primary: npm global (`pi` / `pi.cmd`). Optional native-style home bins.
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            push_native(&mut paths, home.join(".pi").join("bin"));
            push_native(&mut paths, home.join(".pi").join("agent").join("bin"));
        }
        AgentId::WorkBuddy => {
            // Electron desktop under LocalAppData\Programs\WorkBuddy (not PATH/npm).
            #[cfg(windows)]
            {
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    paths.push((
                        PathBuf::from(local)
                            .join("Programs")
                            .join("WorkBuddy")
                            .join("WorkBuddy.exe"),
                        "native",
                    ));
                }
            }
            #[cfg(not(windows))]
            {
                paths.push((
                    PathBuf::from("/Applications/WorkBuddy.app/Contents/MacOS/WorkBuddy"),
                    "native",
                ));
            }
            let _ = home;
        }
        AgentId::Cursor => {
            // Prefer cursor-agent install trees — never bare `agent` under .grok.
            // Full validation lives in CursorAdapter::detect; well-known paths here
            // only feed shared helpers / uninstall allowlists.
            for (p, ch) in cursor::uninstall_bin_candidates()
                .into_iter()
                .map(|p| (p, "native"))
            {
                paths.push((p, ch));
            }
            let _ = home;
        }
    }

    paths
}

fn npm_global_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("npm"));
        }
        // Fallback when APPDATA is missing (rare): classic Roaming path under home.
        dirs.push(home.join("AppData").join("Roaming").join("npm"));
    }
    #[cfg(not(windows))]
    {
        // Common npm global bin locations on macOS/Linux (PATH may omit them in GUI).
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        dirs.push(home.join(".local").join("bin"));
        // nvm default current
        if let Ok(nvm) = std::env::var("NVM_DIR") {
            let nvm_dir = PathBuf::from(nvm);
            if let Ok(rd) = std::fs::read_dir(nvm_dir.join("versions").join("node")) {
                // Prefer newest version dir by name (v22.x > v18.x).
                let mut versions: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();
                if let Some(latest) = versions.pop() {
                    dirs.push(latest.join("bin"));
                }
            }
        }
    }
    dirs
}

/// Prefer concrete channel (`npm` / `native`) over ambiguous hints like `npm-or-native`.
fn infer_channel(path: &Path, hint: Option<&str>) -> String {
    let s = path.to_string_lossy().to_ascii_lowercase();
    let from_path = if s.contains(std::path::MAIN_SEPARATOR) {
        // Windows npm shim: ...\AppData\Roaming\npm\xxx.cmd
        // Unix npm: .../node_modules/... or .../npm-global/...
        if s.contains(&format!("{sep}npm{sep}", sep = std::path::MAIN_SEPARATOR))
            || s.contains("/npm/")
            || s.ends_with(".cmd")
            || s.contains("node_modules")
            || s.contains("npm-global")
        {
            Some("npm")
        } else if s.contains(".local")
            || s.contains(".grok")
            || s.contains(".kimi-code")
            || s.contains(".kimi")
            || s.contains(".codex")
            || s.contains(".pi")
            || s.contains("workbuddy")
            || s.contains("programs") && s.contains("workbuddy")
            || s.contains("cursor-agent")
            || s.contains(".cursor")
        {
            Some("native")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(c) = from_path {
        return c.to_string();
    }
    match hint {
        Some("npm") | Some("native") => hint.unwrap().to_string(),
        Some(h) if h.contains("npm") && !h.contains("native") => "npm".into(),
        Some(h) if h.contains("native") && !h.contains("npm") => "native".into(),
        // Ambiguous (e.g. claude "npm-or-native"): default native when path looks like home bin.
        _ => "native".into(),
    }
}

/// Reject Windows/cmd noise that is clearly not a version string.
pub(crate) fn looks_like_version_line(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.len() > 120 {
        return false;
    }
    let lower = l.to_ascii_lowercase();
    if lower.contains("not recognized")
        || lower.contains("not found")
        || lower.contains("cannot find")
        || lower.contains("is not recognized")
        || l.contains("不是内部或外部命令")
        || l.contains("无法识别")
        || l.contains("系统找不到")
    {
        return false;
    }
    // Prefer lines that look like versions (digit present or known CLI name prefixes).
    l.chars().any(|c| c.is_ascii_digit())
        || lower.starts_with("claude")
        || lower.starts_with("codex")
        || lower.starts_with("kimi")
        || lower.starts_with("grok")
        || lower.starts_with("pi")
        || lower.starts_with("cursor")
        || lower.starts_with("cursor-agent")
}

/// Extract a display / compare-friendly version token from CLI `--version` output.
///
/// Examples:
/// - `codex-cli 0.144.5` → `0.144.5`
/// - `2.1.220 (Claude Code)` → `2.1.220`
/// - `grok 0.2.118 (1e1687c1cf)` → `0.2.118`
/// - `0.83.0` → `0.83.0`
///
/// If no digit-leading token is found, returns the trimmed original line.
pub(crate) fn extract_version_token(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // Split on whitespace and parentheses so "2.1.220 (Claude Code)" yields "2.1.220".
    let token = s
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .find(|p| {
            let p = p.trim_start_matches(['v', 'V']);
            p.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .unwrap_or(s);
    let token = token.trim_start_matches(['v', 'V']);
    let cleaned = token
        .trim_matches(|c: char| c == ',' || c == ';' || c == ')' || c == '(')
        .to_string();
    if cleaned.chars().any(|c| c.is_ascii_digit()) {
        cleaned
    } else {
        s.to_string()
    }
}

/// Binary was resolved on disk — always `Installed`. Version probe timeout stays
/// Installed with empty version + note (never map timeout → NotFound).
fn finish_detect(
    agent: AgentId,
    path: std::path::PathBuf,
    version_args: &[&str],
    channel_hint: Option<&str>,
    env_ready: bool,
    via_well_known: bool,
) -> DetectResult {
    use crate::models::DetectStatus;
    use crate::utils::process::{run_capture, stdout_first_line};

    let mut notes = Vec::new();
    if via_well_known {
        notes.push(format!(
            "found via well-known path (not on process PATH): {}; \
             restart AgentHub after installs if PATH still incomplete",
            path.display()
        ));
    }

    let version = match run_capture(&path, version_args) {
        Ok(o) => {
            if o.status.success() {
                stdout_first_line(&o)
                    .filter(|l| looks_like_version_line(l))
                    .map(|l| extract_version_token(&l))
                    .filter(|l| !l.is_empty())
            } else {
                // Some CLIs print version on stderr; never treat shell/PATH errors as a version.
                let err = String::from_utf8_lossy(&o.stderr);
                let candidate = err
                    .lines()
                    .next()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && looks_like_version_line(l))
                    .map(|l| extract_version_token(&l))
                    .filter(|l| !l.is_empty());
                if candidate.is_none() {
                    let out = String::from_utf8_lossy(&o.stdout);
                    let hint = err
                        .lines()
                        .chain(out.lines())
                        .next()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty());
                    if let Some(h) = hint {
                        let safe = crate::utils::redact::redact_text(h);
                        notes.push(format!(
                            "version probe failed (binary present at {}): {safe}",
                            path.display()
                        ));
                        tracing::debug!(
                            target: crate::logging::targets::DETECT,
                            module = crate::logging::targets::DETECT,
                            op = "version_probe",
                            agent = agent.as_str(),
                            path = %path.display(),
                            "version probe non-zero: {safe}"
                        );
                    }
                }
                candidate
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            notes.push(format!(
                "version probe timed out (binary present at {})",
                path.display()
            ));
            tracing::warn!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "version_probe",
                agent = agent.as_str(),
                path = %path.display(),
                "version probe timed out (binary still counted as Installed)"
            );
            None
        }
        Err(e) => {
            let err_msg = redact_text(&e.to_string());
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "version_probe",
                agent = agent.as_str(),
                path = %path.display(),
                error = %err_msg,
                "version probe spawn/io failed"
            );
            None
        }
    };

    let channel = channel_hint.map(|s| {
        if s == "npm" || s == "native" {
            s.to_string()
        } else {
            infer_channel(&path, Some(s))
        }
    });

    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "finish_detect",
        agent = agent.as_str(),
        channel = channel.as_deref().unwrap_or("-"),
        version = version.as_deref().unwrap_or("-"),
        via_well_known,
        env_ready,
        path = %path.display(),
        "agent marked Installed"
    );

    DetectResult {
        agent,
        status: DetectStatus::Installed,
        version,
        binary_path: Some(path),
        channel,
        env_ready,
        notes,
    }
}

#[cfg(test)]
mod tests;
