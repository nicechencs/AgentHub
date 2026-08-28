//! skillhub.cn remote market — public list API + zip download install.
//!
//! Public endpoints (no auth):
//! - `GET https://api.skillhub.cn/api/skills?page=&pageSize=&keyword=…`
//! - `GET https://api.skillhub.cn/api/v1/download?slug=…` (302 → zip)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::skill_market::SkillMarket;
#[cfg(windows)]
use crate::catalog::limits::SKILLS_SH_POWERSHELL_TIMEOUT_SECS;
use crate::catalog::limits::{
    SKILLS_SH_CONNECT_TIMEOUT, SKILLS_SH_CURL_CONNECT_SECS, SKILLS_SH_CURL_MAX_SECS,
    SKILLS_SH_READ_TIMEOUT,
};
use crate::catalog::market::{
    skill_market_user_agent, skillhub_detail_url, skillhub_download_url, skillhub_home_url,
    skillhub_skills_list_url, DEFAULT_SKILLHUB_LIMIT,
};
use crate::error::{AppError, Result};
use crate::models::{SkillListing, SkillLocalPayload};

/// Listing id prefix: `skillhub:{slug}` or `skillhub:{slug}@{version}`.
pub const SKILLHUB_ID_PREFIX: &str = "skillhub:";

#[derive(Debug, Clone)]
struct HubSkill {
    /// `skillhub:{slug}` or `skillhub:{slug}@{version}`
    id: String,
    slug: String,
    /// Publisher handle for detail URL (`/{handle}/{slug}`).
    handle: Option<String>,
    name: String,
    description: String,
    version: Option<String>,
    downloads: u64,
    source: String,
}

/// Remote provider backed by [skillhub.cn](https://skillhub.cn/).
pub struct SkillhubMarket {
    limit: usize,
}

impl SkillhubMarket {
    pub fn new() -> Self {
        Self {
            limit: DEFAULT_SKILLHUB_LIMIT,
        }
    }

    fn http_get(url: &str) -> Result<String> {
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
                "skillhub.cn unreachable (curl: {curl_err}; ureq: {ureq_err}; powershell: {ps_err}). \
                 Check network/proxy or open {} in a browser.",
                skillhub_home_url()
            ),
        ))
    }

    fn http_get_ureq(url: &str) -> std::result::Result<String, String> {
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(SKILLS_SH_CONNECT_TIMEOUT)
            .timeout_read(SKILLS_SH_READ_TIMEOUT);
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
        let ua = skill_market_user_agent();
        let resp = agent
            .get(url)
            .set("User-Agent", &ua)
            .set("Accept", "application/json")
            .call()
            .map_err(|e| e.to_string())?;
        resp.into_string().map_err(|e| e.to_string())
    }

    fn http_get_curl(url: &str) -> std::result::Result<String, String> {
        let bin = if cfg!(windows) { "curl.exe" } else { "curl" };
        let ua = skill_market_user_agent();
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
                "Accept: application/json",
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
        let ua = skill_market_user_agent();
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; \
             (Invoke-WebRequest -Uri '{url}' -UseBasicParsing -TimeoutSec {timeout} \
               -Headers @{{'User-Agent'='{ua}';'Accept'='application/json'}}).Content",
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

    fn api_list(&self, query: &str) -> Result<Vec<HubSkill>> {
        let mut url = format!(
            "{}?page=1&pageSize={}",
            skillhub_skills_list_url(),
            self.limit
        );
        let q = query.trim();
        if q.is_empty() {
            url.push_str("&sortBy=downloads&order=desc");
        } else {
            url.push_str("&keyword=");
            url.push_str(&urlencoding::encode(q));
        }
        let body = Self::http_get(&url)?;
        parse_skills_page(&body)
    }

    /// Download zip for slug, extract, locate SKILL.md package dir.
    fn materialize_skill(&self, listing_id: &str) -> Result<(PathBuf, PathBuf)> {
        let (slug, version) = parse_skillhub_id(listing_id)?;
        let download_url = skillhub_download_url(&slug, version.as_deref());
        let tmp = tempfile::tempdir()
            .map_err(|e| AppError::message("skill.market", format!("tempdir failed: {e}")))?;
        let zip_path = tmp.path().join("skill.zip");
        let extract_dir = tmp.path().join("extracted");
        fs::create_dir_all(&extract_dir)
            .map_err(|e| AppError::message("skill.market", format!("mkdir extract failed: {e}")))?;

        download_file(&download_url, &zip_path)?;
        extract_zip(&zip_path, &extract_dir)?;

        let skill_dir = find_skill_package(&extract_dir).ok_or_else(|| {
            AppError::NotFound(format!(
                "SKILL.md not found in skillhub package for slug '{slug}'"
            ))
        })?;

        let cleanup = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        Ok((skill_dir, cleanup))
    }
}

impl Default for SkillhubMarket {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillMarket for SkillhubMarket {
    fn id(&self) -> &str {
        "skillhub.cn"
    }

    fn name(&self) -> &str {
        "SkillHub 市场"
    }

    fn search(&self, query: &str) -> Result<Vec<SkillListing>> {
        Ok(self.api_list(query)?.into_iter().map(to_listing).collect())
    }

    fn fetch(&self, id: &str) -> Result<SkillLocalPayload> {
        let (path, cleanup) = self.materialize_skill(id)?;
        Ok(SkillLocalPayload {
            path,
            version: parse_skillhub_id(id).ok().and_then(|(_, v)| v),
            source_locator: format!("market:skillhub.cn:{id};cleanup={}", cleanup.display()),
        })
    }
}

fn to_listing(s: HubSkill) -> SkillListing {
    let dl = if s.downloads >= 1_000_000 {
        format!("{:.1}M", s.downloads as f64 / 1_000_000.0)
    } else if s.downloads >= 1_000 {
        format!("{:.1}K", s.downloads as f64 / 1_000.0)
    } else {
        s.downloads.to_string()
    };
    let mut desc = s.description;
    if desc.is_empty() {
        desc = format!("来自 skillhub.cn · {} · {} 次下载", s.source, dl);
    } else {
        desc = format!("{desc} · {} 次下载", dl);
    }
    let detail_url = skillhub_detail_url(s.handle.as_deref(), &s.slug);
    SkillListing {
        id: s.id,
        name: s.name,
        description: desc,
        version: s.version,
        provider_id: "skillhub.cn".into(),
        installed: false,
        detail_url: Some(detail_url),
    }
}

/// Parse `skillhub:{slug}` / `skillhub:{slug}@{version}` / bare slug.
pub fn parse_skillhub_id(id: &str) -> Result<(String, Option<String>)> {
    let raw = id
        .trim()
        .trim_start_matches("market:skillhub.cn:")
        .trim_start_matches(SKILLHUB_ID_PREFIX)
        .trim();
    if raw.is_empty() {
        return Err(AppError::InvalidArg(format!(
            "invalid skillhub id (expected skillhub:slug): {id}"
        )));
    }
    // Do not treat `@handle/slug` namespace form as version — version uses trailing @semver.
    if let Some((slug, ver)) = raw.rsplit_once('@') {
        let ver = ver.trim();
        let slug = slug.trim();
        if !slug.is_empty()
            && !ver.is_empty()
            && ver.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Ok((slug.to_string(), Some(ver.to_string())));
        }
    }
    Ok((raw.to_string(), None))
}

pub fn is_skillhub_listing_id(id: &str) -> bool {
    let t = id.trim();
    t.starts_with(SKILLHUB_ID_PREFIX) || t.starts_with("market:skillhub.cn:")
}

fn parse_skills_page(body: &str) -> Result<Vec<HubSkill>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| AppError::message("skill.market", format!("skillhub.cn list JSON: {e}")))?;
    // { code: 0, data: { skills: [...], total }, message }
    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 0 {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(AppError::message(
                "skill.market",
                format!("skillhub.cn API code={code}: {msg}"),
            ));
        }
    }
    let arr = v
        .pointer("/data/skills")
        .and_then(|x| x.as_array())
        .or_else(|| v.get("skills").and_then(|x| x.as_array()))
        .ok_or_else(|| {
            AppError::message(
                "skill.market",
                "skillhub.cn list: missing data.skills array",
            )
        })?;
    Ok(parse_skills_array(arr))
}

fn parse_skills_array(arr: &[Value]) -> Vec<HubSkill> {
    let mut out = Vec::new();
    for item in arr {
        let slug = item
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if slug.is_empty() {
            continue;
        }
        let version = item
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let id = match &version {
            Some(v) => format!("{SKILLHUB_ID_PREFIX}{slug}@{v}"),
            None => format!("{SKILLHUB_ID_PREFIX}{slug}"),
        };
        let handle = item
            .pointer("/namespace/handle")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("ownerName").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&slug)
            .to_string();
        let description = item
            .get("description_zh")
            .or_else(|| item.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let downloads = item
            .get("downloads")
            .and_then(|v| v.as_u64())
            .or_else(|| item.get("installs").and_then(|v| v.as_u64()))
            .unwrap_or(0);
        let source = item
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("skillhub")
            .to_string();
        out.push(HubSkill {
            id,
            slug,
            handle,
            name,
            description,
            version,
            downloads,
            source,
        });
    }
    out
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let bin = if cfg!(windows) { "curl.exe" } else { "curl" };
    let ua = skill_market_user_agent();
    let connect = SKILLS_SH_CURL_CONNECT_SECS.to_string();
    let max_time = (SKILLS_SH_CURL_MAX_SECS * 3).to_string();
    let output = Command::new(bin)
        .args([
            "-fsSL",
            "-L",
            "--connect-timeout",
            &connect,
            "--max-time",
            &max_time,
            "-A",
            &ua,
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .output()
        .map_err(|e| {
            AppError::message(
                "skill.market",
                format!("skillhub download (curl) failed: {e}"),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::message(
            "skill.market",
            format!(
                "skillhub download failed (curl exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            ),
        ));
    }
    let meta = fs::metadata(dest).map_err(|e| {
        AppError::message(
            "skill.market",
            format!("skillhub zip missing after download: {e}"),
        )
    })?;
    if meta.len() == 0 {
        return Err(AppError::message(
            "skill.market",
            "skillhub download produced empty zip",
        ));
    }
    Ok(())
}

fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    crate::platform::skills::zip_extract::extract_zip_file(zip_path, dest_dir)
}

fn find_skill_package(root: &Path) -> Option<PathBuf> {
    if root.join("SKILL.md").is_file() {
        return Some(root.to_path_buf());
    }
    // Prefer single top-level dir containing SKILL.md
    if let Ok(entries) = fs::read_dir(root) {
        let dirs: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        for d in &dirs {
            if d.join("SKILL.md").is_file() {
                return Some(d.clone());
            }
        }
    }
    find_skill_md_rec(root, 0, 4)
}

fn find_skill_md_rec(dir: &Path, depth: usize, max_depth: usize) -> Option<PathBuf> {
    if depth > max_depth {
        return None;
    }
    if dir.join("SKILL.md").is_file() {
        return Some(dir.to_path_buf());
    }
    let entries = fs::read_dir(dir).ok()?;
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.eq_ignore_ascii_case("__MACOSX") {
            continue;
        }
        if let Some(found) = find_skill_md_rec(&path, depth + 1, max_depth) {
            return Some(found);
        }
    }
    None
}

/// Install a skillhub listing into the shared library.
pub fn install_skillhub_listing(
    skills: &super::SkillService,
    listing_id: &str,
    overwrite: bool,
) -> Result<crate::models::Skill> {
    let market = SkillhubMarket::new();
    let (package_dir, cleanup) = market.materialize_skill(listing_id)?;
    let path_str = package_dir.to_string_lossy().to_string();
    let result = skills.install_skill(&path_str, overwrite);
    let _ = fs::remove_dir_all(&cleanup);
    result
}

/// Local shared-library id for a skillhub listing (slug last segment).
pub fn local_skill_id_from_skillhub_id(market_id: &str) -> String {
    parse_skillhub_id(market_id)
        .map(|(slug, _)| {
            slug.rsplit('/')
                .next()
                .unwrap_or(&slug)
                .trim_start_matches('@')
                .to_string()
        })
        .unwrap_or_else(|_| {
            market_id
                .trim_start_matches(SKILLHUB_ID_PREFIX)
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
    fn parse_list_payload() {
        let body = r#"{
          "code":0,
          "message":"success",
          "data":{
            "skills":[{
              "slug":"find-skills",
              "name":"Find Skills",
              "description":"discover skills",
              "description_zh":"发现技能",
              "downloads":1000,
              "source":"clawhub",
              "version":"1.0.0"
            }],
            "total":1
          }
        }"#;
        let skills = parse_skills_page(body).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "skillhub:find-skills@1.0.0");
        assert_eq!(skills[0].slug, "find-skills");
        assert_eq!(skills[0].name, "Find Skills");
        assert_eq!(skills[0].description, "发现技能");
        let listing = to_listing(skills[0].clone());
        assert_eq!(
            listing.detail_url.as_deref(),
            Some("https://skillhub.cn/skills/find-skills")
        );
    }

    #[test]
    fn detail_url_prefers_namespace_handle() {
        let body = r#"{
          "code":0,
          "data":{
            "skills":[{
              "slug":"self-improving-agent",
              "name":"self-improving agent",
              "description":"x",
              "downloads":10,
              "source":"clawhub",
              "version":"3.0.24",
              "namespace":{"handle":"pskoett","publicSlug":"self-improving-agent"}
            }],
            "total":1
          }
        }"#;
        let skills = parse_skills_page(body).unwrap();
        assert_eq!(skills[0].handle.as_deref(), Some("pskoett"));
        let listing = to_listing(skills[0].clone());
        assert_eq!(
            listing.detail_url.as_deref(),
            Some("https://skillhub.cn/skills/pskoett/self-improving-agent")
        );
    }

    #[test]
    fn parse_ids() {
        let (s, v) = parse_skillhub_id("skillhub:find-skills@1.0.0").unwrap();
        assert_eq!(s, "find-skills");
        assert_eq!(v.as_deref(), Some("1.0.0"));
        let (s2, v2) = parse_skillhub_id("skillhub:find-skills").unwrap();
        assert_eq!(s2, "find-skills");
        assert!(v2.is_none());
        assert!(is_skillhub_listing_id("skillhub:x"));
        assert!(!is_skillhub_listing_id("vercel-labs/agent-skills/foo"));
    }
}
