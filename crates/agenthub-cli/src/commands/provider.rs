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
    confirm(
        &format!(
            "Switch {} to provider {id_or_name}? Current live config will be backfilled and backed up.",
            agent.as_str()
        ),
        assume_yes,
    )?;
    let result = hub.providers.switch(id_or_name, agent)?;
    emit_provider_switch(&result, format)
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
mod tests {
    use super::*;
    use agenthub_core::error::AppError;
    use serde_json::json;

    fn sample_provider() -> Provider {
        Provider {
            id: "p1".into(),
            agent_id: AgentId::Claude,
            name: "Relay".into(),
            settings_config: json!({
                "api_key": "sk-secret",
                "base_url": "https://example.com",
                "nested": { "auth_token": "tok", "x": 1 }
            }),
            meta: json!({"TOKEN": "t", "note": "ok"}),
            is_current: true,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-02 00:00:00".into(),
        }
    }

    #[test]
    fn parse_agent_filter_none() {
        assert_eq!(parse_agent_filter(None).unwrap(), None);
    }

    #[test]
    fn parse_agent_filter_valid() {
        assert_eq!(
            parse_agent_filter(Some("claude")).unwrap(),
            Some(AgentId::Claude)
        );
        assert_eq!(
            parse_agent_filter(Some("GROK")).unwrap(),
            Some(AgentId::Grok)
        );
        assert_eq!(
            parse_agent_filter(Some("cursor")).unwrap(),
            Some(AgentId::Cursor)
        );
        assert_eq!(
            parse_agent_filter(Some("cursor-agent")).unwrap(),
            Some(AgentId::Cursor)
        );
    }

    #[test]
    fn parse_agent_filter_invalid_is_invalid_arg() {
        let err = parse_agent_filter(Some("not-an-agent")).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        match &err {
            AppError::InvalidArg(msg) => {
                assert!(msg.contains("not-an-agent"));
                assert!(msg.contains("claude"));
                assert!(msg.contains("cursor"));
            }
            other => panic!("expected InvalidArg, got {other:?}"),
        }
    }

    #[test]
    fn select_presets_all_and_filtered() {
        let all = select_presets(None).unwrap();
        assert_eq!(all.len(), 8);

        let claude = select_presets(Some("claude")).unwrap();
        assert_eq!(claude.len(), 2);
        assert!(claude.iter().all(|p| p.agent == AgentId::Claude));
        assert!(claude.iter().all(|p| !p.template.is_empty()));
    }

    #[test]
    fn select_presets_rejects_invalid_agent() {
        assert!(matches!(
            select_presets(Some("nope")),
            Err(AppError::InvalidArg(_))
        ));
    }

    #[test]
    fn resolve_agent_filter_mirrors_parse() {
        assert_eq!(resolve_agent_filter(None).unwrap(), None);
        assert_eq!(
            resolve_agent_filter(Some("kimi")).unwrap(),
            Some(AgentId::Kimi)
        );
        assert!(matches!(
            resolve_agent_filter(Some("bad")),
            Err(AppError::InvalidArg(_))
        ));
    }

    #[test]
    fn write_operations_require_agent() {
        assert_eq!(
            require_agent(None, "switch").unwrap_err().code(),
            "invalid_arg"
        );
        assert_eq!(
            require_agent(None, "import-live").unwrap_err().code(),
            "invalid_arg"
        );
        assert_eq!(
            require_agent(Some("codex"), "switch").unwrap(),
            AgentId::Codex
        );
    }

    #[test]
    fn emit_list_and_show_quiet_is_ok() {
        let items = vec![sample_provider()];
        emit_provider_list(&items, OutputFormat::Quiet).unwrap();
        emit_provider_show(&items[0], OutputFormat::Quiet).unwrap();
    }

    #[test]
    fn emit_json_redacts_secrets_and_is_valid() {
        let p = sample_provider();
        // redacted view used by emit paths
        let r = p.redacted();
        let s = serde_json::to_string(&r).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["settingsConfig"]["api_key"], "***");
        assert_eq!(v["settingsConfig"]["base_url"], "https://example.com");
        assert_eq!(v["settingsConfig"]["nested"]["auth_token"], "***");
        assert_eq!(v["settingsConfig"]["nested"]["x"], 1);
        assert_eq!(v["meta"]["TOKEN"], "***");
        assert_eq!(v["meta"]["note"], "ok");
        assert_eq!(v["isCurrent"], true);
        // original untouched
        assert_eq!(p.settings_config["api_key"], "sk-secret");
    }

    #[test]
    fn emit_list_json_shape_via_redacted_vec() {
        let items = vec![sample_provider()];
        let redacted: Vec<Provider> = items.iter().map(Provider::redacted).collect();
        let v = serde_json::to_value(&redacted).unwrap();
        assert!(v.is_array());
        assert_eq!(v[0]["id"], "p1");
        assert_eq!(v[0]["agentId"], "claude");
        assert_eq!(v[0]["name"], "Relay");
        assert_eq!(v[0]["settingsConfig"]["api_key"], "***");
    }

    #[test]
    fn switch_result_redacts_provider_secrets() {
        let result = ProviderSwitchResult {
            provider: sample_provider(),
            backup: None,
            backfilled_provider_id: Some("old-provider".into()),
        };
        emit_provider_switch(&result, OutputFormat::Quiet).unwrap();
        let value = serde_json::to_value(result.redacted()).unwrap();
        assert_eq!(value["provider"]["settingsConfig"]["api_key"], "***");
        assert_eq!(value["provider"]["meta"]["TOKEN"], "***");
        assert_eq!(value["backfilledProviderId"], "old-provider");
    }
}
