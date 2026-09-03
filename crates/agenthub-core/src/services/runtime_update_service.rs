//! Probe latest versions for shared runtimes.
//!
//! Runtime checks are deliberately kept in Core so the GUI never talks to a
//! registry directly. Successful responses are cached on disk for one hour;
//! a failed network request is always `unknown`, never `up_to_date`.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{EnvStatus, EnvStatusKind, RuntimeId, RuntimeUpdateInfo, RuntimeUpdateState};

pub const DEFAULT_RUNTIME_LATEST_TTL: Duration = Duration::from_secs(60 * 60);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const READ_TIMEOUT: Duration = Duration::from_secs(6);
const NODE_INDEX_URL: &str = "https://nodejs.org/dist/index.json";
const NPM_LATEST_URL: &str = "https://registry.npmjs.org/npm/latest";
const GIT_LATEST_URL: &str = "https://api.github.com/repos/git/git/releases/latest";
const POWERSHELL_LATEST_URL: &str =
    "https://api.github.com/repos/PowerShell/PowerShell/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RuntimeLatestCache {
    entries: BTreeMap<String, CachedRuntimeLatest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedRuntimeLatest {
    version: String,
    fetched_at: String,
}

#[derive(Debug, Clone)]
struct RemoteLatest {
    version: String,
    source: &'static str,
    checked_at: String,
}

/// Check selected runtimes, or every runtime in `statuses` when `ids` is None.
pub fn check_runtime_updates(
    data_dir: &Path,
    statuses: &[EnvStatus],
    ids: Option<&[RuntimeId]>,
    force: bool,
) -> Result<Vec<RuntimeUpdateInfo>> {
    let selected: Vec<RuntimeId> = match ids {
        Some(ids) if !ids.is_empty() => ids.to_vec(),
        _ => statuses.iter().map(|status| status.id).collect(),
    };
    let by_id: HashMap<RuntimeId, &EnvStatus> = statuses.iter().map(|s| (s.id, s)).collect();
    let cache_path = latest_cache_path(data_dir);
    let mut cache = load_cache(&cache_path);
    let mut dirty = false;
    let mut result = Vec::with_capacity(selected.len());

    for id in selected {
        let Some(status) = by_id.get(&id).copied() else {
            result.push(RuntimeUpdateInfo::not_installed(id));
            continue;
        };
        result.push(check_one(status, force, &mut cache, &mut dirty));
    }

    if dirty {
        if let Err(error) = save_cache(&cache_path, &cache) {
            tracing::warn!(
                module = targets::INSTALL,
                op = "check_runtime_updates",
                error = %error,
                "failed to persist runtime latest-version cache"
            );
        }
    }
    Ok(result)
}

/// Drop the cached latest version after a runtime install or upgrade.
pub fn invalidate_runtime_latest_cache(data_dir: &Path, id: RuntimeId) {
    let path = latest_cache_path(data_dir);
    let mut cache = load_cache(&path);
    let mut changed = cache.entries.remove(id.as_str()).is_some();
    // Node install also changes npm, and vice versa.
    if matches!(id, RuntimeId::NodeJs | RuntimeId::Npm) {
        changed |= cache.entries.remove(RuntimeId::NodeJs.as_str()).is_some();
        changed |= cache.entries.remove(RuntimeId::Npm.as_str()).is_some();
    }
    if changed {
        let _ = save_cache(&path, &cache);
    }
}

fn check_one(
    status: &EnvStatus,
    force: bool,
    cache: &mut RuntimeLatestCache,
    dirty: &mut bool,
) -> RuntimeUpdateInfo {
    if status.status == EnvStatusKind::Missing {
        return RuntimeUpdateInfo::not_installed(status.id);
    }

    let current = status.version.clone();
    let source = source_for(status.id);
    if status.status == EnvStatusKind::BrokenPath {
        return RuntimeUpdateInfo::unknown(
            status.id,
            current,
            Some(source.into()),
            "运行环境路径异常，无法可靠比较版本",
            Some(setup_url(status.id).into()),
            false,
        );
    }
    let Some(current_version) = current.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
        return RuntimeUpdateInfo::unknown(
            status.id,
            current,
            Some(source.into()),
            "已安装但未读到本机版本号",
            Some(setup_url(status.id).into()),
            false,
        );
    };

    let remote = match resolve_remote(status.id, force, cache, dirty) {
        Ok(remote) => remote,
        Err(error) => {
            return RuntimeUpdateInfo::unknown(
                status.id,
                Some(current_version.to_string()),
                Some(source.into()),
                format!("无法检测更新: {error}"),
                Some(setup_url(status.id).into()),
                supports_auto_upgrade(status.id),
            )
        }
    };

    let state = match compare_versions(current_version, &remote.version) {
        VersionCmp::Less => RuntimeUpdateState::UpdateAvailable,
        VersionCmp::Equal | VersionCmp::Greater => RuntimeUpdateState::UpToDate,
        VersionCmp::Incomparable => RuntimeUpdateState::Unknown,
    };
    let note = if state == RuntimeUpdateState::Unknown {
        Some("版本号无法严格比较，已展示远端版本".into())
    } else {
        None
    };
    RuntimeUpdateInfo {
        runtime_id: status.id,
        state,
        current_version: Some(current_version.to_string()),
        latest_version: Some(remote.version),
        source: Some(remote.source.into()),
        checked_at: Some(remote.checked_at),
        note,
        setup_url: Some(setup_url(status.id).into()),
        can_auto_upgrade: supports_auto_upgrade(status.id),
    }
}

fn supports_auto_upgrade(id: RuntimeId) -> bool {
    if cfg!(windows) {
        return matches!(id, RuntimeId::NodeJs | RuntimeId::Npm | RuntimeId::Git);
    }
    if cfg!(target_os = "macos") {
        return match id {
            RuntimeId::NodeJs | RuntimeId::Npm => true,
            RuntimeId::Git => crate::runtime::resolve_binary(&["brew"]).is_some(),
            RuntimeId::PowerShell => false,
        };
    }
    false
}

fn resolve_remote(
    id: RuntimeId,
    force: bool,
    cache: &mut RuntimeLatestCache,
    dirty: &mut bool,
) -> std::result::Result<RemoteLatest, String> {
    let key = id.as_str();
    if !force {
        if let Some(hit) = cache.entries.get(key) {
            if let Ok(fetched) = DateTime::parse_from_rfc3339(&hit.fetched_at) {
                let age = Utc::now().signed_duration_since(fetched.with_timezone(&Utc));
                if age.num_seconds() >= 0
                    && age.to_std().unwrap_or(Duration::MAX) < DEFAULT_RUNTIME_LATEST_TTL
                {
                    return Ok(RemoteLatest {
                        version: hit.version.clone(),
                        source: source_for(id),
                        checked_at: hit.fetched_at.clone(),
                    });
                }
            }
        }
    }

    let version = fetch_latest(id)?;
    let checked_at = Utc::now().to_rfc3339();
    cache.entries.insert(
        key.into(),
        CachedRuntimeLatest {
            version: version.clone(),
            fetched_at: checked_at.clone(),
        },
    );
    *dirty = true;
    Ok(RemoteLatest {
        version,
        source: source_for(id),
        checked_at,
    })
}

fn fetch_latest(id: RuntimeId) -> std::result::Result<String, String> {
    match id {
        RuntimeId::NodeJs => {
            let body = http_get(NODE_INDEX_URL, "application/json")?;
            let rows: Vec<Value> =
                serde_json::from_str(&body).map_err(|e| format!("invalid json: {e}"))?;
            rows.iter()
                .filter_map(|row| {
                    let lts = row.get("lts")?;
                    let is_lts = match lts {
                        Value::Bool(value) => *value,
                        Value::String(value) => !value.trim().is_empty() && value != "false",
                        _ => false,
                    };
                    if !is_lts {
                        return None;
                    }
                    row.get("version")?.as_str().map(normalize_version)
                })
                .filter(|version| !version.is_empty())
                .max_by(|left, right| compare_versions(left, right).ordering())
                .ok_or_else(|| "Node.js LTS feed missing a version".into())
        }
        RuntimeId::Npm => {
            let body = http_get(NPM_LATEST_URL, "application/json")?;
            json_version(&body)
        }
        RuntimeId::Git | RuntimeId::PowerShell => {
            let url = if id == RuntimeId::Git {
                GIT_LATEST_URL
            } else {
                POWERSHELL_LATEST_URL
            };
            let body = http_get(url, "application/vnd.github+json, application/json;q=0.8")?;
            let value: Value =
                serde_json::from_str(&body).map_err(|e| format!("invalid json: {e}"))?;
            value
                .get("tag_name")
                .and_then(Value::as_str)
                .map(normalize_version)
                .filter(|version| !version.is_empty())
                .ok_or_else(|| "official release feed missing tag_name".into())
        }
    }
}

fn json_version(body: &str) -> std::result::Result<String, String> {
    let value: Value = serde_json::from_str(body).map_err(|e| format!("invalid json: {e}"))?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(normalize_version)
        .filter(|version| !version.is_empty())
        .ok_or_else(|| "version feed missing version".into())
}

fn source_for(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::NodeJs => "nodejs.org",
        RuntimeId::Npm => "npm",
        RuntimeId::Git => "git",
        RuntimeId::PowerShell => "powershell",
    }
}

fn setup_url(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => "https://nodejs.org/",
        RuntimeId::Git => "https://git-scm.com/downloads",
        RuntimeId::PowerShell => {
            "https://learn.microsoft.com/powershell/scripting/install/installing-powershell"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionCmp {
    Less,
    Equal,
    Greater,
    Incomparable,
}

impl VersionCmp {
    fn ordering(self) -> std::cmp::Ordering {
        match self {
            Self::Less => std::cmp::Ordering::Less,
            Self::Equal => std::cmp::Ordering::Equal,
            Self::Greater => std::cmp::Ordering::Greater,
            Self::Incomparable => std::cmp::Ordering::Equal,
        }
    }
}

fn compare_versions(left: &str, right: &str) -> VersionCmp {
    let (Some(left), Some(right)) = (
        parse_comparable_version(left),
        parse_comparable_version(right),
    ) else {
        return VersionCmp::Incomparable;
    };
    match left.cmp(&right) {
        std::cmp::Ordering::Less => VersionCmp::Less,
        std::cmp::Ordering::Equal => VersionCmp::Equal,
        std::cmp::Ordering::Greater => VersionCmp::Greater,
    }
}

/// Parse common CLI versions leniently while retaining proper semver prerelease
/// ordering when it is available. Windows Git adds a `.windows.1` suffix and
/// Windows PowerShell commonly reports only `5.1`; neither is strict semver.
fn parse_comparable_version(raw: &str) -> Option<semver::Version> {
    let normalized = normalize_version(raw);
    if let Ok(version) = semver::Version::parse(&normalized) {
        return Some(version);
    }

    let numeric = normalized
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect::<String>()
        .trim_end_matches('.')
        .to_string();
    let mut parts = numeric
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    parts.resize(3, 0);
    Some(semver::Version::new(parts[0], parts[1], parts[2]))
}

fn normalize_version(raw: &str) -> String {
    let token = crate::adapters::extract_version_token(raw);
    token.trim_start_matches('v').to_string()
}

fn http_get(url: &str, accept: &str) -> std::result::Result<String, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT);
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(proxy_url) = std::env::var(key) {
            let proxy_url = proxy_url.trim();
            if !proxy_url.is_empty() {
                if let Ok(proxy) = ureq::Proxy::new(proxy_url) {
                    builder = builder.proxy(proxy);
                    break;
                }
            }
        }
    }
    let agent = builder.build();
    let user_agent = format!(
        "AgentHub/{} (+https://github.com/nicechencs/AgentHub)",
        env!("CARGO_PKG_VERSION")
    );
    let response = agent
        .get(url)
        .set("User-Agent", &user_agent)
        .set("Accept", accept)
        .call()
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("HTTP {}", response.status()));
    }
    response.into_string().map_err(|error| error.to_string())
}

fn latest_cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("runtime-latest.json")
}

fn load_cache(path: &Path) -> RuntimeLatestCache {
    let Ok(raw) = fs::read_to_string(path) else {
        return RuntimeLatestCache::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_cache(path: &Path, cache: &RuntimeLatestCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::message("runtime.update.cache", format!("create cache dir: {error}"))
        })?;
    }
    let raw = serde_json::to_string_pretty(cache).map_err(|error| {
        AppError::message("runtime.update.cache", format!("serialize cache: {error}"))
    })?;
    fs::write(path, raw).map_err(|error| {
        AppError::message("runtime.update.cache", format!("write cache: {error}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
