//! Multi-agent run (parallel / sequential) payloads.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::AgentId;

/// How multiple agents are scheduled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Parallel,
    Sequential,
}

impl RunMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parallel => "parallel",
            Self::Sequential => "sequential",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "parallel" | "par" | "p" => Some(Self::Parallel),
            "sequential" | "seq" | "s" => Some(Self::Sequential),
            _ => None,
        }
    }
}

/// Per-agent outcome status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
    Timeout,
    Skipped,
    DryRun,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::Skipped => "skipped",
            Self::DryRun => "dry_run",
            Self::Cancelled => "cancelled",
        }
    }

    /// Counts as a hard failure for overall report / exit code.
    /// `Cancelled` is not a hard failure (user-initiated stop).
    pub fn is_hard_failure(self) -> bool {
        matches!(self, Self::Failed | Self::Timeout)
    }
}

/// How headless CLI stdout should be produced / decoded for process UI.
///
/// CLI `agenthub run` keeps [`ProcessMode::Text`] (human-readable).
/// GUI Chat uses [`ProcessMode::Auto`] so all registered agents emit structured
/// streams when their CLI supports it (Claude/Codex/Kimi/Pi/Grok).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProcessMode {
    /// Plain text stdout (no NDJSON parse). Default for CLI multi-run.
    #[default]
    Text,
    /// Prefer structured JSONL / stream-json when the agent supports it.
    Structured,
    /// Structured for every agent that has a known decoder; text otherwise.
    Auto,
}

impl ProcessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Structured => "structured",
            Self::Auto => "auto",
        }
    }

    /// Whether this mode wants structured output, given the agent's matrix cell.
    ///
    /// Pass `adapters::supports_structured_stream(agent)` as `agent_supports`.
    /// Kept free of adapter imports so `models` stays a leaf crate module.
    pub fn wants_structured(self, agent_supports: bool) -> bool {
        match self {
            Self::Text => false,
            Self::Structured | Self::Auto => agent_supports,
        }
    }
}

/// Options shared by all agents in one multi-run.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub mode: RunMode,
    pub timeout: Duration,
    pub cwd: Option<PathBuf>,
    pub dry_run: bool,
    /// When true (default), missing agents become Skipped instead of hard error.
    pub skip_missing: bool,
    /// Inject agent-specific dangerous auto-approve flags. Default false.
    pub allow_dangerous: bool,
    /// Truncate captured stdout/stderr beyond this many bytes.
    pub max_output_bytes: usize,
    /// Structured process stream decoding (Chat UI). Default text for CLI safety.
    pub process_mode: ProcessMode,
    /// When set, print-capable agents resume this official CLI session.
    pub native_session_id: Option<String>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            mode: RunMode::Parallel,
            timeout: crate::catalog::limits::DEFAULT_RUN_TIMEOUT,
            cwd: None,
            dry_run: false,
            skip_missing: true,
            allow_dangerous: false,
            max_output_bytes: 2 * 1024 * 1024,
            process_mode: ProcessMode::Text,
            native_session_id: None,
        }
    }
}

/// Concrete process invocation for one agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSpec {
    pub agent: AgentId,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Extra environment variables for the child process (e.g. `ELECTRON_RUN_AS_NODE=1`).
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl RunSpec {
    /// Shell-ish display string for logs / dry-run (not for re-exec).
    pub fn display_command(&self) -> String {
        let mut parts = Vec::with_capacity(self.env.len() + 1 + self.args.len());
        for (k, v) in &self.env {
            parts.push(format!("{k}={v}"));
        }
        parts.push(quote_if_needed(&self.program.display().to_string()));
        let mut hide_next = false;
        for a in &self.args {
            if hide_next {
                parts.push("<prompt>".into());
                hide_next = false;
                continue;
            }
            if a == "-p" || a == "--prompt" {
                hide_next = true;
            }
            parts.push(quote_if_needed(a));
        }
        parts.join(" ")
    }
}

fn quote_if_needed(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// One agent's run outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResult {
    pub agent: AgentId,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub command: String,
    pub error: Option<String>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
}

impl AgentRunResult {
    pub fn skipped(agent: AgentId, reason: impl Into<String>) -> Self {
        Self {
            agent,
            status: RunStatus::Skipped,
            exit_code: None,
            duration_ms: 0,
            stdout: String::new(),
            stderr: String::new(),
            command: String::new(),
            error: Some(reason.into()),
            truncated: false,
            native_session_id: None,
        }
    }

    pub fn dry_run(agent: AgentId, command: String) -> Self {
        Self {
            agent,
            status: RunStatus::DryRun,
            exit_code: None,
            duration_ms: 0,
            stdout: String::new(),
            stderr: String::new(),
            command,
            error: None,
            truncated: false,
            native_session_id: None,
        }
    }
}

/// Aggregated multi-agent run report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiRunReport {
    pub prompt: String,
    pub mode: RunMode,
    pub results: Vec<AgentRunResult>,
    /// True when no hard failures (Failed/Timeout). Skipped/DryRun are OK.
    pub ok: bool,
    pub started_at: String,
    pub finished_at: String,
}

impl MultiRunReport {
    pub fn from_results(
        prompt: String,
        mode: RunMode,
        results: Vec<AgentRunResult>,
        started_at: String,
        finished_at: String,
    ) -> Self {
        let ok = results.iter().all(|r| !r.status.is_hard_failure());
        Self {
            prompt,
            mode,
            results,
            ok,
            started_at,
            finished_at,
        }
    }

    pub fn success_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.status, RunStatus::Ok | RunStatus::DryRun))
            .count()
    }

    pub fn hard_failure_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status.is_hard_failure())
            .count()
    }
}

/// Parse comma-separated agent ids (`claude,codex`).
pub fn parse_agent_list(s: &str) -> Result<Vec<AgentId>, String> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let id = AgentId::parse(part).ok_or_else(|| {
            format!(
                "invalid agent id '{part}', expected: {}",
                AgentId::expected_list()
            )
        })?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.is_empty() {
        return Err("agent list is empty".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_list_ok() {
        let ids = parse_agent_list("claude, codex,claude").unwrap();
        assert_eq!(ids, vec![AgentId::Claude, AgentId::Codex]);
    }

    #[test]
    fn parse_agent_list_rejects_invalid() {
        assert!(parse_agent_list("claude,foo").is_err());
        assert!(parse_agent_list("").is_err());
        assert!(parse_agent_list("  ,  ").is_err());
    }

    #[test]
    fn run_mode_parse() {
        assert_eq!(RunMode::parse("parallel"), Some(RunMode::Parallel));
        assert_eq!(RunMode::parse("SEQ"), Some(RunMode::Sequential));
        assert_eq!(RunMode::parse("nope"), None);
    }

    #[test]
    fn run_spec_display_quotes_spaces() {
        let spec = RunSpec {
            agent: AgentId::Claude,
            program: PathBuf::from(r"C:\Program Files\claude.exe"),
            args: vec!["-p".into(), "hello world".into()],
            cwd: None,
            env: vec![],
        };
        let s = spec.display_command();
        assert!(s.contains('\"'));
        assert!(s.contains("<prompt>"));
        assert!(!s.contains("hello world"));
    }

    #[test]
    fn multi_run_report_ok_ignores_skipped() {
        let report = MultiRunReport::from_results(
            "p".into(),
            RunMode::Parallel,
            vec![
                AgentRunResult::skipped(AgentId::Claude, "missing"),
                AgentRunResult {
                    agent: AgentId::Codex,
                    status: RunStatus::Ok,
                    exit_code: Some(0),
                    duration_ms: 1,
                    stdout: "ok".into(),
                    stderr: String::new(),
                    command: "codex".into(),
                    error: None,
                    truncated: false,
                    native_session_id: None,
                },
            ],
            "t0".into(),
            "t1".into(),
        );
        assert!(report.ok);
        assert_eq!(report.success_count(), 1);
    }

    #[test]
    fn multi_run_report_failed_sets_ok_false() {
        let report = MultiRunReport::from_results(
            "p".into(),
            RunMode::Sequential,
            vec![AgentRunResult {
                agent: AgentId::Grok,
                status: RunStatus::Timeout,
                exit_code: None,
                duration_ms: 100,
                stdout: String::new(),
                stderr: String::new(),
                command: "grok".into(),
                error: Some("timeout".into()),
                truncated: false,
                native_session_id: None,
            }],
            "t0".into(),
            "t1".into(),
        );
        assert!(!report.ok);
        assert_eq!(report.hard_failure_count(), 1);
    }
}
