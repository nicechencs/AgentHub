//! Skill projection Tauri commands — thin wrappers over agenthub-core.

use agenthub_core::models::{
    AgentId, InstalledSkill, Skill, SkillListing, SkillMarkdownPreview, SkillProjectMode,
    SkillProjectResult,
};
use agenthub_core::AgentHub;
use tauri::State;

use agenthub_core::logging::targets;

use crate::commands::{map_err_string, parse_agent, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

pub use agenthub_core::models::SkillSyncReport;

/// Invoke: `list_skills`
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| list_skills_inner(hub)).await
}

/// Invoke: `sync_skill` — project one skill to one agent (`force` replaces conflicts).
#[tauri::command]
pub async fn sync_skill(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    force: Option<bool>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    let force = force.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        sync_skill_inner(hub, &skill_id, &agent_id, force)
    })
    .await
}

/// Invoke: `disable_skill` — remove projection; source kept.
#[tauri::command]
pub async fn disable_skill(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        disable_skill_inner(hub, &skill_id, &agent_id)
    })
    .await
}

/// Invoke: `sync_all_skills` — sync every skill for one agent or all agents.
#[tauri::command]
pub async fn sync_all_skills(
    state: State<'_, AppState>,
    agent_id: Option<String>,
    force: Option<bool>,
) -> Result<SkillSyncReport, String> {
    let hub = state.hub_arc()?;
    let force = force.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        sync_all_skills_inner(hub, agent_id.as_deref(), force)
    })
    .await
}

fn list_skills_inner(hub: &AgentHub) -> Result<Vec<Skill>, String> {
    hub.skills
        .list()
        .map_err(|e| map_err_string("list_skills", e))
}

/// Invoke: `list_installed_skills`
#[tauri::command]
pub async fn list_installed_skills(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledSkill>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| {
        hub.skills
            .list_installed()
            .map_err(|e| map_err_string("list_installed_skills", e))
    })
    .await
}

/// Invoke: `list_skill_catalog`
#[tauri::command]
pub async fn list_skill_catalog(state: State<'_, AppState>) -> Result<Vec<InstalledSkill>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| list_skill_catalog_inner(hub)).await
}

fn list_skill_catalog_inner(hub: &AgentHub) -> Result<Vec<InstalledSkill>, String> {
    hub.skills
        .list_catalog()
        .map_err(|e| map_err_string("list_skill_catalog", e))
}

/// Invoke: `read_skill_markdown` — load local `SKILL.md` for GUI preview.
///
/// Pass `private_agent` for agent-private skills; omit for shared library.
#[tauri::command]
pub async fn read_skill_markdown(
    state: State<'_, AppState>,
    skill_id: String,
    private_agent: Option<String>,
) -> Result<SkillMarkdownPreview, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        read_skill_markdown_inner(hub, &skill_id, private_agent.as_deref())
    })
    .await
}

fn read_skill_markdown_inner(
    hub: &AgentHub,
    skill_id: &str,
    private_agent: Option<&str>,
) -> Result<SkillMarkdownPreview, String> {
    let agent = parse_agent_opt(private_agent)?;
    hub.skills
        .read_skill_markdown(skill_id, agent)
        .map_err(|e| map_err_string("read_skill_markdown", e))
}

/// Invoke: `install_skill`
#[tauri::command]
pub async fn install_skill(
    state: State<'_, AppState>,
    source: String,
    overwrite: Option<bool>,
) -> Result<Skill, String> {
    let hub = state.hub_arc()?;
    let overwrite = overwrite.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        hub.skills
            .install_skill(&source, overwrite)
            .map_err(|e| map_err_string("install_skill", e))
    })
    .await
}

/// Invoke: `import_private_skill` — copy agent-private skill into shared source.
#[tauri::command]
pub async fn import_private_skill(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    overwrite: Option<bool>,
) -> Result<Skill, String> {
    let hub = state.hub_arc()?;
    let overwrite = overwrite.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent(&agent_id)?;
        hub.skills
            .import_private_to_shared(&skill_id, agent, overwrite)
            .map_err(|e| map_err_string("import_private_skill", e))
    })
    .await
}

/// Invoke: `uninstall_skill`
#[tauri::command]
pub async fn uninstall_skill(
    state: State<'_, AppState>,
    skill_id: String,
    private_agent: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        if let Some(agent_id) = private_agent {
            let agent = parse_agent(&agent_id)?;
            hub.skills
                .uninstall_private_skill(&skill_id, agent)
                .map_err(|e| map_err_string("uninstall_skill", e))
        } else {
            hub.skills
                .uninstall_skill(&skill_id, Some(&hub.backups))
                .map_err(|e| map_err_string("uninstall_skill", e))
        }
    })
    .await
}

/// Invoke: `update_skill`
#[tauri::command]
pub async fn update_skill(state: State<'_, AppState>, skill_id: String) -> Result<Skill, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.skills
            .update_skill(&skill_id)
            .map_err(|e| map_err_string("update_skill", e))
    })
    .await
}

/// Invoke: `project_skill`
#[tauri::command]
pub async fn project_skill(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    mode: Option<String>,
) -> Result<SkillProjectResult, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent(&agent_id)?;
        let mode = match mode.as_deref() {
            None | Some("") | Some("link") => SkillProjectMode::Link,
            Some("copy") => SkillProjectMode::Copy,
            Some(other) => {
                let msg = format!("invalid project mode '{other}', expected: link|copy");
                tracing::warn!(target: targets::GUI, op = "project_skill", "{msg}");
                return Err(msg);
            }
        };
        hub.skills
            .project_skill(&skill_id, agent, mode)
            .map_err(|e| map_err_string("project_skill", e))
    })
    .await
}

/// Invoke: `search_skill_market` — skills.sh / skillhub.cn (settings-driven, auto fallback).
#[tauri::command]
pub async fn search_skill_market(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<SkillListing>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.search_skill_market(query.as_deref().unwrap_or(""))
            .map_err(|e| map_err_string("search_skill_market", e))
    })
    .await
}

/// Invoke: `install_market_skill` — fetch from market provider and install into shared library.
#[tauri::command]
pub async fn install_market_skill(
    state: State<'_, AppState>,
    skill_id: String,
    overwrite: Option<bool>,
) -> Result<Skill, String> {
    let hub = state.hub_arc()?;
    let overwrite = overwrite.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        hub.install_market_listing(&skill_id, overwrite)
            .map_err(|e| map_err_string("install_market_skill", e))
    })
    .await
}

fn sync_skill_inner(
    hub: &AgentHub,
    skill_id: &str,
    agent_id: &str,
    force: bool,
) -> Result<(), String> {
    let agent = parse_agent(agent_id)?;
    hub.skills
        .sync(skill_id, agent, force)
        .map_err(|e| map_err_string("sync_skill", e))
}

fn disable_skill_inner(hub: &AgentHub, skill_id: &str, agent_id: &str) -> Result<(), String> {
    let agent = parse_agent(agent_id)?;
    hub.skills
        .disable(skill_id, agent)
        .map_err(|e| map_err_string("disable_skill", e))
}

fn sync_all_skills_inner(
    hub: &AgentHub,
    agent_id: Option<&str>,
    force: bool,
) -> Result<SkillSyncReport, String> {
    let selected = parse_agent_opt(agent_id)?;
    let (targets, skip_unsupported) = match selected {
        Some(a) => (vec![a], false),
        None => (AgentId::ALL.to_vec(), true),
    };
    let report = hub
        .skills
        .sync_targets(&targets, force, skip_unsupported)
        .map_err(|e| map_err_string("sync_all_skills", e))?;
    for failure in &report.failed {
        tracing::warn!(
            target: targets::GUI,
            op = "sync_all_skills",
            skill = %failure.skill,
            agent = %failure.agent.as_str(),
            code = %failure.code,
            error = %failure.error,
            "skill sync failed"
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
