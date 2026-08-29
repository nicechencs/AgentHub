//! Unified logging for AgentHub (CLI + GUI).
//!
//! - File sink: `{data_dir}/logs/agenthub.YYYY-MM-DD.log` (daily rotation)
//! - Optional stderr console
//! - Retention purge by `log_retention_days`
//! - Module targets under [`targets`]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{Local, NaiveDate};
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

use crate::catalog::limits::DEFAULT_LOG_RETENTION_DAYS as DEFAULT_RETENTION_DAYS;
use crate::error::{AppError, Result};
use crate::utils::paths::{db_path, ensure_data_layout, logs_dir, resolve_data_dir};
use crate::utils::redact::redact_text;

/// Canonical tracing targets (module ids in log lines).
pub mod targets {
    pub const BOOT: &str = "core.boot";
    pub const STORAGE: &str = "core.storage";
    pub const LOCK: &str = "core.lock";
    pub const PROVIDER: &str = "core.provider";
    pub const ACCOUNT: &str = "core.account";
    pub const BACKUP: &str = "core.backup";
    pub const INSTALL: &str = "core.install";
    /// Agent / runtime detection (doctor, Agents page, redetect after install).
    pub const DETECT: &str = "core.detect";
    pub const SKILL: &str = "core.skill";
    pub const CHAT: &str = "core.chat";
    pub const PROJECT: &str = "core.project";
    pub const RUN: &str = "core.run";
    /// Capability matrix gates (`AdapterRegistry::require` / doctor matrix).
    pub const CAPABILITY: &str = "core.capability";
    pub const SETTINGS: &str = "core.settings";
    pub const USAGE: &str = "core.usage";
    pub const OAUTH: &str = "core.oauth";
    /// Ticket bind / local Routes / bridge lifecycle milestones.
    pub const ADAPTER: &str = "core.adapter";
    pub const CLI: &str = "cli";
    pub const GUI: &str = "gui";
}

const DEFAULT_LEVEL: &str = "info";
const MIN_RETENTION_DAYS: u32 = 1;
const MAX_RETENTION_DAYS: u32 = 365;
const LOG_FILENAME_PREFIX: &str = "agenthub";
const LOG_FILENAME_SUFFIX: &str = "log";

static LOG_GUARD: OnceLock<Mutex<Option<WorkerGuard>>> = OnceLock::new();
static INIT_DONE: OnceLock<()> = OnceLock::new();

/// Logging bootstrap configuration.
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub data_dir: PathBuf,
    /// File log level: error|warn|info|debug|trace
    pub level: String,
    pub retention_days: u32,
    /// When true, also log to stderr.
    pub console: bool,
    /// Console level (defaults to `level`; CLI often uses warn unless verbose).
    pub console_level: Option<String>,
    /// `"cli"` or `"gui"`
    pub shell: &'static str,
    pub version: &'static str,
}

impl LogConfig {
    pub fn logs_dir(&self) -> PathBuf {
        logs_dir(&self.data_dir)
    }
}

/// Stats from retention purge.
#[derive(Debug, Clone, Default)]
pub struct PurgeStats {
    pub deleted: u32,
    pub kept: u32,
    pub errors: u32,
}

/// Parse and validate log level string.
pub fn parse_level(s: &str) -> Result<Level> {
    match s.trim().to_ascii_lowercase().as_str() {
        "error" => Ok(Level::ERROR),
        "warn" | "warning" => Ok(Level::WARN),
        "info" => Ok(Level::INFO),
        "debug" => Ok(Level::DEBUG),
        "trace" => Ok(Level::TRACE),
        other => Err(AppError::InvalidArg(format!(
            "invalid log_level '{other}', expected: error|warn|info|debug|trace"
        ))),
    }
}

/// Validate retention days (1..=365).
pub fn parse_retention_days(s: &str) -> Result<u32> {
    let n: u32 = s.trim().parse().map_err(|_| {
        AppError::InvalidArg(format!(
            "invalid log_retention_days '{s}', expected integer {MIN_RETENTION_DAYS}..={MAX_RETENTION_DAYS}"
        ))
    })?;
    if !(MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&n) {
        return Err(AppError::InvalidArg(format!(
            "log_retention_days out of range: {n} (allowed {MIN_RETENTION_DAYS}..={MAX_RETENTION_DAYS})"
        )));
    }
    Ok(n)
}

/// Load log_level + retention from SQLite settings if present.
pub fn load_log_prefs(data_dir: &Path) -> (String, u32) {
    let values =
        crate::storage::peek_settings(&db_path(data_dir), &["log_level", "log_retention_days"]);
    let level = values
        .get("log_level")
        .filter(|s| parse_level(s).is_ok())
        .cloned()
        .unwrap_or_else(|| DEFAULT_LEVEL.into());
    let retention = values
        .get("log_retention_days")
        .and_then(|s| parse_retention_days(s).ok())
        .unwrap_or(DEFAULT_RETENTION_DAYS);
    (level, retention)
}

/// Resolve data dir, ensure layout, load prefs, init logging.
///
/// Safe to call once per process. Subsequent calls are no-ops (Ok).
pub fn init_for_app(
    data_dir_override: Option<&Path>,
    shell: &'static str,
    verbose: bool,
    version: &'static str,
) -> Result<()> {
    let data_dir = resolve_data_dir(data_dir_override)?;
    ensure_data_layout(&data_dir)?;
    let (mut level, retention_days) = load_log_prefs(&data_dir);
    if verbose {
        // Raise at least to debug when -v
        if matches!(
            parse_level(&level).unwrap_or(Level::INFO),
            Level::ERROR | Level::WARN | Level::INFO
        ) {
            level = "debug".into();
        }
    }
    let console_level = if verbose {
        Some("debug".into())
    } else if shell == "cli" {
        Some("warn".into())
    } else {
        // GUI: no console by default (Windows release has no console)
        None
    };
    init_logging(LogConfig {
        data_dir,
        level,
        retention_days,
        console: console_level.is_some(),
        console_level,
        shell,
        version,
    })
}

/// Initialize tracing (file + optional stderr). Idempotent.
pub fn init_logging(cfg: LogConfig) -> Result<()> {
    if INIT_DONE.get().is_some() {
        return Ok(());
    }

    let file_level = parse_level(&cfg.level)?;
    let retention = if (MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&cfg.retention_days) {
        cfg.retention_days
    } else {
        DEFAULT_RETENTION_DAYS
    };

    let logs = cfg.logs_dir();
    fs::create_dir_all(&logs)?;

    let purge = purge_old_logs(&logs, retention);
    if purge.deleted > 0 {
        // Cannot log yet; purge stats go into first boot line via fields below.
    }

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILENAME_PREFIX)
        .filename_suffix(LOG_FILENAME_SUFFIX)
        .build(&logs)
        .map_err(|e| AppError::message("io", format!("init rolling log: {e}")))?;
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    let file_filter = EnvFilter::new(format_level_directive(file_level));

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_filter(file_filter);

    let console_level = cfg
        .console_level
        .as_deref()
        .map(parse_level)
        .transpose()?
        .unwrap_or(file_level);

    // Build subscriber with optional console layer.
    let registry = tracing_subscriber::registry().with(file_layer);

    let init_result = if cfg.console {
        let console_filter = EnvFilter::new(format_level_directive(console_level));
        let console_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .with_target(true)
            .with_level(true)
            .with_filter(console_filter);
        registry.with(console_layer).try_init()
    } else {
        registry.try_init()
    };

    // Ignore "already initialized" from tests / double shell.
    let _ = init_result;

    let slot = LOG_GUARD.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(guard);
    }

    let _ = INIT_DONE.set(());

    tracing::info!(
        target: targets::BOOT,
        module = targets::BOOT,
        op = "init",
        shell = cfg.shell,
        version = cfg.version,
        data_dir = %cfg.data_dir.display(),
        logs_dir = %logs.display(),
        log_level = %cfg.level,
        retention_days = retention,
        purged = purge.deleted,
        "logging initialized"
    );

    Ok(())
}

fn format_level_directive(level: Level) -> String {
    // EnvFilter: "info" means info and above for all targets.
    LevelFilter::from_level(level).to_string()
}

/// Delete `agenthub.YYYY-MM-DD.log` (and legacy dated names) older than retention.
pub fn purge_old_logs(logs_dir: &Path, retention_days: u32) -> PurgeStats {
    let mut stats = PurgeStats::default();
    let Ok(entries) = fs::read_dir(logs_dir) else {
        return stats;
    };
    let today = Local::now().date_naive();
    let cutoff = today - chrono::Duration::days(i64::from(retention_days));

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(LOG_FILENAME_PREFIX) {
            continue;
        }
        let Some(date) = parse_log_filename_date(name) else {
            // Keep non-dated active files (e.g. agenthub without suffix while open)
            stats.kept += 1;
            continue;
        };
        if date < cutoff {
            match fs::remove_file(&path) {
                Ok(()) => stats.deleted += 1,
                Err(_) => stats.errors += 1,
            }
        } else {
            stats.kept += 1;
        }
    }
    stats
}

/// Accept `agenthub.2026-08-02.log`, `agenthub.2026-08-02`, `agenthub-2026-08-02.log`.
fn parse_log_filename_date(name: &str) -> Option<NaiveDate> {
    let rest = name.strip_prefix(LOG_FILENAME_PREFIX)?;
    let rest = rest
        .trim_start_matches(['.', '-', '_'])
        .trim_end_matches(".log");
    // tracing-appender daily: often `prefix.YYYY-MM-DD`
    if rest.len() >= 10 {
        let ymd = &rest[..10];
        if let Ok(d) = NaiveDate::parse_from_str(ymd, "%Y-%m-%d") {
            return Some(d);
        }
    }
    None
}

/// Log an [`AppError`] at ERROR with stable fields (message redacted).
///
/// `module` is a field (not tracing `target:`) so callers can pass runtime
/// constants from [`targets`]. Search logs with `module=core.provider`.
pub fn log_app_error(module: &'static str, op: &str, err: &AppError) {
    let msg = redact_text(&err.to_string());
    tracing::error!(module = module, code = err.code(), op = op, "{msg}");
}

/// Log an [`AppError`] with agent id.
pub fn log_app_error_agent(module: &'static str, op: &str, agent: &str, err: &AppError) {
    let msg = redact_text(&err.to_string());
    tracing::error!(
        module = module,
        code = err.code(),
        op = op,
        agent = agent,
        "{msg}"
    );
}

/// Info milestone helper.
pub fn log_info(module: &'static str, op: &str, msg: &str) {
    let msg = redact_text(msg);
    tracing::info!(module = module, op = op, "{msg}");
}

/// Warn helper (redacted message).
pub fn log_warn(module: &'static str, op: &str, msg: &str) {
    let msg = redact_text(msg);
    tracing::warn!(module = module, op = op, "{msg}");
}

/// Debug helper for path/lock/timing (redacted).
pub fn log_debug(module: &'static str, op: &str, msg: &str) {
    let msg = redact_text(msg);
    tracing::debug!(module = module, op = op, "{msg}");
}

/// Today's log file name (for tests / doctor).
pub fn today_log_stem() -> String {
    format!(
        "{LOG_FILENAME_PREFIX}.{}.{LOG_FILENAME_SUFFIX}",
        Local::now().format("%Y-%m-%d")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use crate::storage::Database;
    use crate::utils::paths::db_path;

    #[test]
    fn parse_level_accepts_canonical() {
        assert_eq!(parse_level("info").unwrap(), Level::INFO);
        assert_eq!(parse_level("WARN").unwrap(), Level::WARN);
        assert_eq!(parse_level("warning").unwrap(), Level::WARN);
        assert_eq!(parse_level("trace").unwrap(), Level::TRACE);
        assert!(parse_level("verbose").is_err());
        assert!(parse_level("").is_err());
    }

    #[test]
    fn parse_retention_bounds() {
        assert_eq!(parse_retention_days("1").unwrap(), 1);
        assert_eq!(parse_retention_days("14").unwrap(), 14);
        assert_eq!(parse_retention_days("365").unwrap(), 365);
        assert!(parse_retention_days("0").is_err());
        assert!(parse_retention_days("999").is_err());
        assert!(parse_retention_days("x").is_err());
    }

    #[test]
    fn parse_log_filename_date_variants() {
        assert_eq!(
            parse_log_filename_date("agenthub.2026-08-02"),
            Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
        );
        assert_eq!(
            parse_log_filename_date("agenthub.2026-08-02.log"),
            Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
        );
        assert_eq!(
            parse_log_filename_date("agenthub-2026-08-02.log"),
            Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
        );
        assert!(parse_log_filename_date("other.log").is_none());
        assert!(parse_log_filename_date("agenthub").is_none());
    }

    #[test]
    fn purge_deletes_old_files_keeps_recent() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("agenthub.2020-01-01");
        let mut f = fs::File::create(&old).unwrap();
        writeln!(f, "old").unwrap();

        let today = Local::now().format("%Y-%m-%d");
        let recent = dir.path().join(format!("agenthub.{today}.log"));
        let mut f2 = fs::File::create(&recent).unwrap();
        writeln!(f2, "recent").unwrap();

        let old_log = dir.path().join("agenthub.2020-01-02.log");
        let mut f3 = fs::File::create(&old_log).unwrap();
        writeln!(f3, "old log").unwrap();

        // non-log file should be ignored (not counted as deleted)
        let other = dir.path().join("readme.txt");
        fs::write(&other, "x").unwrap();

        let stats = purge_old_logs(dir.path(), 14);
        assert_eq!(stats.deleted, 2);
        assert!(!old.exists());
        assert!(!old_log.exists());
        assert!(recent.exists());
        assert!(other.exists());
        assert!(stats.kept >= 1);
    }

    #[test]
    fn load_log_prefs_defaults_when_db_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (level, days) = load_log_prefs(dir.path());
        assert_eq!(level, "info");
        assert_eq!(days, 14);
    }

    #[test]
    fn load_log_prefs_reads_settings_and_rejects_invalid_level() {
        let dir = tempfile::tempdir().unwrap();
        ensure_data_layout(dir.path()).unwrap();
        let db = Database::open(&db_path(dir.path())).unwrap();
        db.set_setting("log_level", "debug").unwrap();
        db.set_setting("log_retention_days", "30").unwrap();
        drop(db);

        let (level, days) = load_log_prefs(dir.path());
        assert_eq!(level, "debug");
        assert_eq!(days, 30);

        // invalid level falls back to default info while retention still reads
        let db = Database::open(&db_path(dir.path())).unwrap();
        db.set_setting("log_level", "nope").unwrap();
        db.set_setting("log_retention_days", "7").unwrap();
        drop(db);
        let (level2, days2) = load_log_prefs(dir.path());
        assert_eq!(level2, "info");
        assert_eq!(days2, 7);
    }

    #[test]
    fn init_logging_is_idempotent_and_creates_logs_dir() {
        let dir = tempfile::tempdir().unwrap();
        ensure_data_layout(dir.path()).unwrap();
        let cfg = LogConfig {
            data_dir: dir.path().to_path_buf(),
            level: "info".into(),
            retention_days: 14,
            console: false,
            console_level: None,
            shell: "cli",
            version: "0.0.0-test",
        };
        // First call may succeed or no-op if process already initialized subscriber.
        let _ = init_logging(cfg.clone());
        // Second call must not error.
        init_logging(cfg).unwrap();
        assert!(logs_dir(dir.path()).is_dir());
    }

    #[test]
    fn today_log_stem_format() {
        let stem = today_log_stem();
        assert!(stem.starts_with("agenthub."));
        assert!(stem.ends_with(".log"));
        assert_eq!(stem.len(), "agenthub.YYYY-MM-DD.log".len());
    }
}
