//! Pluggable skill market providers.
//!
//! Default registry is driven by [`SkillMarketSource`] settings:
//! - `skills.sh` / `skillhub.cn` — single provider
//! - `auto` — skills.sh first, fall back to skillhub.cn when the primary is unreachable

use std::path::{Path, PathBuf};

use crate::catalog::market::SkillMarketSource;
use crate::error::{AppError, Result};
use crate::models::{SkillListing, SkillLocalPayload};

/// Market provider contract (search + fetch).
pub trait SkillMarket: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn search(&self, query: &str) -> Result<Vec<SkillListing>>;
    fn fetch(&self, id: &str) -> Result<SkillLocalPayload>;
}

/// Built-in local catalog — example data for the Skills Market tab.
pub struct BuiltinSkillMarket {
    /// Optional directory of example skill packages (each subdir = one skill).
    catalog_dir: Option<PathBuf>,
}

impl BuiltinSkillMarket {
    pub fn new(catalog_dir: Option<PathBuf>) -> Self {
        Self { catalog_dir }
    }

    /// Static demo listings used when no on-disk catalog is configured.
    fn demo_listings() -> Vec<SkillListing> {
        vec![
            SkillListing {
                id: "example-hello".into(),
                name: "Hello Skill".into(),
                description: "Built-in demo skill for market UI (not a real package).".into(),
                version: Some("0.1.0".into()),
                provider_id: "builtin".into(),
                installed: false,
                detail_url: None,
            },
            SkillListing {
                id: "example-notes".into(),
                name: "Notes Helper".into(),
                description: "Demo listing reserved for offline catalog demos.".into(),
                version: Some("0.0.1".into()),
                provider_id: "builtin".into(),
                installed: false,
                detail_url: None,
            },
        ]
    }
}

impl SkillMarket for BuiltinSkillMarket {
    fn id(&self) -> &str {
        "builtin"
    }

    fn name(&self) -> &str {
        "内置目录"
    }

    fn search(&self, query: &str) -> Result<Vec<SkillListing>> {
        let q = query.trim().to_ascii_lowercase();
        let mut items = if let Some(dir) = &self.catalog_dir {
            if dir.is_dir() {
                scan_catalog_dir(dir, self.id())?
            } else {
                Self::demo_listings()
            }
        } else {
            Self::demo_listings()
        };
        if !q.is_empty() {
            items.retain(|item| {
                item.id.to_ascii_lowercase().contains(&q)
                    || item.name.to_ascii_lowercase().contains(&q)
                    || item.description.to_ascii_lowercase().contains(&q)
            });
        }
        Ok(items)
    }

    fn fetch(&self, id: &str) -> Result<SkillLocalPayload> {
        if let Some(dir) = &self.catalog_dir {
            let skill_dir = dir.join(id);
            if skill_dir.join("SKILL.md").is_file() {
                return Ok(SkillLocalPayload {
                    path: skill_dir,
                    version: None,
                    source_locator: format!("market:builtin:{id}"),
                });
            }
        }
        Err(AppError::NotFound(format!(
            "builtin market skill not available for install: {id} (demo listing only)"
        )))
    }
}

fn scan_catalog_dir(dir: &Path, provider_id: &str) -> Result<Vec<SkillListing>> {
    let mut out = Vec::new();
    for ent in std::fs::read_dir(dir)? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").is_file() {
            continue;
        }
        let (display, description) = read_name_desc(&path, &name);
        out.push(SkillListing {
            id: name,
            name: display,
            description,
            version: None,
            provider_id: provider_id.to_string(),
            installed: false,
            detail_url: None,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn read_name_desc(dir: &Path, fallback: &str) -> (String, String) {
    let content = std::fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
    // Share frontmatter rules with SkillService (incl. YAML `|` / `>` block scalars).
    super::skill_service::parse_skill_frontmatter(&content, fallback)
}

/// Registry of market providers (id → trait object).
pub struct SkillMarketRegistry {
    providers: Vec<Box<dyn SkillMarket>>,
    /// When true, [`Self::search_configured`] tries providers in order until one succeeds.
    fallback: bool,
}

impl SkillMarketRegistry {
    pub fn with_builtin(catalog_dir: Option<PathBuf>) -> Self {
        Self {
            providers: vec![Box::new(BuiltinSkillMarket::new(catalog_dir))],
            fallback: false,
        }
    }

    /// Default GUI/CLI market: skills.sh first (real data). Kept for callers that
    /// do not load settings yet.
    pub fn with_defaults() -> Self {
        Self::from_source(SkillMarketSource::DEFAULT)
    }

    /// skills.sh + optional local builtin catalog (merge mode, no auto-fallback).
    pub fn with_skills_sh_and_builtin(catalog_dir: Option<PathBuf>) -> Self {
        Self {
            providers: vec![
                Box::new(super::skillssh_market::SkillsShMarket::new()),
                Box::new(BuiltinSkillMarket::new(catalog_dir)),
            ],
            fallback: false,
        }
    }

    /// Build registry from user preference (`auto` | `skills.sh` | `skillhub.cn`).
    pub fn from_source(source: SkillMarketSource) -> Self {
        match source {
            SkillMarketSource::SkillsSh => Self {
                providers: vec![Box::new(super::skillssh_market::SkillsShMarket::new())],
                fallback: false,
            },
            SkillMarketSource::SkillhubCn => Self {
                providers: vec![Box::new(super::skillhub_market::SkillhubMarket::new())],
                fallback: false,
            },
            SkillMarketSource::Auto => Self {
                // Primary: skills.sh (existing default). Fallback: skillhub.cn for
                // regions / networks where skills.sh is unreachable.
                providers: vec![
                    Box::new(super::skillssh_market::SkillsShMarket::new()),
                    Box::new(super::skillhub_market::SkillhubMarket::new()),
                ],
                fallback: true,
            },
        }
    }

    pub fn providers(&self) -> &[Box<dyn SkillMarket>] {
        &self.providers
    }

    pub fn fallback_enabled(&self) -> bool {
        self.fallback
    }

    pub fn get(&self, id: &str) -> Option<&dyn SkillMarket> {
        self.providers
            .iter()
            .find(|p| p.id() == id)
            .map(|p| p.as_ref())
    }

    /// Search according to registry mode:
    /// - fallback (`auto`): first successful provider wins
    /// - otherwise: merge all providers (existing behavior)
    pub fn search_configured(&self, query: &str) -> Result<Vec<SkillListing>> {
        if self.fallback {
            self.search_with_fallback(query)
        } else {
            self.search_all(query)
        }
    }

    /// Try providers in order; return the first successful non-hard-error result.
    pub fn search_with_fallback(&self, query: &str) -> Result<Vec<SkillListing>> {
        let mut errors = Vec::new();
        for p in &self.providers {
            match p.search(query) {
                Ok(items) => return Ok(items),
                Err(e) => errors.push(format!("{}: {e}", p.id())),
            }
        }
        Err(AppError::message(
            "skill.market",
            format!(
                "market search failed (all sources): {}",
                if errors.is_empty() {
                    "no providers configured".into()
                } else {
                    errors.join("; ")
                }
            ),
        ))
    }

    pub fn search_all(&self, query: &str) -> Result<Vec<SkillListing>> {
        let mut all = Vec::new();
        let mut errors = Vec::new();
        for p in &self.providers {
            match p.search(query) {
                Ok(items) => all.extend(items),
                Err(e) => errors.push(format!("{}: {e}", p.id())),
            }
        }
        if all.is_empty() && !errors.is_empty() {
            return Err(AppError::message(
                "skill.market",
                format!("market search failed: {}", errors.join("; ")),
            ));
        }
        Ok(all)
    }
}
