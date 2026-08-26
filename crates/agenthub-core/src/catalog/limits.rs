//! Named product limits and default timeouts.
//! Business code should import these instead of scattering magic numbers.

use std::time::Duration;

// --- Run / chat ---

/// Default per-agent run timeout (CLI `run --timeout`, chat send).
pub const DEFAULT_RUN_TIMEOUT_SECS: u64 = 300;
/// Same as [`DEFAULT_RUN_TIMEOUT_SECS`] as a [`Duration`].
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(DEFAULT_RUN_TIMEOUT_SECS);
/// Default max captured stdout/stderr for a run process.
pub const DEFAULT_RUN_MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
/// Max characters of stitched chat history included in a prompt.
pub const CONTEXT_CHAR_LIMIT: usize = 24_000;

// --- Install ---

/// Timeout for runtime (Node/npm) install steps.
pub const INSTALL_ENV_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Timeout for agent CLI install / uninstall / upgrade steps.
pub const INSTALL_AGENT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Max captured output bytes for install-family commands.
pub const INSTALL_MAX_OUTPUT_BYTES: usize = 512 * 1024;

// --- Detect / cache ---

/// Default timeout for version / detect probes.
pub const DETECT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);
/// Max stdout/stderr retained for detect probes.
pub const DETECT_CAPTURE_MAX_BYTES: usize = 256 * 1024;
/// Runtime env detect cache TTL.
pub const DETECT_CACHE_TTL: Duration = Duration::from_secs(30);
/// Agent detect list cache TTL (same order of magnitude as env detect).
pub const AGENT_DETECT_CACHE_TTL: Duration = Duration::from_secs(30);

// --- OAuth ---

/// In-memory OAuth session TTL.
pub const OAUTH_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
/// Upstream account quota probe (ChatGPT /wham/usage, Claude oauth usage).
pub const ACCOUNT_QUOTA_HTTP_TIMEOUT: Duration = Duration::from_secs(12);
/// How long a successful quota snapshot is considered fresh on list().
pub const ACCOUNT_QUOTA_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// Local callback listener overall wait.
pub const OAUTH_CALLBACK_LISTEN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Token HTTP request timeout.
pub const OAUTH_TOKEN_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Subtract from `expires_in` when materializing Pi auth.json `expires` (ms).
/// Matches upstream pi-ai refresh skew so tokens renew before hard expiry.
pub const OAUTH_REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;

// --- Project scan ---

/// Max characters kept in list-row preview.
pub const PROJECT_PREVIEW_CHARS: usize = 120;
/// Cap total sessions returned per agent to keep UI snappy.
pub const PROJECT_MAX_PER_AGENT: usize = 500;
/// When scanning a large jsonl, only read the first N bytes for list-row preview.
pub const PROJECT_SCAN_BYTES: u64 = 256 * 1024;
/// Bytes *scanned* from a jsonl while collecting excerpt turns.
/// Must exceed Codex rollouts / Grok `updates.jsonl` that bury later turns behind multi-MB tool dumps.
/// Matching user/assistant lines are kept in full (tool dumps are skipped).
pub const PROJECT_EXCERPT_READ_BYTES: u64 = 256 * 1024 * 1024;
/// Cheap `list_projects` peek of the newest session file (cwd / preview only).
/// Do not use this in `list_sessions` — that path still uses [`PROJECT_SCAN_BYTES`].
pub const PROJECT_LIST_HEAD_BYTES: u64 = 16 * 1024;

// --- Runtime / logging / GUI ---

/// Minimum supported Node.js major version for shared doctor / package install.
/// Do not raise this for Pi — Pi's `engines.node` is handled separately.
pub const NODE_MIN_MAJOR: u64 = 18;
/// Pi CLI `engines.node` floor (`>=22.19.0`). Probe + Chat must use this Node.
pub const PI_NODE_MIN_MAJOR: u64 = 22;
/// Pi `engines.node` minor (`>=22.19.0`). 22.11 must not satisfy.
pub const PI_NODE_MIN_MINOR: u64 = 19;
/// Default log file retention days.
pub const DEFAULT_LOG_RETENTION_DAYS: u32 = 14;
/// Default foreground usage collect interval (minutes). `0` = manual only.
pub const DEFAULT_USAGE_COLLECT_INTERVAL_MIN: u32 = 30;
/// Max allowed usage collect interval (minutes) — matches frontend `normalizeIntervalMin`.
pub const MAX_USAGE_COLLECT_INTERVAL_MIN: u32 = 24 * 60;
/// Debounce for skills filesystem watcher (GUI).
pub const SKILL_FS_DEBOUNCE: Duration = Duration::from_millis(450);

// --- Skill preview ---

/// Max characters returned for a skill `SKILL.md` GUI preview.
pub const SKILL_MARKDOWN_PREVIEW_CHARS: usize = 256 * 1024;

// --- Skill market HTTP ---

pub const SKILLS_SH_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub const SKILLS_SH_READ_TIMEOUT: Duration = Duration::from_secs(20);
pub const SKILLS_SH_CURL_CONNECT_SECS: u64 = 12;
pub const SKILLS_SH_CURL_MAX_SECS: u64 = 35;
pub const SKILLS_SH_POWERSHELL_TIMEOUT_SECS: u64 = 25;
