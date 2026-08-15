use agenthub_core::error::{AppError, Result};
use agenthub_core::models::RuntimeId;
use agenthub_core::AgentHub;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use crate::output::{emit_install_outcome, print_json, OutputFormat};

pub fn list(hub: &AgentHub, format: OutputFormat) -> Result<()> {
    let runtimes = hub.env.detect_all();
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&runtimes),
        OutputFormat::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(vec![
                "Id",
                "Status",
                "Version",
                "Min",
                "Path",
                "Remediation",
            ]);
            for rt in &runtimes {
                let rem = rt
                    .remediation
                    .as_ref()
                    .and_then(|r| {
                        r.command
                            .clone()
                            .or_else(|| r.text.clone())
                            .or_else(|| r.url.clone())
                    })
                    .unwrap_or_else(|| "-".into());
                t.add_row(vec![
                    Cell::new(rt.id.as_str()),
                    Cell::new(format!("{:?}", rt.status)),
                    Cell::new(rt.version.as_deref().unwrap_or("-")),
                    Cell::new(rt.min_required.as_deref().unwrap_or("-")),
                    Cell::new(
                        rt.path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "-".into()),
                    ),
                    Cell::new(rem),
                ]);
            }
            println!("{t}");
            Ok(())
        }
    }
}

pub fn install(hub: &AgentHub, runtime: &str, channel: &str, format: OutputFormat) -> Result<()> {
    let id = RuntimeId::parse(runtime).ok_or_else(|| {
        AppError::InvalidArg(format!(
            "invalid runtime '{runtime}', expected: nodejs|npm|powershell|git"
        ))
    })?;
    let outcome = hub.install_runtime(id, channel)?;
    emit_install_outcome(&outcome, format)
}

#[cfg(test)]
mod tests;
