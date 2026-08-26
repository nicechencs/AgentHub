//! `agenthub backup` — list, create, restore, and delete live snapshots.

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{AgentId, BackupKind, BackupRecord};
use agenthub_core::services::RestoreResult;
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{confirm, print_json, OutputFormat};

pub fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

fn require_agent(agent_filter: Option<&str>) -> Result<AgentId> {
    parse_agent_filter(agent_filter)?.ok_or_else(|| {
        AppError::InvalidArg(format!(
            "backup create requires --agent <{}>",
            AgentId::expected_list()
        ))
    })
}

pub fn list(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let items = hub.backups().list(parse_agent_filter(agent_filter)?)?;
    emit_list(&items, format)
}

pub fn create(
    hub: &AgentHub,
    format: OutputFormat,
    agent_filter: Option<&str>,
    note: Option<&str>,
) -> Result<()> {
    let record = hub
        .backups()
        .snapshot(require_agent(agent_filter)?, BackupKind::Manual, note)?;
    emit_record(&record, format)
}

pub fn restore(hub: &AgentHub, id: &str, format: OutputFormat, assume_yes: bool) -> Result<()> {
    confirm(
        &format!("Restore backup {id}? Current live files will be backed up first."),
        assume_yes,
    )?;
    let result = hub.backups().restore(id)?;
    emit_restore(&result, format)
}

pub fn delete(hub: &AgentHub, id: &str, format: OutputFormat, assume_yes: bool) -> Result<()> {
    confirm(
        &format!("Delete backup {id}? This removes its snapshot files."),
        assume_yes,
    )?;
    hub.backups().delete(id)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({ "deleted": id })),
        OutputFormat::Table => {
            println!("Deleted backup {id}");
            Ok(())
        }
    }
}

pub fn emit_list(items: &[BackupRecord], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&items),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec![
                "Agent", "Id", "Kind", "Files", "Bytes", "Note", "Created",
            ]);
            for item in items {
                table.add_row(vec![
                    Cell::new(item.agent_id.map(|id| id.as_str()).unwrap_or("-")),
                    Cell::new(&item.id),
                    Cell::new(item.kind.as_str()),
                    Cell::new(item.files.len()),
                    Cell::new(item.size),
                    Cell::new(item.note.as_deref().unwrap_or("-")),
                    Cell::new(&item.created_at),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

fn emit_record(record: &BackupRecord, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(record),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Field", "Value"]);
            table.add_row(vec![Cell::new("id"), Cell::new(&record.id)]);
            table.add_row(vec![
                Cell::new("agent"),
                Cell::new(record.agent_id.map(|id| id.as_str()).unwrap_or("-")),
            ]);
            table.add_row(vec![Cell::new("kind"), Cell::new(record.kind.as_str())]);
            table.add_row(vec![Cell::new("files"), Cell::new(record.files.len())]);
            table.add_row(vec![Cell::new("bytes"), Cell::new(record.size)]);
            table.add_row(vec![Cell::new("path"), Cell::new(&record.path)]);
            println!("{table}");
            Ok(())
        }
    }
}

fn emit_restore(result: &RestoreResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(result),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Field", "Value"]);
            table.add_row(vec![Cell::new("restored"), Cell::new(&result.restored.id)]);
            table.add_row(vec![
                Cell::new("agent"),
                Cell::new(
                    result
                        .restored
                        .agent_id
                        .map(|id| id.as_str())
                        .unwrap_or("-"),
                ),
            ]);
            table.add_row(vec![
                Cell::new("restored_files"),
                Cell::new(result.restored_paths.len()),
            ]);
            table.add_row(vec![
                Cell::new("pre_restore_backup"),
                Cell::new(
                    result
                        .pre_restore
                        .as_ref()
                        .map(|backup| backup.id.as_str())
                        .unwrap_or("-"),
                ),
            ]);
            println!("{table}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
