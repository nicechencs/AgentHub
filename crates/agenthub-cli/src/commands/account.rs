//! Account CLI — thin shell over AccountService.

use std::io::{self, Read};

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{Account, AccountSwitchResult, AgentId};
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{confirm, print_json, OutputFormat};

fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

fn require_agent(agent_filter: Option<&str>, operation: &str) -> Result<AgentId> {
    parse_agent_filter(agent_filter)?.ok_or_else(|| {
        AppError::InvalidArg(format!(
            "account {operation} requires --agent <{}>",
            AgentId::expected_list()
        ))
    })
}

/// List accounts (redacted).
pub fn list(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let filter = parse_agent_filter(agent_filter)?;
    let items = hub.accounts().list(filter)?;
    emit_list(&items, format)
}

/// Import current live file credentials into the pool.
pub fn import(
    hub: &AgentHub,
    name: Option<&str>,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "import")?;
    confirm(
        &format!(
            "Import {} live credentials into the account pool?",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let item = hub.accounts().import_live(agent, name)?;
    emit_one(&item, format)
}

/// Add an API key account. `--key -` reads from stdin.
pub fn add_apikey(
    hub: &AgentHub,
    label: Option<&str>,
    key: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "add-apikey")?;
    let api_key = read_key_arg(key)?;
    confirm(
        &format!("Add API key account for {}?", agent.as_str()),
        assume_yes,
    )?;
    let item = hub.accounts().add_api_key(agent, label, &api_key)?;
    emit_one(&item, format)
}

/// Switch live credentials to a saved account.
pub fn switch(
    hub: &AgentHub,
    id_or_label: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "switch")?;
    confirm(&switch_confirm_prompt(hub, agent, id_or_label)?, assume_yes)?;
    let result = hub.accounts().switch(id_or_label, agent)?;
    emit_switch(&result, format)
}

/// Undo the last account switch for `--agent` (one-shot).
pub fn undo(
    hub: &AgentHub,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "undo")?;
    confirm(
        &format!(
            "Undo the last account switch for {}? Live credentials will be backfilled and backed up.",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let undone = hub.accounts().undo_switch(agent)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "undone": undone,
            "agent": agent.as_str(),
        })),
        OutputFormat::Table => {
            if undone {
                println!("undid last account switch for {}", agent.as_str());
            } else {
                println!("no account switch to undo for {}", agent.as_str());
            }
            Ok(())
        }
    }
}

pub fn switch_confirm_prompt(
    hub: &AgentHub,
    agent: AgentId,
    id_or_label: &str,
) -> Result<String> {
    let current = hub
        .accounts()
        .list(Some(agent))?
        .into_iter()
        .find(|a| a.is_current);
    let backfill = match current {
        Some(c) => format!("backfill: current live will be saved as 「{}」", c.label),
        None => "backfill: no current account; live will be written directly".into(),
    };
    let backup = format!(
        "backup: {}",
        hub.backups()
            .backups_root()
            .join("live")
            .join(agent.as_str())
            .display()
    );
    Ok(format!(
        "Switch {} to account {id_or_label}?\n  {backfill}\n  {backup}\n  process: running agent processes are not stopped",
        agent.as_str()
    ))
}

/// Print OAuth authorize URL for --agent (does not wait for callback).
pub fn oauth_url(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let _ = hub;
    let agent = require_agent(agent_filter, "oauth-url")?;
    if !agenthub_core::oauth::oauth_supported(agent) {
        return Err(AppError::Unsupported(format!(
            "OAuth PKCE is not configured for {}",
            agent.as_str()
        )));
    }
    // Start without opening browser — still binds loopback so the URL is valid.
    let start = agenthub_core::oauth::start_oauth(agent, false, None)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&start),
        OutputFormat::Table => {
            println!("agent:        {}", agent.as_str());
            println!("state:        {}", start.state);
            println!("redirect_uri: {}", start.redirect_uri);
            println!("authorize_url:\n{}", start.authorize_url);
            println!();
            println!(
                "This process exits after printing the URL; the loopback callback dies with it. Complete login in the GUI."
            );
            Ok(())
        }
    }
}

/// Refresh OAuth tokens for a saved account.
pub fn refresh(
    hub: &AgentHub,
    id_or_label: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "refresh")?;
    confirm(
        &format!(
            "Refresh OAuth tokens for account {id_or_label} ({})?",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let item = hub.accounts().refresh_token(id_or_label, agent)?;
    emit_one(&item, format)
}

/// Delete an account from the pool (does not modify live files).
pub fn delete(
    hub: &AgentHub,
    id_or_label: &str,
    format: OutputFormat,
    agent_filter: Option<&str>,
    assume_yes: bool,
) -> Result<()> {
    let agent = require_agent(agent_filter, "delete")?;
    confirm(
        &format!(
            "Delete account {id_or_label} for {} from the pool?",
            agent.as_str()
        ),
        assume_yes,
    )?;
    hub.accounts().delete(id_or_label, agent)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "deleted": id_or_label,
            "agent": agent.as_str(),
        })),
        OutputFormat::Table => {
            println!("deleted account {id_or_label} ({})", agent.as_str());
            Ok(())
        }
    }
}

fn read_key_arg(key: &str) -> Result<String> {
    if key == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let trimmed = buf.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::InvalidArg(
                "API key read from stdin is empty".into(),
            ));
        }
        return Ok(trimmed);
    }
    if key.trim().is_empty() {
        return Err(AppError::InvalidArg("API key must not be empty".into()));
    }
    Ok(key.to_string())
}

pub fn emit_list(items: &[Account], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => {
            let redacted: Vec<Account> = items.iter().map(Account::redacted).collect();
            print_json(&redacted)
        }
        OutputFormat::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Agent", "Id", "Kind", "Label", "Current", "Status"]);
            for a in items {
                t.add_row(vec![
                    Cell::new(a.agent_id.as_str()),
                    Cell::new(&a.id),
                    Cell::new(a.kind.as_str()),
                    Cell::new(&a.label),
                    Cell::new(if a.is_current { "yes" } else { "no" }),
                    Cell::new(&a.status),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

pub fn emit_one(item: &Account, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&item.redacted()),
        OutputFormat::Table => {
            println!(
                "{}  {}  {}  current={}  {}",
                item.agent_id.as_str(),
                item.id,
                item.kind.as_str(),
                item.is_current,
                item.label
            );
            Ok(())
        }
    }
}

pub fn emit_switch(result: &AccountSwitchResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&result.redacted()),
        OutputFormat::Table => {
            let redacted = result.redacted();
            println!(
                "switched to {} ({}) current={}",
                redacted.account.label, redacted.account.id, redacted.account.is_current
            );
            if let Some(b) = &redacted.backup {
                println!("backup: {}", b.id);
            }
            if let Some(id) = &redacted.backfilled_account_id {
                println!("backfilled: {id}");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
