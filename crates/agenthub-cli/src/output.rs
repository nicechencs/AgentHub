use std::io::{self, IsTerminal, Write};

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::InstallOutcome;
use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Quiet,
}

pub fn print_json<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(value)?;
    println!("{s}");
    Ok(())
}

pub fn print_error(err: &AppError, format: OutputFormat) {
    if let Some(line) = render_error(err, format) {
        let _ = writeln!(io::stderr(), "{line}");
    }
}

fn render_error(err: &AppError, format: OutputFormat) -> Option<String> {
    match format {
        OutputFormat::Json => Some(
            serde_json::json!({
                "error": format!("operation failed ({})", err.code()),
                "code": err.code(),
                "details": err.details()
            })
            .to_string(),
        ),
        OutputFormat::Quiet => None,
        // Table is user-facing stderr: mask secrets the same way as file logs.
        OutputFormat::Table => {
            let msg = agenthub_core::utils::redact::redact_text(&err.to_string());
            Some(format!("error: {msg} [{}]", err.code()))
        }
    }
}

/// Map an install-family outcome to CLI success / structured failure.
pub fn emit_install_outcome(outcome: &InstallOutcome, format: OutputFormat) -> Result<()> {
    if format != OutputFormat::Quiet {
        for line in &outcome.logs {
            eprintln!("{line}");
        }
    }
    match format {
        OutputFormat::Quiet => {}
        OutputFormat::Json => print_json(outcome)?,
        OutputFormat::Table => {
            println!(
                "{} — {}",
                if outcome.ok { "OK" } else { "FAILED" },
                outcome.message
            );
        }
    }
    if outcome.ok {
        Ok(())
    } else {
        Err(map_install_failure(outcome))
    }
}

pub fn map_install_failure(outcome: &InstallOutcome) -> AppError {
    match outcome.code.as_deref() {
        Some("env.not_ready") => {
            let payload = outcome
                .details
                .clone()
                .unwrap_or_else(|| serde_json::json!({ "message": outcome.message }));
            AppError::EnvNotReady(payload.to_string())
        }
        Some("unsupported") => AppError::Unsupported(outcome.message.clone()),
        _ => AppError::message("install.failed", outcome.message.clone()),
    }
}

/// Confirm a destructive CLI action. Non-interactive callers must pass
/// `--yes`, so CI never blocks waiting for stdin.
pub fn confirm(prompt: &str, assume_yes: bool) -> Result<()> {
    if assume_yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(AppError::message(
            "confirmation_required",
            "confirmation requires an interactive terminal; pass --yes to continue",
        ));
    }

    eprint!("{prompt} [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(AppError::message("cancelled", "operation cancelled"))
    }
}

#[cfg(test)]
mod tests;
