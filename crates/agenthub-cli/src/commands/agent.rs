use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{AgentId, AgentUpdateState, BackupKind};
use agenthub_core::{AgentHub, AgentKey};
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{confirm, emit_install_outcome, print_json, OutputFormat};

fn parse_agent(s: &str) -> Result<AgentId> {
    AgentId::parse_required(s)
}

fn parse_lifecycle_agent_key(value: &str) -> Result<AgentKey> {
    let trimmed = value.trim();
    match AgentKey::parse(trimmed) {
        Ok(key) => Ok(key),
        Err(original) => {
            let normalized = trimmed.to_ascii_lowercase();
            let is_legacy_builtin = AgentId::ALL
                .iter()
                .any(|agent| agent.as_str() == normalized);
            if is_legacy_builtin {
                AgentKey::parse(normalized).map_err(AppError::from)
            } else {
                Err(AppError::from(original))
            }
        }
    }
}

fn legacy_builtin_agent_id(key: &AgentKey) -> Option<AgentId> {
    AgentId::ALL
        .iter()
        .copied()
        .find(|agent| agent.as_str() == key.as_str())
}

fn update_state_label(state: AgentUpdateState) -> &'static str {
    match state {
        AgentUpdateState::UpdateAvailable => "update_available",
        AgentUpdateState::UpToDate => "up_to_date",
        AgentUpdateState::Unknown => "unknown",
        AgentUpdateState::Unsupported => "unsupported",
        AgentUpdateState::NotInstalled => "not_installed",
    }
}

fn level_abbrev(level: agenthub_core::models::CapabilityLevel) -> &'static str {
    use agenthub_core::models::CapabilityLevel::*;
    match level {
        Full => "Full",
        Partial => "Part",
        Unsupported => "Unsup",
        Planned => "Plan",
    }
}

/// Print the capability matrix (table / json / markdown).
pub fn capabilities(
    hub: &AgentHub,
    format: OutputFormat,
    agent_filter: Option<&str>,
    markdown: bool,
) -> Result<()> {
    use agenthub_core::logging::targets;
    use agenthub_core::models::Capability;
    let filter = match agent_filter {
        Some(s) => Some(parse_agent(s)?),
        None => None,
    };

    let mut matrix = hub.registry.matrix();
    if let Some(id) = filter {
        matrix.retain(|k, _| *k == id);
        if matrix.is_empty() {
            return Err(AppError::NotFound(format!(
                "adapter not registered: {}",
                id.as_str()
            )));
        }
    }

    tracing::info!(
        target: targets::CLI,
        module = targets::CLI,
        op = "agent_capabilities",
        agents = matrix.len(),
        capabilities = Capability::ALL.len(),
        markdown,
        format = ?format,
        "print capability matrix"
    );

    if markdown {
        let agents: Vec<AgentId> = matrix.keys().copied().collect();
        print!("| Capability |");
        for a in &agents {
            print!(" {} |", a.as_str());
        }
        println!();
        print!("|---|");
        for _ in &agents {
            print!("---|");
        }
        println!();
        for cap in Capability::ALL {
            print!("| {} |", cap.as_str());
            for a in &agents {
                let cell = matrix
                    .get(a)
                    .and_then(|row| row.get(&cap))
                    .map(|s| level_abbrev(s.level))
                    .unwrap_or("-");
                print!(" {cell} |");
            }
            println!();
        }
        return Ok(());
    }

    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&matrix),
        OutputFormat::Table => {
            let agents: Vec<AgentId> = matrix.keys().copied().collect();
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            let mut header = vec![Cell::new("Capability")];
            for a in &agents {
                header.push(Cell::new(a.as_str()));
            }
            t.set_header(header);
            for cap in Capability::ALL {
                let mut row = vec![Cell::new(cap.as_str())];
                for a in &agents {
                    let cell = matrix
                        .get(a)
                        .and_then(|r| r.get(&cap))
                        .map(|s| level_abbrev(s.level))
                        .unwrap_or("-");
                    row.push(Cell::new(cell));
                }
                t.add_row(row);
            }
            println!("{t}");
            Ok(())
        }
    }
}

fn print_outcome(
    outcome: &agenthub_core::models::InstallOutcome,
    format: OutputFormat,
) -> Result<()> {
    emit_install_outcome(outcome, format)
}

pub fn list(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let filter = match agent_filter {
        Some(s) => {
            let id = AgentId::parse(s).ok_or_else(|| {
                AppError::InvalidArg(format!(
                    "invalid agent id '{s}', expected: {}",
                    AgentId::expected_list()
                ))
            })?;
            Some(id)
        }
        None => None,
    };

    let mut agents = hub.agents.detect_all();
    if let Some(id) = filter {
        agents.retain(|a| a.agent == id);
    }

    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&agents),
        OutputFormat::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec![
                "Agent", "Status", "Version", "Channel", "EnvReady", "Binary",
            ]);
            for ag in &agents {
                t.add_row(vec![
                    Cell::new(ag.agent.as_str()),
                    Cell::new(format!("{:?}", ag.status)),
                    Cell::new(ag.version.as_deref().unwrap_or("-")),
                    Cell::new(ag.channel.as_deref().unwrap_or("-")),
                    Cell::new(if ag.env_ready { "yes" } else { "no" }),
                    Cell::new(
                        ag.binary_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

pub fn install(
    hub: &AgentHub,
    agent: &str,
    channel: Option<&str>,
    install_deps: bool,
    format: OutputFormat,
) -> Result<()> {
    let key = parse_lifecycle_agent_key(agent)?;
    let channel = channel.unwrap_or("");
    let outcome = hub.install_agent_key(&key, channel, install_deps)?;
    print_outcome(&outcome, format)
}

pub fn upgrade(hub: &AgentHub, agent: &str, format: OutputFormat) -> Result<()> {
    let key = parse_lifecycle_agent_key(agent)?;
    let outcome = hub.upgrade_agent_key(&key)?;
    print_outcome(&outcome, format)
}

/// Probe remote latest (npm dist-tags, disk-cached). Optional single-agent filter.
pub fn outdated(
    hub: &AgentHub,
    agent: Option<&str>,
    force: bool,
    format: OutputFormat,
) -> Result<()> {
    let filter = match agent {
        Some(s) => Some(vec![parse_agent(s)?]),
        None => None,
    };
    let rows = hub.check_agent_updates(filter.as_deref(), force)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&rows),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec![
                Cell::new("agent"),
                Cell::new("state"),
                Cell::new("current"),
                Cell::new("latest"),
                Cell::new("source"),
                Cell::new("note"),
            ]);
            for r in &rows {
                table.add_row(vec![
                    Cell::new(r.agent_id.as_str()),
                    Cell::new(update_state_label(r.state)),
                    Cell::new(r.current_version.as_deref().unwrap_or("-")),
                    Cell::new(r.latest_version.as_deref().unwrap_or("-")),
                    Cell::new(r.source.as_deref().unwrap_or("-")),
                    Cell::new(r.note.as_deref().unwrap_or("")),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

pub fn uninstall(
    hub: &AgentHub,
    agent: &str,
    purge_config: bool,
    yes: bool,
    format: OutputFormat,
) -> Result<()> {
    let key = parse_lifecycle_agent_key(agent)?;
    if purge_config {
        confirm(
            &format!(
                "Uninstall {} and delete its config directory? Shared runtimes (Node/npm/git) are kept.",
                key.as_str()
            ),
            yes,
        )?;
    }
    // Best-effort pre-uninstall backup when purging config.
    if purge_config {
        if let Some(agent) = legacy_builtin_agent_id(&key) {
            match hub
                .backups
                .snapshot(agent, BackupKind::PreUninstall, Some("pre-uninstall"))
            {
                Ok(rec) => eprintln!("pre-uninstall backup: {}", rec.id),
                Err(e) => eprintln!("pre-uninstall backup skipped: {e}"),
            }
        }
    }
    let outcome = hub.uninstall_agent_key(&key, purge_config)?;
    print_outcome(&outcome, format)
}

#[cfg(test)]
mod tests;
