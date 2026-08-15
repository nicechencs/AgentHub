//! `agenthub run` — multi-agent parallel / sequential execution.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::{
    parse_agent_list, AgentId, MultiRunReport, RunMode, RunOptions, RunStatus,
};
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{print_json, OutputFormat};

pub struct RunArgs {
    pub prompt: String,
    pub agents: Option<String>,
    pub all: bool,
    pub global_agent: Option<String>,
    pub mode: String,
    pub timeout_secs: u64,
    pub cwd: Option<PathBuf>,
    pub dry_run: bool,
    pub allow_dangerous: bool,
}

pub fn run(hub: &AgentHub, args: RunArgs, format: OutputFormat) -> Result<()> {
    if args.allow_dangerous {
        let _ = writeln!(
            io::stderr(),
            "warning: --allow-dangerous enables per-agent auto-approve / sandbox bypass flags"
        );
    }

    let mode = RunMode::parse(&args.mode).ok_or_else(|| {
        AppError::InvalidArg(format!(
            "invalid --mode '{}', expected: parallel|sequential",
            args.mode
        ))
    })?;

    let agents = resolve_agents(&args, hub)?;
    let opts = RunOptions {
        mode,
        timeout: Duration::from_secs(args.timeout_secs),
        cwd: args.cwd,
        dry_run: args.dry_run,
        skip_missing: true,
        allow_dangerous: args.allow_dangerous,
        max_output_bytes: 2 * 1024 * 1024,
        // CLI multi-run stays human-readable text (no NDJSON process parse).
        process_mode: agenthub_core::models::ProcessMode::Text,
    };

    let report = hub.run_agents(&agents, &args.prompt, &opts)?;
    print_report(&report, format)?;

    if !report.ok {
        return Err(AppError::message(
            "run.failed",
            format!(
                "multi-run finished with {} hard failure(s)",
                report.hard_failure_count()
            ),
        ));
    }

    // All skipped and none dry-run/ok → treat as business failure.
    let any_work = report.results.iter().any(|r| {
        matches!(
            r.status,
            RunStatus::Ok
                | RunStatus::DryRun
                | RunStatus::Failed
                | RunStatus::Timeout
                | RunStatus::Cancelled
        )
    });
    if !any_work {
        return Err(AppError::NotFound(
            "no agents ran (all skipped / not installed)".into(),
        ));
    }

    Ok(())
}

pub(crate) fn resolve_agents(args: &RunArgs, hub: &AgentHub) -> Result<Vec<AgentId>> {
    let mut ids: Vec<AgentId> = Vec::new();

    if args.all {
        ids.extend_from_slice(&AgentId::ALL);
    }

    if let Some(list) = &args.agents {
        let parsed = parse_agent_list(list).map_err(AppError::InvalidArg)?;
        for id in parsed {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }

    if let Some(a) = &args.global_agent {
        let id = AgentId::parse(a).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "invalid agent id '{a}', expected: {}",
                AgentId::expected_list()
            ))
        })?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    if ids.is_empty() {
        // Default: all currently installed agents.
        let detected = hub.agents.detect_all();
        for d in detected {
            if d.status == agenthub_core::models::DetectStatus::Installed {
                ids.push(d.agent);
            }
        }
    }

    if ids.is_empty() {
        return Err(AppError::NotFound(
            "no installed agents detected; pass --agents or --all".into(),
        ));
    }

    Ok(ids)
}

fn print_report(report: &MultiRunReport, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(report),
        OutputFormat::Table => {
            println!(
                "Multi-run  mode={}  ok={}  prompt={}",
                report.mode.as_str(),
                report.ok,
                truncate(&report.prompt, 60)
            );
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec![
                "Agent",
                "Status",
                "Exit",
                "ms",
                "Preview",
                "Command / Error",
            ]);
            for r in &report.results {
                let preview = if !r.stdout.is_empty() {
                    truncate(&r.stdout, 80)
                } else if let Some(err) = &r.error {
                    truncate(err, 80)
                } else {
                    "-".into()
                };
                let cmd_or_err = if r.status == RunStatus::DryRun || !r.command.is_empty() {
                    truncate(&r.command, 100)
                } else {
                    r.error.clone().unwrap_or_else(|| "-".into())
                };
                t.add_row(vec![
                    Cell::new(r.agent.as_str()),
                    Cell::new(r.status.as_str()),
                    Cell::new(
                        r.exit_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                    Cell::new(r.duration_ms.to_string()),
                    Cell::new(preview),
                    Cell::new(cmd_or_err),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        return s;
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

#[cfg(test)]
mod tests;
