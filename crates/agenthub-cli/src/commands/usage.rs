//! `agenthub usage` — collect / stats / models / health.

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{AgentId, CollectResult, ParserHealth, UsageQuery, UsageRecord};
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{print_json, OutputFormat};

fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

/// Incremental collect from agent session logs.
pub fn collect(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let filter = parse_agent_filter(agent_filter)?;
    let result = hub.usage.collect(filter)?;
    emit_collect(&result, format)
}

/// Aggregate stats over usage rows (input/output/cache/cost).
pub fn stats(
    hub: &AgentHub,
    days: u32,
    format: OutputFormat,
    agent_filter: Option<&str>,
    model: Option<&str>,
) -> Result<()> {
    let agent = parse_agent_filter(agent_filter)?;
    let model = model
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "all")
        .map(|s| s.to_string());
    let rows = hub.usage.query(UsageQuery {
        days: days.max(1),
        agent_id: agent,
        model,
        ..Default::default()
    })?;
    emit_stats(&rows, days, agent, format)
}

/// Distinct model names from usage table (not an official catalog).
pub fn models(hub: &AgentHub, format: OutputFormat, agent_filter: Option<&str>) -> Result<()> {
    let agent = parse_agent_filter(agent_filter)?;
    let mut list = hub.usage.list_models()?;
    if let Some(a) = agent {
        // Filter models that appear for this agent in recent wide window.
        let rows = hub.usage.query(UsageQuery {
            days: 365,
            agent_id: Some(a),
            model: None,
            ..Default::default()
        })?;
        let set: std::collections::BTreeSet<_> = rows.into_iter().map(|r| r.model).collect();
        list.retain(|m| set.contains(m));
    }
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&serde_json::json!({
            "models": list,
            "note": "deduped from usage_records; not an official model catalog",
        })),
        OutputFormat::Table => {
            if list.is_empty() {
                println!("(no models in usage_records yet — run `agenthub usage collect`)");
            } else {
                for m in list {
                    println!("{m}");
                }
            }
            Ok(())
        }
    }
}

/// Parser health per agent.
pub fn health(hub: &AgentHub, format: OutputFormat) -> Result<()> {
    let rows = hub.usage.parser_health()?;
    emit_health(&rows, format)
}

fn emit_collect(result: &CollectResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(result),
        OutputFormat::Table => {
            println!(
                "collect: inserted={} skipped={} failed={}",
                result.inserted, result.skipped, result.failed
            );
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Agent", "Supported", "Records", "Fail%", "Skipped"]);
            for h in &result.agents {
                table.add_row(vec![
                    Cell::new(h.agent_id.as_str()),
                    Cell::new(if h.supported { "yes" } else { "no" }),
                    Cell::new(h.records),
                    Cell::new(
                        h.fail_rate_pct
                            .map(|p| format!("{p}"))
                            .unwrap_or_else(|| "-".into()),
                    ),
                    Cell::new(
                        h.skipped
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                ]);
            }
            println!("{table}");
            print_missing_pricing_warnings(&result.missing_pricing_models);
            Ok(())
        }
    }
}

fn emit_stats(
    rows: &[UsageRecord],
    days: u32,
    agent: Option<AgentId>,
    format: OutputFormat,
) -> Result<()> {
    let mut input = 0i64;
    let mut output = 0i64;
    let mut cache_read = 0i64;
    let mut cache_write = 0i64;
    let mut cost = 0.0f64;
    for r in rows {
        // Storage is already ccusage layout: input = non-cached billable for all agents
        // (Codex peeled at parse time). Do not subtract cache again.
        input += r.input_tokens.max(0);
        output += r.output_tokens.max(0);
        cache_read += r.cache_read_tokens.max(0);
        cache_write += r.cache_write_tokens.max(0);
        cost += r.cost_usd.unwrap_or(0.0);
    }
    let missing = missing_pricing_from_rows(rows);
    let summary = serde_json::json!({
        "days": days,
        "agent": agent.map(|a| a.as_str()),
        "rows": rows.len(),
        "inputTokens": input,
        "outputTokens": output,
        "cacheReadTokens": cache_read,
        "cacheWriteTokens": cache_write,
        "costUsd": (cost * 100.0).round() / 100.0,
        "missingPricingModels": missing,
    });
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&summary),
        OutputFormat::Table => {
            let scope = agent.map(|a| a.as_str()).unwrap_or("all");
            println!("usage stats (last {days}d, agent={scope})");
            println!("  rows:          {}", rows.len());
            println!("  input tokens:  {input}  (non-cached / billable)");
            println!("  output tokens: {output}");
            println!("  cache write:   {cache_write}");
            println!("  cache read:    {cache_read}");
            println!("  est. cost USD: {:.2}", cost);
            if rows.is_empty() {
                println!("  tip: run `agenthub usage collect` first");
            }
            print_missing_pricing_warnings(&missing);
            Ok(())
        }
    }
}

fn emit_health(rows: &[ParserHealth], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(rows),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Agent", "Supported", "Records", "Fail%", "Skipped"]);
            for h in rows {
                table.add_row(vec![
                    Cell::new(h.agent_id.as_str()),
                    Cell::new(if h.supported { "yes" } else { "no" }),
                    Cell::new(h.records),
                    Cell::new(
                        h.fail_rate_pct
                            .map(|p| format!("{p}"))
                            .unwrap_or_else(|| "-".into()),
                    ),
                    Cell::new(
                        h.skipped
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

/// Parse --days for stats (allowed 7/14/30 or any positive).
pub fn parse_days(raw: u32) -> Result<u32> {
    if raw == 0 {
        return Err(AppError::InvalidArg(
            "usage stats --days must be >= 1".into(),
        ));
    }
    Ok(raw)
}

fn missing_pricing_from_rows(rows: &[UsageRecord]) -> Vec<String> {
    use agenthub_core::usage::has_embedded_pricing;
    let mut set = std::collections::BTreeSet::new();
    for r in rows {
        let tokens = r.input_tokens + r.output_tokens + r.cache_tokens_total();
        if tokens > 0 && !has_embedded_pricing(&r.model) {
            set.insert(r.model.clone());
        }
    }
    set.into_iter().collect()
}

/// ccusage-style missing pricing warnings on stderr (table mode).
fn print_missing_pricing_warnings(models: &[String]) {
    for m in models {
        eprintln!(
            "WARN  Missing embedded pricing for {m}; cost recorded as $0. Run `pnpm pricing:update` or add scripts/pricing/overrides.json."
        );
    }
}

#[cfg(test)]
mod tests;
