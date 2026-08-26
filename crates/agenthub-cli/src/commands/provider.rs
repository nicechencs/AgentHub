//! `agenthub provider` — pool reads, live import, safe switching, and presets.

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{AgentId, Provider, ProviderPreset, ProviderSwitchResult};
use agenthub_core::presets;
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{confirm, print_json, OutputFormat};

/// Parse optional global `-a/--agent` filter into [`AgentId`].
///
/// Invalid values become [`AppError::InvalidArg`] (CLI exit code 2).
pub fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

/// Select presets for CLI output (thin pure wrapper over core registry).
pub fn select_presets(agent_filter: Option<&str>) -> Result<Vec<ProviderPreset>> {
    let filter = parse_agent_filter(agent_filter)?;
    Ok(presets::list(filter))
}

/// Resolve agent filter for list/show (pure; no DB).
pub fn resolve_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    parse_agent_filter(agent_filter)
}

fn require_agent(agent_filter: Option<&str>, operation: &str) -> Result<AgentId> {
    parse_agent_filter(agent_filter)?.ok_or_else(|| {
        AppError::InvalidArg(format!(
            "provider {operation} requires --agent <{}>",
            AgentId::expected_list()
        ))
    })
}

pub fn presets(format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let items = select_presets(agent_filter)?;

    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&items),
        OutputFormat::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Agent", "Id", "Label", "Format"]);
            for p in &items {
                t.add_row(vec![
                    Cell::new(p.agent.as_str()),
                    Cell::new(&p.id),
                    Cell::new(&p.label),
                    Cell::new(p.format.as_str()),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

/// List persisted L1 providers (redacted JSON; table: agent/id/name/current).
pub fn list(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let filter = resolve_agent_filter(agent_filter)?;
    let items = hub.providers.list(filter)?;
    emit_provider_list(&items, format)
}

/// Show one provider by id or unambiguous name (redacted JSON).
pub fn show(
    hub: &AgentHub,
    id_or_name: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
) -> Result<()> {
    let filter = resolve_agent_filter(agent_filter)?;
    let item = hub.providers.get(id_or_name, filter)?;
    emit_provider_show(&item, format)
}

/// Capture an agent's current complete live config as a current provider row.
pub fn import_live(
    hub: &AgentHub,
    name: Option<&str>,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "import-live")?;
    confirm(
        &format!(
            "Import {} live config into the provider pool?",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let item = hub.providers.import_live(agent, name)?;
    emit_provider_show(&item, format)
}

/// Backfill the current row, snapshot live config, then apply the selected row.
pub fn switch(
    hub: &AgentHub,
    id_or_name: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "switch")?;
    confirm(&switch_confirm_prompt(hub, agent, id_or_name)?, assume_yes)?;
    let result = hub.providers.switch(id_or_name, agent)?;
    emit_provider_switch(&result, format)
}

/// Undo the last provider switch for `--agent` (one-shot).
pub fn undo(
    hub: &AgentHub,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "undo")?;
    confirm(
        &format!(
            "Undo the last provider switch for {}? Live config will be backfilled and backed up.",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let undone = hub.providers.undo_switch(agent)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "undone": undone,
            "agent": agent.as_str(),
        })),
        OutputFormat::Table => {
            if undone {
                println!("undid last provider switch for {}", agent.as_str());
            } else {
                println!("no provider switch to undo for {}", agent.as_str());
            }
            Ok(())
        }
    }
}

/// Probe a saved provider Base URL RTT in milliseconds.
pub fn test_latency(
    hub: &AgentHub,
    id_or_name: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
) -> Result<()> {
    let agent = require_agent(agent_filter, "test-latency")?;
    let ms = hub.providers.test_latency(agent, id_or_name)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "agent": agent.as_str(),
            "provider": id_or_name,
            "latencyMs": ms,
        })),
        OutputFormat::Table => {
            println!("{} {}  {ms} ms", agent.as_str(), id_or_name);
            Ok(())
        }
    }
}

pub fn switch_confirm_prompt(
    hub: &AgentHub,
    agent: AgentId,
    id_or_name: &str,
) -> Result<String> {
    let current = hub
        .providers
        .list(Some(agent))?
        .into_iter()
        .find(|p| p.is_current);
    let backfill = match current {
        Some(c) => format!("backfill: current live will be saved as 「{}」", c.name),
        None => "backfill: no current provider; live will be written directly".into(),
    };
    let backup = format!(
        "backup: {}",
        hub.backups
            .backups_root()
            .join("live")
            .join(agent.as_str())
            .display()
    );
    Ok(format!(
        "Switch {} to provider {id_or_name}?\n  {backfill}\n  {backup}\n  process: running agent processes are not stopped",
        agent.as_str()
    ))
}

/// Pure presentation for list (testable without DB).
pub fn emit_provider_list(items: &[Provider], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => {
            let redacted: Vec<Provider> = items.iter().map(Provider::redacted).collect();
            print_json(&redacted)
        }
        OutputFormat::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Agent", "Id", "Name", "Current"]);
            for p in items {
                t.add_row(vec![
                    Cell::new(p.agent_id.as_str()),
                    Cell::new(&p.id),
                    Cell::new(&p.name),
                    Cell::new(if p.is_current { "yes" } else { "no" }),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

/// Pure presentation for show (testable without DB).
pub fn emit_provider_show(item: &Provider, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&item.redacted()),
        OutputFormat::Table => {
            let r = item.redacted();
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Field", "Value"]);
            t.add_row(vec![Cell::new("agent"), Cell::new(r.agent_id.as_str())]);
            t.add_row(vec![Cell::new("id"), Cell::new(&r.id)]);
            t.add_row(vec![Cell::new("name"), Cell::new(&r.name)]);
            t.add_row(vec![
                Cell::new("current"),
                Cell::new(if r.is_current { "yes" } else { "no" }),
            ]);
            t.add_row(vec![
                Cell::new("settings_config"),
                Cell::new(serde_json::to_string_pretty(&r.settings_config)?),
            ]);
            t.add_row(vec![
                Cell::new("meta"),
                Cell::new(serde_json::to_string_pretty(&r.meta)?),
            ]);
            t.add_row(vec![Cell::new("created_at"), Cell::new(&r.created_at)]);
            t.add_row(vec![Cell::new("updated_at"), Cell::new(&r.updated_at)]);
            println!("{t}");
            Ok(())
        }
    }
}

/// Present a switch result without leaking provider credentials.
pub fn emit_provider_switch(result: &ProviderSwitchResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&result.redacted()),
        OutputFormat::Table => {
            let redacted = result.redacted();
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Field", "Value"]);
            table.add_row(vec![
                Cell::new("agent"),
                Cell::new(redacted.provider.agent_id.as_str()),
            ]);
            table.add_row(vec![
                Cell::new("provider"),
                Cell::new(&redacted.provider.name),
            ]);
            table.add_row(vec![
                Cell::new("provider_id"),
                Cell::new(&redacted.provider.id),
            ]);
            table.add_row(vec![
                Cell::new("backup_id"),
                Cell::new(
                    redacted
                        .backup
                        .as_ref()
                        .map(|backup| backup.id.as_str())
                        .unwrap_or("-"),
                ),
            ]);
            table.add_row(vec![
                Cell::new("backfilled_provider_id"),
                Cell::new(redacted.backfilled_provider_id.as_deref().unwrap_or("-")),
            ]);
            println!("{table}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
