use std::io::{self, IsTerminal, Write};

use agenthub_core::error::AppError;
use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Quiet,
}

pub fn print_json<T: Serialize + ?Sized>(value: &T) -> Result<(), AppError> {
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
                "details": {}
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

/// Confirm a destructive CLI action. Non-interactive callers must pass
/// `--yes`, so CI never blocks waiting for stdin.
pub fn confirm(prompt: &str, assume_yes: bool) -> Result<(), AppError> {
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
mod tests {
    use super::*;

    #[test]
    fn json_and_quiet_errors_do_not_expose_arbitrary_messages() {
        let error = AppError::message("provider.switch.apply", "secret=sk-sensitive");

        let json = render_error(&error, OutputFormat::Json).unwrap();
        assert!(!json.contains("sk-sensitive"));
        assert!(json.contains("provider.switch.apply"));
        assert_eq!(render_error(&error, OutputFormat::Quiet), None);
        let table = render_error(&error, OutputFormat::Table).unwrap();
        assert!(!table.contains("sk-sensitive"));
        assert!(table.contains("sk-***") || table.contains("***"));
        assert!(table.contains("provider.switch.apply"));
    }

    #[test]
    fn assume_yes_skips_terminal_prompt() {
        confirm("provider write", true).unwrap();
    }
}
