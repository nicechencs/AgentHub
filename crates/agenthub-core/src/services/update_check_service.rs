//! Probe remote latest versions for installed agents.
//!
//! Design:
//! - Prefer **npm dist-tags** (not only `/latest`) so pre-release channels can
//!   be considered when the local install is already ahead of `latest`
//!   (Claude Code `next` pre-release channel).
//! - Agents with an npm package get a version compare even when the **install
//!   channel is native** (remote is still npm; note explains the channel).
//! - Native-first agents without a public npm package use an official feed:
//!   Grok CDN pointer, Cursor install-script embed, Kimi CDN (also npm fallback).
//! - Setup-only agents (no npm package + no script) → `unsupported`.
//! - Network failures never pretend "already latest".
//! - Unparseable version pairs → `unknown` (not a false "update available").
//! - Disk cache under `{data_dir}/cache/agent-latest.json` (TTL 1h; key includes
//!   package + local-version bucket so `next` selection is not sticky wrongly).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adapters::AdapterRegistry;
use crate::catalog::install::{
    native_ps1_url, native_setup_url, native_sh_url, npm_package, official_version_probe,
    OfficialVersionProbe,
};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::utils::redact::redact_text;
use crate::models::{AgentId, AgentUpdateInfo, AgentUpdateState, DetectStatus};

/// Default cache TTL for npm latest versions (interactive Agents page).
/// Shorter than the old 12h so reopening the page revalidates within a session;
/// use `force=true` to bypass entirely.
pub const DEFAULT_LATEST_TTL: Duration = Duration::from_secs(60 * 60);

/// Connect / read timeouts for registry probes (keep Agents page snappy).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const READ_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LatestCacheFile {
    /// cache key → cached pick
    entries: BTreeMap<String, CachedLatest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedLatest {
    version: String,
    /// RFC3339 UTC
    fetched_at: String,
    /// Which dist-tag drove the pick (`latest`, `next`, …). Optional for old cache.
    #[serde(default)]
    tag: Option<String>,
}

/// Resolved remote version + provenance.
#[derive(Debug, Clone)]
struct RemoteLatest {
    version: String,
    /// e.g. `npm` or `npm:next`
    source: String,
    checked_at: String,
}

/// Check updates for the given agents (or all when `agents` is empty / None).
pub fn check_agent_updates(
    registry: &AdapterRegistry,
    data_dir: &Path,
    agents: Option<&[AgentId]>,
    force: bool,
) -> Result<Vec<AgentUpdateInfo>> {
    let ids: Vec<AgentId> = match agents {
        Some(list) if !list.is_empty() => list.to_vec(),
        _ => AgentId::ALL.to_vec(),
    };

    let cache_path = latest_cache_path(data_dir);
    let mut cache = load_cache(&cache_path);
    let mut dirty = false;
    let mut out = Vec::with_capacity(ids.len());

    for id in ids {
        let info = check_one(registry, id, force, &mut cache, &mut dirty);
        out.push(info);
    }

    if dirty {
        if let Err(e) = save_cache(&cache_path, &cache) {
            let err_msg = redact_text(&e.to_string());
            tracing::warn!(
                module = targets::INSTALL,
                op = "check_agent_updates",
                error = %err_msg,
                "failed to persist latest-version cache"
            );
        }
    }

    Ok(out)
}

/// Drop cached remote latest for the agent (npm package keys + official feed).
pub fn invalidate_latest_cache(data_dir: &Path, agent: AgentId) {
    let path = latest_cache_path(data_dir);
    let mut cache = load_cache(&path);
    let before = cache.entries.len();
    if let Some(pkg) = npm_package(agent) {
        // Remove any key that starts with this package (includes local-version buckets).
        let prefix = format!("{pkg}|");
        cache.entries.retain(|k, _| k != pkg && !k.starts_with(&prefix));
        // Legacy single-key form.
        cache.entries.remove(pkg);
    }
    if let Some(probe) = official_version_probe(agent) {
        cache.entries.remove(&probe.cache_key());
    }
    if cache.entries.len() != before {
        let _ = save_cache(&path, &cache);
    }
}

fn check_one(
    registry: &AdapterRegistry,
    agent: AgentId,
    force: bool,
    cache: &mut LatestCacheFile,
    dirty: &mut bool,
) -> AgentUpdateInfo {
    let detect = match registry.get(agent) {
        Some(a) => a.detect(),
        None => {
            return AgentUpdateInfo::unknown(
                agent,
                None,
                None,
                format!("adapter not registered: {}", agent.as_str()),
            );
        }
    };

    if detect.status != DetectStatus::Installed {
        return AgentUpdateInfo::not_installed(agent);
    }

    let current = detect.version.clone();
    let channel = detect
        .channel
        .as_deref()
        .map(normalize_channel)
        .unwrap_or("unknown");

    // Setup-only (WorkBuddy): no automated probe — UI links to official Setup page.
    if npm_package(agent).is_none()
        && official_version_probe(agent).is_none()
        && native_ps1_url(agent).is_none()
        && native_sh_url(agent).is_none()
        && native_setup_url(agent).is_some()
    {
        return AgentUpdateInfo::unsupported(
            agent,
            current,
            "该 Agent 仅提供官网 Setup，无法自动检测更新",
            native_setup_url(agent).map(str::to_string),
        );
    }

    let remote = match resolve_remote_latest(
        agent,
        current.as_deref(),
        force,
        cache,
        dirty,
    ) {
        Ok(r) => r,
        Err(e) => {
            return AgentUpdateInfo::unknown(
                agent,
                current,
                Some(channel.into()),
                format!("无法检测更新: {e}"),
            );
        }
    };

    let mut info = compare_versions(
        agent,
        current,
        remote.version,
        remote.source.clone(),
        remote.checked_at,
    );

    // Annotate non-npm installs: remote probe source may differ from install channel.
    if channel != "npm" {
        let base = if remote.source.starts_with("npm") {
            format!(
                "当前安装渠道为 {channel}，已对照 npm dist-tag 版本；升级仍按本机渠道执行"
            )
        } else {
            format!(
                "当前安装渠道为 {channel}，已对照官方版本源（{}）；升级仍按本机渠道执行",
                remote.source
            )
        };
        info.note = Some(match info.note {
            Some(n) if !n.is_empty() => format!("{base}（{n}）"),
            _ => base,
        });
    }

    info
}

/// Resolve remote latest: npm first when available, then official CDN feed.
fn resolve_remote_latest(
    agent: AgentId,
    local_version: Option<&str>,
    force: bool,
    cache: &mut LatestCacheFile,
    dirty: &mut bool,
) -> std::result::Result<RemoteLatest, String> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(pkg) = npm_package(agent) {
        match resolve_npm_remote(agent, pkg, local_version, force, cache, dirty) {
            Ok(r) => return Ok(r),
            Err(e) => errors.push(format!("npm({pkg}): {e}")),
        }
    }

    if let Some(probe) = official_version_probe(agent) {
        match resolve_official_probe(probe, force, cache, dirty) {
            Ok(r) => return Ok(r),
            Err(e) => errors.push(format!("{}: {e}", probe.source())),
        }
    }

    if errors.is_empty() {
        Err("无 npm 包与官方版本源可查询；可强制升级".into())
    } else {
        Err(errors.join("; "))
    }
}

fn resolve_official_probe(
    probe: OfficialVersionProbe,
    force: bool,
    cache: &mut LatestCacheFile,
    dirty: &mut bool,
) -> std::result::Result<RemoteLatest, String> {
    let key = probe.cache_key();
    if !force {
        if let Some(hit) = cache.entries.get(&key) {
            if let Ok(fetched) = DateTime::parse_from_rfc3339(&hit.fetched_at) {
                let age = Utc::now().signed_duration_since(fetched.with_timezone(&Utc));
                if age.num_seconds() >= 0
                    && (age.to_std().unwrap_or(Duration::MAX) < DEFAULT_LATEST_TTL)
                {
                    return Ok(RemoteLatest {
                        version: hit.version.clone(),
                        source: probe.source().into(),
                        checked_at: hit.fetched_at.clone(),
                    });
                }
            }
        }
    }

    let version = match probe {
        OfficialVersionProbe::JsonVersion { url, .. } => {
            let body = http_get_json(url, "application/json")?;
            let v: Value =
                serde_json::from_str(&body).map_err(|e| format!("invalid json: {e}"))?;
            v.get("version")
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "version feed missing version field".to_string())?
        }
        OfficialVersionProbe::PlainVersion { url, .. } => {
            let body = http_get_text(url)?;
            let line = body
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .ok_or_else(|| "version feed empty".to_string())?;
            // Accept bare semver or `v0.1.2`.
            let token = crate::adapters::extract_version_token(line);
            if token.is_empty() {
                return Err(format!("version feed unparseable: {line}"));
            }
            token
        }
        OfficialVersionProbe::ScriptVersion { url, kind, .. } => {
            let body = http_get_text(url)?;
            extract_version_from_script(kind, &body)
                .ok_or_else(|| "install script missing version token".to_string())?
        }
    };

    let checked_at = Utc::now().to_rfc3339();
    cache.entries.insert(
        key,
        CachedLatest {
            version: version.clone(),
            fetched_at: checked_at.clone(),
            tag: Some(probe.source().into()),
        },
    );
    *dirty = true;

    Ok(RemoteLatest {
        version,
        source: probe.source().into(),
        checked_at,
    })
}

fn compare_versions(
    agent: AgentId,
    current: Option<String>,
    latest: String,
    source: String,
    checked_at: String,
) -> AgentUpdateInfo {
    let Some(cur) = current.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return AgentUpdateInfo {
            agent_id: agent,
            state: AgentUpdateState::Unknown,
            current_version: current,
            latest_version: Some(latest),
            source: Some(source),
            checked_at: Some(checked_at),
            note: Some("已安装但未读到本机版本号".into()),
            setup_url: None,
        };
    };

    match version_cmp(cur, &latest) {
        VersionCmp::Less => AgentUpdateInfo {
            agent_id: agent,
            state: AgentUpdateState::UpdateAvailable,
            current_version: Some(cur.to_string()),
            latest_version: Some(latest),
            source: Some(source),
            checked_at: Some(checked_at),
            note: None,
            setup_url: None,
        },
        VersionCmp::Equal | VersionCmp::Greater => AgentUpdateInfo {
            agent_id: agent,
            state: AgentUpdateState::UpToDate,
            current_version: Some(cur.to_string()),
            latest_version: Some(latest),
            source: Some(source),
            checked_at: Some(checked_at),
            note: None,
            setup_url: None,
        },
        VersionCmp::Incomparable => {
            // Fail closed: never claim "update available" on unparseable pairs
            // (string inequality is a common false positive for build metadata).
            AgentUpdateInfo {
                agent_id: agent,
                state: AgentUpdateState::Unknown,
                current_version: Some(cur.to_string()),
                latest_version: Some(latest),
                source: Some(source),
                checked_at: Some(checked_at),
                note: Some("版本号无法严格 semver 比较，已展示远端版本；可强制升级".into()),
                setup_url: None,
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum VersionCmp {
    Less,
    Equal,
    Greater,
    Incomparable,
}

/// Compare two version strings after light normalization.
fn version_cmp(local: &str, remote: &str) -> VersionCmp {
    let a = normalize_version_token(local);
    let b = normalize_version_token(remote);
    if a.is_empty() || b.is_empty() {
        return VersionCmp::Incomparable;
    }
    match (semver::Version::parse(&a), semver::Version::parse(&b)) {
        (Ok(va), Ok(vb)) => {
            if va < vb {
                VersionCmp::Less
            } else if va > vb {
                VersionCmp::Greater
            } else {
                VersionCmp::Equal
            }
        }
        _ => {
            // Try stripping pre-release / build noise via VersionReq-unfriendly forms:
            // take leading x.y.z only.
            match (parse_loose_semver(&a), parse_loose_semver(&b)) {
                (Some(va), Some(vb)) => {
                    if va < vb {
                        VersionCmp::Less
                    } else if va > vb {
                        VersionCmp::Greater
                    } else {
                        VersionCmp::Equal
                    }
                }
                _ => VersionCmp::Incomparable,
            }
        }
    }
}

fn normalize_version_token(s: &str) -> String {
    // Shared with detect (`extract_version_token`) so "codex-cli 0.144.5" compares as 0.144.5.
    crate::adapters::extract_version_token(s)
}

fn parse_loose_semver(s: &str) -> Option<semver::Version> {
    // Extract leading digits.digits.digits (stops at `-` build metadata etc.).
    let mut end = 0;
    let bytes = s.as_bytes();
    let mut dots = 0;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_digit() {
            end += 1;
        } else if c == b'.' {
            dots += 1;
            if dots > 2 {
                break;
            }
            end += 1;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let core = &s[..end].trim_end_matches('.');
    // Parse as integers so leading zeros work (Cursor `2026.07.23-hash`).
    let nums: Vec<u64> = core
        .split('.')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u64>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    let (major, minor, patch) = match nums.as_slice() {
        [a] => (*a, 0, 0),
        [a, b] => (*a, *b, 0),
        [a, b, c, ..] => (*a, *b, *c),
        _ => return None,
    };
    Some(semver::Version::new(major, minor, patch))
}

fn extract_version_from_script(
    kind: crate::catalog::install::ScriptVersionKind,
    body: &str,
) -> Option<String> {
    use crate::catalog::install::ScriptVersionKind;
    match kind {
        ScriptVersionKind::CursorInstall => {
            crate::adapters::cursor::extract_latest_version_from_install_script(body)
        }
    }
}

/// Pre-release dist-tags consulted only when local is strictly ahead of `latest`.
/// Keep the allowlist narrow (Claude `next` only) — other tools have messy tags.
fn npm_prerelease_tags(agent: AgentId) -> &'static [&'static str] {
    match agent {
        AgentId::Claude => &["next"],
        _ => &[],
    }
}

/// Pick display "latest" from dist-tags.
///
/// Default: `latest`. If local is strictly ahead of `latest` and the agent has
/// prerelease tags, take the highest among those tags that beat `latest`.
fn pick_latest_from_dist_tags(
    dist_tags: &BTreeMap<String, String>,
    prerelease_tags: &[&str],
    local_version: Option<&str>,
) -> Option<(String, String)> {
    let latest = dist_tags.get("latest")?.clone();

    let local_ahead = local_version
        .map(|local| version_cmp(local, &latest) == VersionCmp::Greater)
        .unwrap_or(false);

    if prerelease_tags.is_empty() || !local_ahead {
        return Some((latest, "latest".into()));
    }

    let mut best = latest.clone();
    let mut best_tag = "latest".to_string();
    for tag in prerelease_tags {
        if let Some(candidate) = dist_tags.get(*tag) {
            if version_cmp(candidate, &best) == VersionCmp::Greater {
                best = candidate.clone();
                best_tag = (*tag).to_string();
            }
        }
    }
    Some((best, best_tag))
}

fn cache_key(package: &str, local_version: Option<&str>) -> String {
    // Include a coarse local token so "local ahead → next" picks are not reused
    // for a different local version after upgrade/downgrade within TTL.
    let local = local_version
        .map(normalize_version_token)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "_".into());
    format!("{package}|{local}")
}

fn resolve_npm_remote(
    agent: AgentId,
    package: &str,
    local_version: Option<&str>,
    force: bool,
    cache: &mut LatestCacheFile,
    dirty: &mut bool,
) -> std::result::Result<RemoteLatest, String> {
    let key = cache_key(package, local_version);
    if !force {
        if let Some(hit) = cache.entries.get(&key) {
            if let Ok(fetched) = DateTime::parse_from_rfc3339(&hit.fetched_at) {
                let age = Utc::now().signed_duration_since(fetched.with_timezone(&Utc));
                if age.num_seconds() >= 0
                    && (age.to_std().unwrap_or(Duration::MAX) < DEFAULT_LATEST_TTL)
                {
                    let tag = hit.tag.as_deref().unwrap_or("latest");
                    let source = if tag == "latest" {
                        "npm".to_string()
                    } else {
                        format!("npm:{tag}")
                    };
                    return Ok(RemoteLatest {
                        version: hit.version.clone(),
                        source,
                        checked_at: hit.fetched_at.clone(),
                    });
                }
            }
        }
    }

    let dist_tags = fetch_npm_dist_tags(package)?;
    let (version, tag) = pick_latest_from_dist_tags(
        &dist_tags,
        npm_prerelease_tags(agent),
        local_version,
    )
    .ok_or_else(|| "registry dist-tags missing latest".to_string())?;

    let checked_at = Utc::now().to_rfc3339();
    let source = if tag == "latest" {
        "npm".to_string()
    } else {
        format!("npm:{tag}")
    };

    cache.entries.insert(
        key,
        CachedLatest {
            version: version.clone(),
            fetched_at: checked_at.clone(),
            tag: Some(tag),
        },
    );
    *dirty = true;

    Ok(RemoteLatest {
        version,
        source,
        checked_at,
    })
}

fn fetch_npm_dist_tags(package: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let url = npm_package_url(package);
    // Prefer abbreviated packument (install-v1): full docs for large packages
    // like @openai/codex blow past ureq's default into_string limit.
    match http_get_json(
        &url,
        "application/vnd.npm.install-v1+json, application/json;q=0.8",
    ) {
        Ok(body) => parse_dist_tags_json(&body),
        Err(e) => {
            // Fallback: per-tag package.json (`/latest`, etc.) is tiny.
            let err_msg = redact_text(&e);
            tracing::debug!(
                module = targets::INSTALL,
                op = "fetch_npm_dist_tags",
                package,
                error = %err_msg,
                "packument fetch failed; falling back to /latest"
            );
            fetch_npm_dist_tags_via_tag_endpoints(package)
                .map_err(|fb| format!("{e}; fallback: {fb}"))
        }
    }
}

fn parse_dist_tags_json(body: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid json: {e}"))?;
    let tags = v
        .get("dist-tags")
        .and_then(|x| x.as_object())
        .ok_or_else(|| "registry response missing dist-tags".to_string())?;

    let mut out = BTreeMap::new();
    for (k, val) in tags {
        if let Some(s) = val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            out.insert(k.clone(), s.to_string());
        }
    }
    if out.is_empty() {
        return Err("registry dist-tags empty".into());
    }
    Ok(out)
}

/// Tiny responses: `GET /{pkg}/latest` → `{ "version": "…" }`.
fn fetch_npm_dist_tags_via_tag_endpoints(
    package: &str,
) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for tag in ["latest", "next"] {
        let url = format!("{}/{}", npm_package_url(package), tag);
        match http_get_json(&url, "application/json") {
            Ok(body) => {
                let v: Value =
                    serde_json::from_str(&body).map_err(|e| format!("invalid json ({tag}): {e}"))?;
                if let Some(ver) = v
                    .get("version")
                    .and_then(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    out.insert(tag.to_string(), ver.to_string());
                }
            }
            // `next` is optional; only `latest` is required.
            Err(_) if tag != "latest" => continue,
            Err(e) => return Err(e),
        }
    }
    if !out.contains_key("latest") {
        return Err("registry /latest missing version".into());
    }
    Ok(out)
}

fn npm_package_url(package: &str) -> String {
    // Scoped: @scope/name → @scope%2Fname (slash only).
    let encoded = package.replace('/', "%2F");
    format!("https://registry.npmjs.org/{encoded}")
}

/// Legacy helper kept for tests / callers that only need the `/latest` path form.
#[cfg(test)]
fn npm_latest_url(package: &str) -> String {
    format!("{}/latest", npm_package_url(package))
}

fn http_get_json(url: &str, accept: &str) -> std::result::Result<String, String> {
    http_get(url, accept)
}

fn http_get_text(url: &str) -> std::result::Result<String, String> {
    http_get(url, "text/plain, */*;q=0.8")
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
                if let Ok(p) = ureq::Proxy::new(proxy_url) {
                    builder = builder.proxy(p);
                    break;
                }
            }
        }
    }
    let agent = builder.build();
    let ua = format!(
        "AgentHub/{} (+https://github.com/agenthub)",
        env!("CARGO_PKG_VERSION")
    );
    let resp = agent
        .get(url)
        .set("User-Agent", &ua)
        .set("Accept", accept)
        .call()
        .map_err(|e| e.to_string())?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("HTTP {}", resp.status()));
    }
    // ureq hard-caps into_string (~10 MiB). Prefer abbreviated Accept; fallback
    // paths use tiny /latest JSON when the packument is still too large.
    resp.into_string().map_err(|e| e.to_string())
}

fn normalize_channel(raw: &str) -> &'static str {
    let c = raw.trim().to_ascii_lowercase();
    if c == "npm" || (c.starts_with("npm") && !c.contains("native")) {
        "npm"
    } else if c.contains("npm") && !c.contains("native") {
        "npm"
    } else {
        // Prefer native for unknown / mixed labels (force-upgrade still available).
        "native"
    }
}

fn latest_cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("agent-latest.json")
}

fn load_cache(path: &Path) -> LatestCacheFile {
    let Ok(raw) = fs::read_to_string(path) else {
        return LatestCacheFile::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_cache(path: &Path, cache: &LatestCacheFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::message("update.cache", format!("create cache dir: {e}"))
        })?;
    }
    let raw = serde_json::to_string_pretty(cache)
        .map_err(|e| AppError::message("update.cache", format!("serialize cache: {e}")))?;
    fs::write(path, raw)
        .map_err(|e| AppError::message("update.cache", format!("write cache: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_urls_encode_scope_slash() {
        assert_eq!(
            npm_package_url("@openai/codex"),
            "https://registry.npmjs.org/@openai%2Fcodex"
        );
        assert_eq!(
            npm_latest_url("@openai/codex"),
            "https://registry.npmjs.org/@openai%2Fcodex/latest"
        );
        assert_eq!(
            npm_package_url("left-pad"),
            "https://registry.npmjs.org/left-pad"
        );
    }

    #[test]
    fn version_cmp_semver() {
        assert_eq!(version_cmp("1.0.0", "1.0.1"), VersionCmp::Less);
        assert_eq!(version_cmp("v1.2.3", "1.2.3"), VersionCmp::Equal);
        assert_eq!(version_cmp("2.0.0", "1.9.9"), VersionCmp::Greater);
        assert_eq!(version_cmp("1.2", "1.2.1"), VersionCmp::Less);
    }

    #[test]
    fn version_cmp_strips_noise() {
        assert_eq!(
            version_cmp("claude 1.0.5 (x64)", "1.0.6"),
            VersionCmp::Less
        );
    }

    #[test]
    fn version_cmp_prerelease_below_release() {
        // 2.0.0-beta.1 < 2.0.0 (semver)
        assert_eq!(version_cmp("2.0.0-beta.1", "2.0.0"), VersionCmp::Less);
        assert_eq!(version_cmp("2.0.0", "2.0.0-beta.1"), VersionCmp::Greater);
    }

    #[test]
    fn normalize_channel_npm() {
        assert_eq!(normalize_channel("npm"), "npm");
        assert_eq!(normalize_channel("NPM"), "npm");
        assert_eq!(normalize_channel("native"), "native");
    }

    #[test]
    fn cache_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = latest_cache_path(dir.path());
        let mut cache = LatestCacheFile::default();
        cache.entries.insert(
            cache_key("@openai/codex", Some("0.1.0")),
            CachedLatest {
                version: "0.1.0".into(),
                fetched_at: Utc::now().to_rfc3339(),
                tag: Some("latest".into()),
            },
        );
        save_cache(&path, &cache).unwrap();
        let loaded = load_cache(&path);
        assert_eq!(
            loaded
                .entries
                .get(&cache_key("@openai/codex", Some("0.1.0")))
                .unwrap()
                .version,
            "0.1.0"
        );
    }

    #[test]
    fn compare_marks_update_available() {
        let info = compare_versions(
            AgentId::Codex,
            Some("1.0.0".into()),
            "1.1.0".into(),
            "npm".into(),
            Utc::now().to_rfc3339(),
        );
        assert_eq!(info.state, AgentUpdateState::UpdateAvailable);
        assert_eq!(info.latest_version.as_deref(), Some("1.1.0"));
    }

    #[test]
    fn setup_only_agent_unsupported_includes_setup_url() {
        let info = AgentUpdateInfo::unsupported(
            AgentId::WorkBuddy,
            Some("1.0.0".into()),
            "该 Agent 仅提供官网 Setup，无法自动检测更新",
            native_setup_url(AgentId::WorkBuddy).map(str::to_string),
        );
        assert_eq!(info.state, AgentUpdateState::Unsupported);
        assert_eq!(
            info.setup_url.as_deref(),
            Some("https://www.codebuddy.cn/work/")
        );
    }

    #[test]
    fn compare_marks_up_to_date() {
        let info = compare_versions(
            AgentId::Pi,
            Some("0.83.0".into()),
            "0.83.0".into(),
            "npm".into(),
            Utc::now().to_rfc3339(),
        );
        assert_eq!(info.state, AgentUpdateState::UpToDate);
    }

    #[test]
    fn compare_incomparable_is_unknown_not_update() {
        let info = compare_versions(
            AgentId::Codex,
            Some("build-foo".into()),
            "build-bar".into(),
            "npm".into(),
            Utc::now().to_rfc3339(),
        );
        assert_eq!(info.state, AgentUpdateState::Unknown);
        assert_eq!(info.latest_version.as_deref(), Some("build-bar"));
        assert!(info.note.as_deref().unwrap_or("").contains("无法严格"));
    }

    #[test]
    fn cursor_date_build_versions_compare() {
        // Leading-zero months/days must not break semver parse.
        assert_eq!(
            version_cmp("2026.07.23-e383d2b", "2026.07.23-e383d2b"),
            VersionCmp::Equal
        );
        assert_eq!(
            version_cmp("2026.07.23-e383d2b", "2026.08.01-aabbcc1"),
            VersionCmp::Less
        );
        assert_eq!(
            version_cmp("2026.08.01-aabbcc1", "2026.07.23-e383d2b"),
            VersionCmp::Greater
        );
        // Same calendar day, different commit → date-only equal (Cursor agent.ps1 style).
        assert_eq!(
            version_cmp("2026.07.23-aaaaaa1", "2026.07.23-bbbbbb2"),
            VersionCmp::Equal
        );

        let info = compare_versions(
            AgentId::Cursor,
            Some("2026.07.23-e383d2b".into()),
            "2026.08.01-deadbeef".into(),
            "install-script".into(),
            Utc::now().to_rfc3339(),
        );
        assert_eq!(info.state, AgentUpdateState::UpdateAvailable);
        assert_eq!(info.latest_version.as_deref(), Some("2026.08.01-deadbeef"));
    }

    #[test]
    fn pick_latest_defaults_to_latest_tag() {
        let mut tags = BTreeMap::new();
        tags.insert("latest".into(), "1.0.0".into());
        tags.insert("next".into(), "1.1.0-beta.1".into());
        let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("0.9.0")).unwrap();
        assert_eq!(v, "1.0.0");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn pick_latest_uses_next_when_local_ahead() {
        let mut tags = BTreeMap::new();
        tags.insert("latest".into(), "1.0.0".into());
        tags.insert("next".into(), "1.1.0-beta.1".into());
        // Local 1.0.5 is ahead of latest 1.0.0 → consult next
        let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("1.0.5")).unwrap();
        assert_eq!(v, "1.1.0-beta.1");
        assert_eq!(tag, "next");
    }

    #[test]
    fn pick_latest_keeps_latest_when_next_not_higher() {
        let mut tags = BTreeMap::new();
        tags.insert("latest".into(), "2.0.0".into());
        tags.insert("next".into(), "1.9.0".into()); // dirty / lower
        let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("2.0.1")).unwrap();
        assert_eq!(v, "2.0.0");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn claude_has_next_prerelease_tag() {
        assert_eq!(npm_prerelease_tags(AgentId::Claude), &["next"]);
        assert!(npm_prerelease_tags(AgentId::Codex).is_empty());
    }

    #[test]
    fn invalidate_removes_bucketed_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = latest_cache_path(dir.path());
        let mut cache = LatestCacheFile::default();
        cache.entries.insert(
            cache_key("@anthropic-ai/claude-code", Some("1.0.0")),
            CachedLatest {
                version: "1.0.0".into(),
                fetched_at: Utc::now().to_rfc3339(),
                tag: Some("latest".into()),
            },
        );
        cache.entries.insert(
            "@other/pkg".into(),
            CachedLatest {
                version: "9.0.0".into(),
                fetched_at: Utc::now().to_rfc3339(),
                tag: None,
            },
        );
        save_cache(&path, &cache).unwrap();
        invalidate_latest_cache(dir.path(), AgentId::Claude);
        let loaded = load_cache(&path);
        assert!(!loaded
            .entries
            .keys()
            .any(|k| k.contains("@anthropic-ai/claude-code")));
        assert!(loaded.entries.contains_key("@other/pkg"));
    }
}
