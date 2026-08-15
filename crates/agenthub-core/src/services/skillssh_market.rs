//! skills.sh remote market — search API + leaderboard HTML, install via git clone.
//!
//! Public endpoints (no auth):
//! - `GET https://skills.sh/api/search?q=…&limit=…`
//! - Leaderboard pages at `https://skills.sh/` (and `/trending`, `/hot`) embed skill JSON

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::skill_market::SkillMarket;
use crate::catalog::limits::{
    SKILLS_SH_CONNECT_TIMEOUT, SKILLS_SH_CURL_CONNECT_SECS, SKILLS_SH_CURL_MAX_SECS,
    SKILLS_SH_READ_TIMEOUT,
};
#[cfg(windows)]
use crate::catalog::limits::SKILLS_SH_POWERSHELL_TIMEOUT_SECS;
use crate::catalog::market::{
    skills_sh_detail_url, skills_sh_home_url, skills_sh_search_url, skills_sh_user_agent,
    DEFAULT_SKILLS_SH_LIMIT,
};
use crate::error::{AppError, Result};
use crate::models::{SkillListing, SkillLocalPayload};

#[derive(Debug, Clone)]
struct ShSkill {
    /// Unique listing id: `{source}/{skill_id}`
    id: String,
    skill_id: String,
    name: String,
    source: String,
    installs: u64,
}

/// Remote provider backed by [skills.sh](https://skills.sh/).
pub struct SkillsShMarket {
    limit: usize,
}

impl SkillsShMarket {
    pub fn new() -> Self {
        Self {
            limit: DEFAULT_SKILLS_SH_LIMIT,
        }
    }

    fn http_get(url: &str) -> Result<String> {
        // Prefer curl.exe on Windows (often handles system proxy / fake-ip better).
        let curl_err = match Self::http_get_curl(url) {
            Ok(body) => return Ok(body),
            Err(e) => e,
        };
        let ureq_err = match Self::http_get_ureq(url) {
            Ok(body) => return Ok(body),
            Err(e) => e,
        };
        let ps_err = match Self::http_get_powershell(url) {
            Ok(body) => return Ok(body),
            Err(e) => e,
        };
        Err(AppError::message(
            "skill.market",
            format!(
                "skills.sh unreachable (curl: {curl_err}; ureq: {ureq_err}; powershell: {ps_err}). \
                 Check network/proxy (HTTPS_PROXY) or open {} in a browser.",
                skills_sh_home_url()
            ),
        ))
    }

    fn http_get_ureq(url: &str) -> std::result::Result<String, String> {
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(SKILLS_SH_CONNECT_TIMEOUT)
            .timeout_read(SKILLS_SH_READ_TIMEOUT);
        // Honor common proxy env vars (Clash / corporate proxies).
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
        let ua = skills_sh_user_agent();
        let resp = agent
            .get(url)
            .set("User-Agent", &ua)
            .set("Accept", "application/json, text/html;q=0.9,*/*;q=0.8")
            .call()
            .map_err(|e| e.to_string())?;
        resp.into_string().map_err(|e| e.to_string())
    }

    fn http_get_curl(url: &str) -> std::result::Result<String, String> {
        // Windows 10+ ships curl.exe; often respects system proxy better than pure Rust stacks.
        let bin = if cfg!(windows) { "curl.exe" } else { "curl" };
        let ua = skills_sh_user_agent();
        let connect = SKILLS_SH_CURL_CONNECT_SECS.to_string();
        let max_time = SKILLS_SH_CURL_MAX_SECS.to_string();
        let output = Command::new(bin)
            .args([
                "-fsSL",
                "--connect-timeout",
                &connect,
                "--max-time",
                &max_time,
                "-A",
                &ua,
                "-H",
                "Accept: application/json, text/html;q=0.9,*/*;q=0.8",
                url,
            ])
            .output()
            .map_err(|e| format!("spawn curl failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "curl exit {:?} {}",
                output.status.code(),
                stderr.trim()
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| format!("curl utf8: {e}"))
    }

    #[cfg(windows)]
    fn http_get_powershell(url: &str) -> std::result::Result<String, String> {
        let ua = skills_sh_user_agent();
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; \
             (Invoke-WebRequest -Uri '{url}' -UseBasicParsing -TimeoutSec {timeout} \
               -Headers @{{'User-Agent'='{ua}'}}).Content",
            url = url.replace('\'', "''"),
            timeout = SKILLS_SH_POWERSHELL_TIMEOUT_SECS,
            ua = ua.replace('\'', "''"),
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|e| format!("spawn powershell failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "powershell exit {:?} {}",
                output.status.code(),
                stderr.trim()
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| format!("powershell utf8: {e}"))
    }

    #[cfg(not(windows))]
    fn http_get_powershell(_url: &str) -> std::result::Result<String, String> {
        Err("powershell not available".into())
    }

    fn api_search(&self, query: &str) -> Result<Vec<ShSkill>> {
        let url = format!(
            "{}?q={}&limit={}",
            skills_sh_search_url(),
            urlencoding::encode(query),
            self.limit
        );
        let body = Self::http_get(&url)?;
        let v: Value = serde_json::from_str(&body).map_err(|e| {
            AppError::message("skill.market", format!("skills.sh search JSON: {e}"))
        })?;
        let arr = v
            .get("skills")
            .and_then(|x| x.as_array())
            .or_else(|| v.as_array())
            .ok_or_else(|| {
                AppError::message("skill.market", "skills.sh search: missing skills array")
            })?;
        Ok(parse_skills_array(arr))
    }

    fn leaderboard(&self) -> Result<Vec<ShSkill>> {
        let html = Self::http_get(&skills_sh_home_url())?;
        if let Ok(skills) = parse_next_data(&html) {
            if !skills.is_empty() {
                return Ok(skills.into_iter().take(self.limit).collect());
            }
        }
        let skills = parse_embedded_loose(&html);
        if skills.is_empty() {
            // Fallback: soft search so empty market UI still has data when HTML shape changes.
            return self.api_search("skill");
        }
        Ok(skills.into_iter().take(self.limit).collect())
    }

    /// Clone GitHub source and locate the skill directory (contains SKILL.md).
    fn materialize_skill(&self, listing_id: &str) -> Result<(PathBuf, PathBuf)> {
        let sh = parse_listing_id(listing_id)?;
        let git_url = format!("https://github.com/{}.git", sh.source);
        let tmp = tempfile::tempdir()
            .map_err(|e| AppError::message("skill.market", format!("tempdir failed: {e}")))?;
        let repo = tmp.path().join("repo");
        let status = Command::new("git")
            .args(["clone", "--depth", "1", &git_url])
            .arg(&repo)
            .status()
            .map_err(|e| {
                AppError::message(
                    "skill.market",
                    format!("git clone failed (is git installed?): {e}"),
                )
            })?;
        if !status.success() {
            return Err(AppError::message(
                "skill.market",
                format!("git clone failed for {git_url}"),
            ));
        }

        let skill_dir = find_skill_dir(&repo, &sh.skill_id).ok_or_else(|| {
            AppError::NotFound(format!(
                "SKILL.md for '{}' not found under {}",
                sh.skill_id, sh.source
            ))
        })?;

        let cleanup = tmp.path().to_path_buf();
        // Keep temp alive until caller finishes install.
        std::mem::forget(tmp);
        Ok((skill_dir, cleanup))
    }
}

impl Default for SkillsShMarket {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillMarket for SkillsShMarket {
    fn id(&self) -> &str {
        "skills.sh"
    }

    fn name(&self) -> &str {
        "skills.sh 市场"
    }

    fn search(&self, query: &str) -> Result<Vec<SkillListing>> {
        let q = query.trim();
        let raw = if q.is_empty() {
            self.leaderboard()?
        } else {
            self.api_search(q)?
        };
        Ok(raw.into_iter().map(to_listing).collect())
    }

    fn fetch(&self, id: &str) -> Result<SkillLocalPayload> {
        let (path, cleanup) = self.materialize_skill(id)?;
        // Best-effort: leave cleanup dir; install command will remove if path is under it.
        // Store cleanup root in locator so install_market can wipe it.
        Ok(SkillLocalPayload {
            path,
            version: None,
            source_locator: format!("market:skills.sh:{id};cleanup={}", cleanup.display()),
        })
    }
}

fn to_listing(s: ShSkill) -> SkillListing {
    let installs = if s.installs >= 1_000_000 {
        format!("{:.1}M", s.installs as f64 / 1_000_000.0)
    } else if s.installs >= 1_000 {
        format!("{:.1}K", s.installs as f64 / 1_000.0)
    } else {
        s.installs.to_string()
    };
    let detail_url = skills_sh_detail_url(&s.id);
    SkillListing {
        id: s.id,
        name: s.name,
        description: format!("来自 {} · {} 次安装", s.source, installs),
        version: None,
        provider_id: "skills.sh".into(),
        installed: false,
        detail_url: Some(detail_url),
    }
}

fn parse_listing_id(id: &str) -> Result<ShSkill> {
    // Formats:
    // - owner/repo/skill
    // - owner/repo/nested/skill (skill_id may contain path - take last segment as skill name)
    let id = id.trim().trim_start_matches("market:skills.sh:");
    let parts: Vec<&str> = id.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return Err(AppError::InvalidArg(format!(
            "invalid skills.sh id (expected owner/repo/skill): {id}"
        )));
    }
    let source = format!("{}/{}", parts[0], parts[1]);
    let skill_id = parts[2..].join("/");
    // Prefer last segment as skill folder name for lookup.
    let folder = parts.last().copied().unwrap_or(skill_id.as_str());
    Ok(ShSkill {
        id: id.to_string(),
        skill_id: folder.to_string(),
        name: folder.to_string(),
        source,
        installs: 0,
    })
}

fn parse_skills_array(arr: &[Value]) -> Vec<ShSkill> {
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for item in arr {
        let source = item
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let skill_id = item
            .get("skillId")
            .or_else(|| item.get("skill_id"))
            .or_else(|| item.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // API sometimes returns full id in "id" already as owner/repo/skill
        let (source, skill_id) = if source.is_empty() {
            if let Some((s, sk)) = split_source_skill(&skill_id) {
                (s, sk)
            } else {
                continue;
            }
        } else if skill_id.contains('/') {
            // skillId field might be full path
            let folder = skill_id.rsplit('/').next().unwrap_or(&skill_id).to_string();
            (source, folder)
        } else {
            (source, skill_id)
        };
        if source.is_empty() || skill_id.is_empty() {
            continue;
        }
        let id = format!("{source}/{skill_id}");
        if !seen.insert(id.clone()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&skill_id)
            .to_string();
        let installs = item.get("installs").and_then(|v| v.as_u64()).unwrap_or(0);
        skills.push(ShSkill {
            id,
            skill_id,
            name,
            source,
            installs,
        });
    }
    skills
}

fn split_source_skill(full: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = full.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return None;
    }
    Some((
        format!("{}/{}", parts[0], parts[1]),
        parts[2..].join("/").rsplit('/').next()?.to_string(),
    ))
}

fn parse_next_data(html: &str) -> Result<Vec<ShSkill>> {
    let marker = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let start = html
        .find(marker)
        .ok_or_else(|| AppError::message("skill.market", "__NEXT_DATA__ not found"))?
        + marker.len();
    let end_rel = html[start..]
        .find("</script>")
        .ok_or_else(|| AppError::message("skill.market", "__NEXT_DATA__ unclosed"))?;
    let json_str = &html[start..start + end_rel];
    let data: Value = serde_json::from_str(json_str)
        .map_err(|e| AppError::message("skill.market", format!("__NEXT_DATA__ JSON: {e}")))?;
    let arr = data
        .pointer("/props/pageProps/initialSkills")
        .or_else(|| data.pointer("/props/pageProps/skills"))
        .or_else(|| data.pointer("/props/pageProps/items"))
        .and_then(|v| v.as_array());
    match arr {
        Some(a) => Ok(parse_skills_array(a)),
        None => Ok(Vec::new()),
    }
}

/// Best-effort scan for JSON skill objects when Next data shape changes.
fn parse_embedded_loose(html: &str) -> Vec<ShSkill> {
    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    let mut rest = html;
    while let Some(idx) = rest.find("\"source\"") {
        rest = &rest[idx..];
        // Pull a window and try to find skillId + installs nearby.
        let window = &rest[..rest.len().min(800)];
        if let Some(s) = try_parse_window(window) {
            if seen.insert(s.id.clone()) {
                skills.push(s);
            }
        }
        rest = &rest[8..];
    }
    skills
}

fn try_parse_window(window: &str) -> Option<ShSkill> {
    let source = extract_json_string(window, "source")?;
    let skill_id = extract_json_string(window, "skillId")
        .or_else(|| extract_json_string(window, "skill_id"))?;
    if source.is_empty() || skill_id.is_empty() {
        return None;
    }
    let folder = skill_id.rsplit('/').next().unwrap_or(&skill_id).to_string();
    let name = extract_json_string(window, "name").unwrap_or_else(|| folder.clone());
    let installs = extract_json_u64(window, "installs").unwrap_or(0);
    let id = format!("{source}/{folder}");
    Some(ShSkill {
        id,
        skill_id: folder,
        name,
        source,
        installs,
    })
}

fn extract_json_string(window: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let i = window.find(&pat)?;
    let after = &window[i + pat.len()..];
    let colon = after.find(':')?;
    let mut s = after[colon + 1..].trim_start();
    if !s.starts_with('"') {
        return None;
    }
    s = &s[1..];
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
            continue;
        }
        if c == '"' {
            break;
        }
        out.push(c);
    }
    Some(out)
}

fn extract_json_u64(window: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{key}\"");
    let i = window.find(&pat)?;
    let after = &window[i + pat.len()..];
    let colon = after.find(':')?;
    let s = after[colon + 1..].trim_start();
    let num: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

fn find_skill_dir(repo: &Path, skill_id: &str) -> Option<PathBuf> {
    // Prefer exact path segments commonly used by skill repos.
    let candidates = [
        repo.join(skill_id),
        repo.join("skills").join(skill_id),
        repo.join("packages").join(skill_id),
        repo.join(".agents").join("skills").join(skill_id),
        repo.join(".claude").join("skills").join(skill_id),
    ];
    for c in candidates {
        if c.join("SKILL.md").is_file() {
            return Some(c);
        }
    }
    // Recursive shallow search (depth-limited walk).
    find_skill_dir_rec(repo, skill_id, 0, 5)
}

fn find_skill_dir_rec(
    dir: &Path,
    skill_id: &str,
    depth: usize,
    max_depth: usize,
) -> Option<PathBuf> {
    if depth > max_depth {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    for ent in entries.flatten() {
        let path = ent.path();
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if name == skill_id && path.join("SKILL.md").is_file() {
            return Some(path);
        }
        if let Some(found) = find_skill_dir_rec(&path, skill_id, depth + 1, max_depth) {
            return Some(found);
        }
    }
    None
}

/// Install a skills.sh listing into the shared library (path-based install + temp cleanup).
pub fn install_skills_sh_listing(
    skills: &super::SkillService,
    listing_id: &str,
    overwrite: bool,
) -> Result<crate::models::Skill> {
    let market = SkillsShMarket::new();
    let (package_dir, cleanup) = market.materialize_skill(listing_id)?;
    let path_str = package_dir.to_string_lossy().to_string();
    let result = skills.install_skill(&path_str, overwrite);
    let _ = fs::remove_dir_all(&cleanup);
    result
}

/// Map market listing id → local skill directory id (last path segment).
///
/// Supports skills.sh (`owner/repo/skill`) and skillhub (`skillhub:slug[@ver]`).
pub fn local_skill_id_from_market_id(market_id: &str) -> String {
    if super::skillhub_market::is_skillhub_listing_id(market_id) {
        return super::skillhub_market::local_skill_id_from_skillhub_id(market_id);
    }
    parse_listing_id(market_id)
        .map(|s| s.skill_id)
        .unwrap_or_else(|_| {
            market_id
                .rsplit('/')
                .next()
                .unwrap_or(market_id)
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_payload() {
        let v: Value = serde_json::from_str(
            r#"{"skills":[{"id":"vercel-labs/agent-skills/foo","skillId":"foo","name":"Foo","installs":12,"source":"vercel-labs/agent-skills"}]}"#,
        )
        .unwrap();
        let arr = v.get("skills").unwrap().as_array().unwrap();
        let skills = parse_skills_array(arr);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "vercel-labs/agent-skills/foo");
        assert_eq!(skills[0].skill_id, "foo");
    }

    #[test]
    fn parse_next_data_fixture() {
        let html = r#"
        <html><script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"initialSkills":[{"source":"antfu/skills","skillId":"vite","name":"vite","installs":152}]}}}
        </script></html>
        "#;
        let skills = parse_next_data(html).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "antfu/skills/vite");
    }

    #[test]
    fn listing_id_parse() {
        let s = parse_listing_id("vercel-labs/agent-skills/vercel-react-best-practices").unwrap();
        assert_eq!(s.source, "vercel-labs/agent-skills");
        assert_eq!(s.skill_id, "vercel-react-best-practices");
    }
}
