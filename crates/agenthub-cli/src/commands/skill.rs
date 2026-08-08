//! `agenthub skill` — inspect and manage shared-skill projections.

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{AgentId, Skill, SkillProjectMode, SkillSyncState};
use agenthub_core::services::SkillMarketRegistry;
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::Serialize;

use crate::output::{confirm, print_json, OutputFormat};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillAction {
    skill: String,
    agent: AgentId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillFailure {
    skill: String,
    agent: AgentId,
    code: String,
    error: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillSyncReport {
    synced: Vec<SkillAction>,
    skipped: Vec<SkillAction>,
    failed: Vec<SkillFailure>,
}

pub fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

fn require_agent(agent_filter: Option<&str>) -> Result<AgentId> {
    parse_agent_filter(agent_filter)?.ok_or_else(|| {
        AppError::InvalidArg(format!(
            "skill operation requires --agent <{}>",
            AgentId::expected_list()
        ))
    })
}

pub fn list(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let filter = parse_agent_filter(agent_filter)?;
    let mut skills = hub.skills.list()?;
    if let Some(agent) = filter {
        for skill in &mut skills {
            skill.projections.retain(|item| item.agent == agent);
        }
    }
    emit_list(&skills, format)
}

pub fn sync(
    hub: &AgentHub,
    format: OutputFormat,
    agent_filter: Option<&str>,
    all: bool,
    force: bool,
    assume_yes: bool,
) -> Result<()> {
    if all && agent_filter.is_some() {
        return Err(AppError::InvalidArg(
            "skill sync accepts either --all or --agent, not both".into(),
        ));
    }
    let selected = parse_agent_filter(agent_filter)?;
    if !all && selected.is_none() {
        return Err(AppError::InvalidArg(
            "skill sync requires --agent <id> or --all".into(),
        ));
    }
    if force {
        confirm(
            "Force-sync skills? Conflicting projection directories will be replaced.",
            assume_yes,
        )?;
    }

    let targets: Vec<AgentId> = if all {
        AgentId::ALL.to_vec()
    } else {
        vec![selected.expect("validated selected agent")]
    };
    let skills = hub.skills.list()?;
    let mut report = SkillSyncReport::default();

    for skill in &skills {
        for &agent in &targets {
            let action = SkillAction {
                skill: skill.id.clone(),
                agent,
            };
            if all && skill.state_for(agent) == Some(SkillSyncState::Unsupported) {
                report.skipped.push(action);
                continue;
            }
            match hub.skills.sync(&skill.id, agent, force) {
                Ok(()) => report.synced.push(action),
                Err(error) => report.failed.push(SkillFailure {
                    skill: action.skill,
                    agent,
                    code: error.code().to_string(),
                    error: error.to_string(),
                }),
            }
        }
    }

    let failures = report.failed.len();
    emit_sync_report(&report, format)?;
    if failures == 0 {
        Ok(())
    } else {
        Err(AppError::message(
            "partial",
            format!("skill sync completed with {failures} failure(s)"),
        ))
    }
}

pub fn enable(
    hub: &AgentHub,
    skill_id: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    force: bool,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter)?;
    if force {
        confirm(
            &format!(
                "Force-enable skill {skill_id} for {}? An existing projection may be replaced.",
                agent.as_str()
            ),
            assume_yes,
        )?;
    }
    hub.skills.sync(skill_id, agent, force)?;
    emit_action("enabled", skill_id, agent, format)
}

pub fn disable(
    hub: &AgentHub,
    skill_id: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter)?;
    confirm(
        &format!(
            "Disable skill {skill_id} for {}? The shared source will be kept.",
            agent.as_str()
        ),
        assume_yes,
    )?;
    hub.skills.disable(skill_id, agent)?;
    emit_action("disabled", skill_id, agent, format)
}

pub fn list_installed(hub: &AgentHub, format: OutputFormat) -> Result<()> {
    let items = hub.skills.list_installed()?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&items),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Id", "Name", "Origin", "Root", "Projectable", "MapStatus"]);
            for s in &items {
                table.add_row(vec![
                    Cell::new(&s.id),
                    Cell::new(&s.name),
                    Cell::new(&s.origin),
                    Cell::new(&s.root_label),
                    Cell::new(if s.projectable { "yes" } else { "no" }),
                    Cell::new(s.map_status.as_str()),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

pub fn install(hub: &AgentHub, source: &str, overwrite: bool, format: OutputFormat) -> Result<()> {
    let skill = hub.skills.install_skill(source, overwrite)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Table => {
            println!(
                "installed skill {} -> {}",
                skill.id,
                skill.source_dir.display()
            );
            Ok(())
        }
    }
}

/// Copy agent-private skill into shared source; requires `--agent`.
pub fn import_private(
    hub: &AgentHub,
    skill_id: &str,
    overwrite: bool,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter)?;
    if overwrite {
        confirm(
            &format!(
                "Overwrite shared skill '{skill_id}' with private copy from {}? The private skill will be kept.",
                agent.as_str()
            ),
            assume_yes,
        )?;
    }
    let skill = hub
        .skills
        .import_private_to_shared(skill_id, agent, overwrite)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Table => {
            println!(
                "imported private skill {} from {} -> {} (private kept)",
                skill.id,
                agent.as_str(),
                skill.source_dir.display()
            );
            Ok(())
        }
    }
}

pub fn uninstall(
    hub: &AgentHub,
    skill_id: &str,
    private: bool,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    if private {
        let agent = require_agent(agent_filter)?;
        confirm(
            &format!(
                "Uninstall private skill {skill_id} from {}?",
                agent.as_str()
            ),
            assume_yes,
        )?;
        hub.skills.uninstall_private_skill(skill_id, agent)?;
    } else {
        confirm(
            &format!(
                "Uninstall skill {skill_id} from shared source and remove all agent projections?"
            ),
            assume_yes,
        )?;
        hub.skills.uninstall_skill(skill_id, Some(&hub.backups))?;
    }
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "action": "uninstalled",
            "skill": skill_id,
            "private": private,
        })),
        OutputFormat::Table => {
            println!("uninstalled skill {skill_id}");
            Ok(())
        }
    }
}

pub fn update(hub: &AgentHub, skill_id: &str, format: OutputFormat) -> Result<()> {
    let skill = hub.skills.update_skill(skill_id)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&skill),
        OutputFormat::Table => {
            println!("updated skill {}", skill.id);
            Ok(())
        }
    }
}

pub fn project(
    hub: &AgentHub,
    skill_id: &str,
    mode: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
) -> Result<()> {
    let agent = require_agent(agent_filter)?;
    let mode = SkillProjectMode::parse(mode).ok_or_else(|| {
        AppError::InvalidArg(format!(
            "invalid project mode '{mode}', expected: link|copy"
        ))
    })?;
    let result = hub.skills.project_skill(skill_id, agent, mode)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&result),
        OutputFormat::Table => {
            println!(
                "projected {} -> {} mode={} applied={} fallback={}",
                result.skill_id,
                result.agent.as_str(),
                result.requested_mode.as_str(),
                result.applied_link_kind.as_str(),
                result.fell_back
            );
            Ok(())
        }
    }
}

pub fn market(hub: &AgentHub, query: &str, format: OutputFormat) -> Result<()> {
    let source = hub
        .settings
        .load()
        .map(|s| s.skill_market_source_parsed())
        .unwrap_or_default();
    let registry = SkillMarketRegistry::from_source(source);
    let mut items = registry.search_configured(query)?;
    // Mark installed against shared source when possible.
    if let Ok(installed) = hub.skills.list() {
        let ids: std::collections::HashSet<_> = installed.iter().map(|s| s.id.as_str()).collect();
        for item in &mut items {
            let local_id = agenthub_core::services::local_skill_id_from_market_id(&item.id);
            item.installed = ids.contains(item.id.as_str()) || ids.contains(local_id.as_str());
        }
    }
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&items),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Id", "Name", "Provider", "Installed", "Version"]);
            for item in &items {
                table.add_row(vec![
                    Cell::new(&item.id),
                    Cell::new(&item.name),
                    Cell::new(&item.provider_id),
                    Cell::new(if item.installed { "yes" } else { "no" }),
                    Cell::new(item.version.as_deref().unwrap_or("-")),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

fn emit_action(action: &str, skill: &str, agent: AgentId, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "action": action,
            "skill": skill,
            "agent": agent,
        })),
        OutputFormat::Table => {
            println!("{action} skill {skill} for {}", agent.as_str());
            Ok(())
        }
    }
}

fn emit_list(skills: &[Skill], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(skills),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Skill", "Name", "Claude", "Codex", "Kimi", "Grok"]);
            for skill in skills {
                let state = |agent| {
                    skill
                        .state_for(agent)
                        .map(|value| value.as_str())
                        .unwrap_or("-")
                };
                table.add_row(vec![
                    Cell::new(&skill.id),
                    Cell::new(&skill.name),
                    Cell::new(state(AgentId::Claude)),
                    Cell::new(state(AgentId::Codex)),
                    Cell::new(state(AgentId::Kimi)),
                    Cell::new(state(AgentId::Grok)),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

fn emit_sync_report(report: &SkillSyncReport, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(report),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Result", "Skill", "Agent", "Detail"]);
            for action in &report.synced {
                table.add_row(vec![
                    Cell::new("synced"),
                    Cell::new(&action.skill),
                    Cell::new(action.agent.as_str()),
                    Cell::new("-"),
                ]);
            }
            for action in &report.skipped {
                table.add_row(vec![
                    Cell::new("skipped"),
                    Cell::new(&action.skill),
                    Cell::new(action.agent.as_str()),
                    Cell::new("unsupported"),
                ]);
            }
            for failure in &report.failed {
                table.add_row(vec![
                    Cell::new("failed"),
                    Cell::new(&failure.skill),
                    Cell::new(failure.agent.as_str()),
                    Cell::new(&failure.error),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_filter_and_requires_agent() {
        assert_eq!(parse_agent_filter(None).unwrap(), None);
        assert_eq!(
            parse_agent_filter(Some("GROK")).unwrap(),
            Some(AgentId::Grok)
        );
        assert_eq!(
            parse_agent_filter(Some("bad")).unwrap_err().code(),
            "invalid_arg"
        );
        assert_eq!(require_agent(None).unwrap_err().code(), "invalid_arg");
    }

    #[test]
    fn empty_outputs_are_valid() {
        emit_list(&[], OutputFormat::Quiet).unwrap();
        emit_sync_report(&SkillSyncReport::default(), OutputFormat::Quiet).unwrap();
        let value = serde_json::to_value(SkillSyncReport::default()).unwrap();
        assert_eq!(value["synced"], serde_json::json!([]));
        assert_eq!(value["skipped"], serde_json::json!([]));
        assert_eq!(value["failed"], serde_json::json!([]));
    }
}
