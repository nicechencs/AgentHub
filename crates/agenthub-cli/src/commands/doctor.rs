use agenthub_core::error::Result;
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{print_json, OutputFormat};

pub fn run(hub: &AgentHub, format: OutputFormat) -> Result<()> {
    let report = hub.doctor();
    let ok = report.ok;

    match format {
        OutputFormat::Quiet => {}
        OutputFormat::Json => print_json(&report)?,
        OutputFormat::Table => {
            println!("AgentHub doctor  v{}", report.version);
            println!();

            // ① Runtimes
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Runtime", "Status", "Version", "Path"]);
            for rt in &report.runtimes {
                t.add_row(vec![
                    Cell::new(rt.id.as_str()),
                    Cell::new(format!("{:?}", rt.status)),
                    Cell::new(rt.version.as_deref().unwrap_or("-")),
                    Cell::new(
                        rt.path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                ]);
            }
            println!("① Runtimes");
            println!("{t}");
            for rt in &report.runtimes {
                if !rt.notes.is_empty() {
                    println!("  notes [{}]:", rt.id.as_str());
                    for n in &rt.notes {
                        println!("    - {n}");
                    }
                }
            }
            println!();

            // ② Agents
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec![
                "Agent", "Status", "Version", "Channel", "EnvReady", "Binary",
            ]);
            for ag in &report.agents {
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
            println!("② Agents");
            println!("{t}");
            println!();

            // ③ Paths / DB
            println!("③ Paths / DB");
            println!("  data_dir   : {}", report.paths.data_dir);
            println!("  db_path    : {}", report.paths.db_path);
            println!("  backups_dir: {}", report.paths.backups_dir);
            println!("  logs_dir   : {}", report.paths.logs_dir);
            println!("  db_ok      : {}", report.db_ok);
            println!();

            // ④ Usage parsers
            println!("④ Usage parsers");
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec!["Agent", "Supported", "Records", "Fail%", "Skipped"]);
            for h in &report.usage_health {
                t.add_row(vec![
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
            println!("{t}");
            println!();

            // ⑤ Locks
            println!("⑤ Locks");
            if report.locks.is_empty() {
                println!("  (no live-write locks held)");
            } else {
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(vec!["Agent", "Status", "PID", "Note"]);
                for lock in &report.locks {
                    t.add_row(vec![
                        Cell::new(&lock.agent),
                        Cell::new(&lock.status),
                        Cell::new(
                            lock.pid
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "-".into()),
                        ),
                        Cell::new(lock.note.as_deref().unwrap_or("-")),
                    ]);
                }
                println!("{t}");
            }
            println!();

            if !report.warnings.is_empty() {
                println!("Warnings:");
                for w in &report.warnings {
                    println!("  - {w}");
                }
            }

            if ok {
                println!("Overall: OK (warnings do not fail doctor)");
            } else {
                println!("Overall: FAIL");
            }
        }
    }

    doctor_result(ok)
}

#[cfg(test)]
mod tests;

/// Contract: warnings stay exit 0; hard failures (e.g. db) exit 1.
pub(crate) fn doctor_result(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(agenthub_core::error::AppError::message(
            "doctor.failed",
            "hard checks failed (see report)",
        ))
    }
}
