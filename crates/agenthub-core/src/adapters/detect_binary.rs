//! Shared binary detection for production adapters.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::models::{AgentId, DetectResult, DetectStatus, DetectedBinaryCopy};
use crate::utils::redact::redact_text;

/// Cache TTL for `npm prefix -g` (same machine result; avoids N console flashes per detect_all).
const NPM_PREFIX_CACHE_TTL: Duration = Duration::from_secs(600);

/// Shared detect helper used by adapters.
///
/// Platform matrix:
/// - PATH / `which` with Win suffixes (`.cmd` / `.exe` / bare)
/// - well-known install dirs that do **not** require the GUI process PATH
///   (Tauri often inherits a stale PATH after installs)
/// - npm global bin: `npm prefix -g` (npm itself may be off PATH; probe
///   well-known node dirs and `~/.npmrc` `prefix=`)
pub(crate) fn detect_binary(
    agent: AgentId,
    candidates: &[&str],
    version_args: &[&str],
    channel_hint: Option<&str>,
    env_ready: bool,
) -> DetectResult {
    detect_binary_with_env(
        agent,
        candidates,
        version_args,
        channel_hint,
        env_ready,
        &[],
    )
}

/// Same as [`detect_binary`], with extra child env for the version probe
/// (Pi prefixes PATH with a Node 22 bin dir).
pub(crate) fn detect_binary_with_env(
    agent: AgentId,
    candidates: &[&str],
    version_args: &[&str],
    channel_hint: Option<&str>,
    env_ready: bool,
    extra_env: &[(String, String)],
) -> DetectResult {
    use crate::models::DetectStatus;
    use which::which;

    let mut names: Vec<String> = Vec::new();
    for base in candidates {
        for n in expand_binary_names(base) {
            if !names.iter().any(|e| e.eq_ignore_ascii_case(&n)) {
                names.push(n);
            }
        }
    }

    let mut result = (|| {
        for name in &names {
            if let Ok(path) = which(name) {
                if !is_direct_spawnable(&path) {
                    continue;
                }
                if is_under_agenthub_user_npm_prefix(&path) {
                    tracing::info!(
                        target: crate::logging::targets::DETECT,
                        module = crate::logging::targets::DETECT,
                        op = "detect_binary",
                        agent = agent.as_str(),
                        via = "path_leftover_skip",
                        path = %path.display(),
                        "skipping leftover AgentHub data-dir npm copy on PATH; not an install target"
                    );
                    continue;
                }
                let channel = infer_channel(&path, channel_hint);
                tracing::debug!(
                    target: crate::logging::targets::DETECT,
                    module = crate::logging::targets::DETECT,
                    op = "detect_binary",
                    agent = agent.as_str(),
                    via = "path",
                    channel = %channel,
                    path = %path.display(),
                    "agent binary resolved on PATH"
                );
                return finish_detect(
                    agent,
                    path,
                    version_args,
                    Some(channel.as_str()),
                    env_ready,
                    false,
                    extra_env,
                );
            }
        }

        // Remaining well-known dirs (OS npm global, ~/.local/bin, …).
        // Never spawn from leftover `~/.agenthub/npm` — that directory is not
        // an install target for any agent.
        for (path, channel) in well_known_bin_paths(agent) {
            if is_under_agenthub_user_npm_prefix(&path) {
                tracing::info!(
                    target: crate::logging::targets::DETECT,
                    module = crate::logging::targets::DETECT,
                    op = "detect_binary",
                    agent = agent.as_str(),
                    via = "well_known_leftover_skip",
                    channel = channel,
                    path = %path.display(),
                    "skipping leftover AgentHub data-dir npm copy in well-known dirs; not an install target"
                );
                continue;
            }
            if path.is_file() && is_direct_spawnable(&path) {
                // PATH miss but disk hit — common after install without AgentHub restart.
                tracing::info!(
                    target: crate::logging::targets::DETECT,
                    module = crate::logging::targets::DETECT,
                    op = "detect_binary",
                    agent = agent.as_str(),
                    via = "well_known",
                    channel = channel,
                    path = %path.display(),
                    "agent binary found outside process PATH (well-known dir); restart may refresh PATH"
                );
                return finish_detect(
                    agent,
                    path,
                    version_args,
                    Some(channel),
                    env_ready,
                    true,
                    extra_env,
                );
            }
        }

        tracing::debug!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect_binary",
            agent = agent.as_str(),
            candidates = ?names,
            "agent binary not found on PATH or well-known dirs"
        );

        DetectResult {
            agent,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready,
            notes: vec![NOT_FOUND_FIREFIGHTING_NOTE.into()],
            extra_copies: Vec::new(),
        }
    })();
    attach_leftover_agenthub_npm_copy(&mut result, agent, &names, extra_env);
    attach_extra_binary_copies(
        &mut result,
        well_known_bin_paths(agent)
            .into_iter()
            .filter(|(path, _)| !is_under_agenthub_user_npm_prefix(path)),
        version_args,
        extra_env,
    );
    result
}

/// Other on-disk CLIs besides `binary_path` (npm/native well-known, IDE, desktop).
/// Leftover `~/.agenthub/npm` is attached separately and is never a spawn target.
/// When still `NotFound`, a spawnable extra copy (native → npm → desktop → ide)
/// is promoted to the spawn target so the agent counts as installed.
pub(crate) fn attach_extra_binary_copies(
    result: &mut DetectResult,
    candidates: impl IntoIterator<Item = (PathBuf, &'static str)>,
    version_args: &[&str],
    extra_env: &[(String, String)],
) {
    let primary = result.binary_path.as_deref();
    let mut added = 0usize;
    for (path, kind) in candidates {
        if !path.is_file() || !is_direct_spawnable(&path) {
            continue;
        }
        if is_under_agenthub_user_npm_prefix(&path) {
            continue;
        }
        if primary.is_some_and(|p| leftover_paths_equal(p, &path)) {
            continue;
        }
        if result
            .extra_copies
            .iter()
            .any(|c| leftover_paths_equal(&c.path, &path))
        {
            continue;
        }
        let version = probe_bin_version(&path, version_args, extra_env);
        let channel = match kind {
            "ide" | "desktop" => None,
            other => Some(other.to_string()),
        };
        result.extra_copies.push(DetectedBinaryCopy::from_kind(
            result.agent,
            path,
            kind,
            version,
            channel,
        ));
        added += 1;
    }
    if added > 0 {
        refresh_channel_extra_copies_note(result);
    }
    promote_spawnable_extra_copy_if_missing(result);
}

/// PATH / well-known miss, but a real extra copy exists (desktop hashed dir, IDE, …).
/// Leftover AgentHub npm is observational only and never becomes Installed.
fn promote_spawnable_extra_copy_if_missing(result: &mut DetectResult) {
    if result.status == DetectStatus::Installed && result.binary_path.is_some() {
        return;
    }
    const ORDER: [&str; 4] = ["native", "npm", "desktop", "ide"];
    let Some(idx) = ORDER
        .iter()
        .find_map(|kind| result.extra_copies.iter().position(|c| c.kind == *kind))
    else {
        return;
    };
    let copy = result.extra_copies.remove(idx);
    let channel = copy
        .channel
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some(copy.kind.clone()));
    tracing::info!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "detect_binary",
        agent = result.agent.as_str(),
        via = %copy.kind,
        path = %copy.path.display(),
        "agent binary found as extra copy; marking Installed"
    );
    result.status = DetectStatus::Installed;
    result.version = copy.version;
    result.channel = channel;
    result.notes.retain(|n| n != NOT_FOUND_FIREFIGHTING_NOTE);
    result.notes.push(format!(
        "found via {} copy (not on process PATH): {}",
        copy.kind,
        copy.path.display()
    ));
    result.binary_path = Some(copy.path);
    refresh_channel_extra_copies_note(result);
}

fn probe_bin_version(
    path: &Path,
    version_args: &[&str],
    extra_env: &[(String, String)],
) -> Option<String> {
    use crate::utils::process::{run_capture_with_env, stdout_first_line};
    let output = run_capture_with_env(path, version_args, extra_env).ok()?;
    stdout_first_line(&output)
        .filter(|l| looks_like_version_line(l))
        .map(|l| extract_version_token(&l))
        .filter(|l| !l.is_empty())
}

fn refresh_channel_extra_copies_note(result: &mut DetectResult) {
    result.notes.retain(|n| !n.starts_with("另有 "));
    let copies: Vec<&DetectedBinaryCopy> = result
        .extra_copies
        .iter()
        .filter(|c| c.kind != "leftover-agenthub")
        .collect();
    if copies.is_empty() {
        return;
    }
    let summary = copies
        .iter()
        .map(|c| {
            format!(
                "{} {} @ {}",
                c.kind,
                c.version.as_deref().unwrap_or("?"),
                c.path.display()
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    result.notes.push(format!(
        "另有 {} 份 {}：{summary}",
        copies.len(),
        result.agent.display_name()
    ));
}

/// Observe leftover `<data>/npm` shims without using them as the spawn target.
fn attach_leftover_agenthub_npm_copy(
    result: &mut DetectResult,
    agent: AgentId,
    names: &[String],
    extra_env: &[(String, String)],
) {
    let Some(path) = first_existing_named_bin(&agenthub_user_npm_bin_dirs(), names) else {
        return;
    };
    if result
        .binary_path
        .as_deref()
        .is_some_and(|primary| leftover_paths_equal(primary, &path))
    {
        return;
    }
    if result
        .extra_copies
        .iter()
        .any(|c| leftover_paths_equal(&c.path, &path))
    {
        return;
    }
    let version = probe_bin_version(&path, &["--version"], extra_env);
    tracing::info!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "detect_binary",
        agent = agent.as_str(),
        via = "leftover_agenthub_npm",
        path = %path.display(),
        "leftover AgentHub data-dir npm copy observed; not used to spawn"
    );
    result.notes.push(format!(
        "发现数据目录遗留 npm 副本 @ {}（不会用于启动，也不会再往 ~/.agenthub/npm 安装）",
        path.display()
    ));
    result.extra_copies.push(DetectedBinaryCopy::from_kind(
        agent,
        path,
        "leftover-agenthub",
        version,
        Some("npm".into()),
    ));
}

fn leftover_paths_equal(a: &Path, b: &Path) -> bool {
    crate::utils::paths::same_path_identity(a, b).unwrap_or_else(|_| {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    })
}

/// Surfaced in DetectResult.notes and searchable in doctor / GUI when binary is missing.
pub(crate) const NOT_FOUND_FIREFIGHTING_NOTE: &str =
    "未在 PATH 或常见安装目录中找到该命令。若刚完成安装，请完全退出并重启 AgentHub。";

/// Expand a base command name with platform-typical suffixes.
///
/// On Windows, CreateProcess-spawnable shims come first. npm always also
/// writes a Unix shebang `name` (no extension) that is **not** a valid Win32
/// application; probing it marks the agent Installed with an empty version.
///
/// `.ps1` is deliberately not produced: CreateProcess cannot spawn PowerShell
/// scripts directly (they require a `powershell -File` shim), and every
/// consumer filters candidates through [`is_direct_spawnable`], which only
/// allows cmd/bat/exe/com — so a `.ps1` entry could never be selected.
pub(crate) fn expand_binary_names(base: &str) -> Vec<String> {
    if cfg!(windows)
        && !base.ends_with(".cmd")
        && !base.ends_with(".exe")
        && !base.ends_with(".ps1")
    {
        return vec![
            format!("{base}.cmd"),
            format!("{base}.exe"),
            base.to_string(),
        ];
    }
    vec![base.to_string()]
}

/// Allowlisted install locations (platform × agent). Channel is `npm` or `native`.
pub(crate) fn well_known_bin_paths(agent: AgentId) -> Vec<(PathBuf, &'static str)> {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return Vec::new();
    };
    let name = agent.as_str();
    let mut paths: Vec<(PathBuf, &'static str)> = Vec::new();

    // Shared helpers: native home bins + npm global prefix shims.
    let push_native = |paths: &mut Vec<(PathBuf, &'static str)>, dir: PathBuf| {
        #[cfg(windows)]
        {
            paths.push((dir.join(format!("{name}.exe")), "native"));
        }
        paths.push((dir.join(name), "native"));
    };
    let push_npm = |paths: &mut Vec<(PathBuf, &'static str)>, dir: PathBuf| {
        // No `{name}.ps1` on Windows: CreateProcess cannot spawn PowerShell
        // scripts directly, and is_direct_spawnable rejects them anyway.
        #[cfg(windows)]
        {
            paths.push((dir.join(format!("{name}.cmd")), "npm"));
            paths.push((dir.join(format!("{name}.exe")), "npm"));
        }
        paths.push((dir.join(name), "npm"));
    };

    match agent {
        AgentId::Claude => {
            push_native(&mut paths, home.join(".local").join("bin"));
            // npm global (Windows AppData\Roaming\npm, macOS common prefixes)
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            // Legacy local npm install from older Claude Code versions.
            push_npm(&mut paths, home.join(".claude").join("local"));
            push_npm(&mut paths, home.join(".claude").join("local").join("bin"));
        }
        AgentId::Codex => {
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            push_native(&mut paths, home.join(".codex").join("bin"));
            #[cfg(windows)]
            {
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    push_native(
                        &mut paths,
                        PathBuf::from(local)
                            .join("Programs")
                            .join("OpenAI")
                            .join("Codex")
                            .join("bin"),
                    );
                }
            }
        }
        AgentId::Kimi => {
            push_native(&mut paths, home.join(".kimi-code").join("bin"));
            push_native(&mut paths, home.join(".kimi").join("bin"));
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
        }
        AgentId::Grok => {
            push_native(&mut paths, home.join(".grok").join("bin"));
            push_native(&mut paths, home.join(".local").join("bin"));
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
        }
        AgentId::Pi => {
            // Primary: npm global (`pi` / `pi.cmd`). Optional native-style home bins.
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            push_native(&mut paths, home.join(".pi").join("bin"));
            push_native(&mut paths, home.join(".pi").join("agent").join("bin"));
        }
        AgentId::WorkBuddy => {
            // Electron desktop under LocalAppData\Programs\WorkBuddy (not PATH/npm).
            #[cfg(windows)]
            {
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    paths.push((
                        PathBuf::from(local)
                            .join("Programs")
                            .join("WorkBuddy")
                            .join("WorkBuddy.exe"),
                        "native",
                    ));
                }
            }
            #[cfg(not(windows))]
            {
                paths.push((
                    PathBuf::from("/Applications/WorkBuddy.app/Contents/MacOS/WorkBuddy"),
                    "native",
                ));
            }
            let _ = home;
        }
        AgentId::Cursor => {
            // Prefer cursor-agent install trees — never bare `agent` under .grok.
            // Full validation lives in CursorAdapter::detect; well-known paths here
            // only feed shared helpers / uninstall allowlists.
            for (p, ch) in super::cursor::uninstall_bin_candidates()
                .into_iter()
                .map(|p| (p, "native"))
            {
                paths.push((p, ch));
            }
            let _ = home;
        }
        AgentId::Dsh => {
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            push_native(&mut paths, home.join(".dsh").join("bin"));
        }
    }

    paths
}

/// User-writable npm prefix that [`well_known_bin_paths`] already scans.
///
/// Not leftover `~/.agenthub/npm`. Unix: `~/.npm-global` (bins in `prefix/bin`).
/// Windows: `%APPDATA%\npm` (bins in the prefix root).
pub(crate) fn user_writable_npm_prefix() -> Option<PathBuf> {
    let home = crate::utils::paths::home_dir().ok()?;
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let prefix = PathBuf::from(appdata).join("npm");
            if !is_under_agenthub_user_npm_prefix(&prefix) {
                return Some(prefix);
            }
        }
        let fallback = home.join("AppData").join("Roaming").join("npm");
        if is_under_agenthub_user_npm_prefix(&fallback) {
            return None;
        }
        Some(fallback)
    }
    #[cfg(not(windows))]
    {
        let prefix = home.join(".npm-global");
        if is_under_agenthub_user_npm_prefix(&prefix) {
            return None;
        }
        Some(prefix)
    }
}

/// Bin dir under [`user_writable_npm_prefix`] (Windows: prefix root; Unix: `prefix/bin`).
pub(crate) fn user_writable_npm_bin_dir() -> Option<PathBuf> {
    user_writable_npm_prefix().map(npm_prefix_to_bin_dir)
}

/// Leftover AgentHub data-dir npm prefix roots (`<data>/npm` and `~/.agenthub/npm`).
///
/// Not install targets and not the OS/global npm prefix. Detect lists these
/// as extra copies only; leftover uninstall iterates the same roots.
pub(crate) fn agenthub_user_npm_prefix_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(data) = crate::utils::paths::resolve_data_dir(None) {
        roots.push(data.join("npm"));
    }
    if let Ok(home) = crate::utils::paths::home_dir() {
        let fallback = home.join(".agenthub").join("npm");
        if !roots.iter().any(|p| p == &fallback) {
            roots.push(fallback);
        }
    }
    roots
}

fn npm_prefix_to_bin_dir(prefix: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        prefix
    }
    #[cfg(not(windows))]
    {
        prefix.join("bin")
    }
}

/// Bin dirs under AgentHub user npm prefixes (Windows: prefix root; Unix: `prefix/bin`).
pub(crate) fn agenthub_user_npm_bin_dirs() -> Vec<PathBuf> {
    agenthub_user_npm_prefix_roots()
        .into_iter()
        .map(npm_prefix_to_bin_dir)
        .collect()
}

/// True when `path` is inside an AgentHub user npm prefix (not legacy global npm).
pub(crate) fn is_under_agenthub_user_npm_prefix(path: &Path) -> bool {
    agenthub_user_npm_prefix_roots()
        .iter()
        .any(|root| path.starts_with(root))
}

/// First existing `names` entry under `dirs` (earlier dir wins). Does not call `which`.
///
/// Windows skips Unix shebang shims and `.ps1` (CreateProcess cannot run them).
pub(crate) fn first_existing_named_bin(dirs: &[PathBuf], names: &[String]) -> Option<PathBuf> {
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() && is_direct_spawnable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// True when `Command::new(path)` can launch the file without a shell.
///
/// npm's extensionless `codex` on Windows is `#!/bin/sh` — CreateProcess
/// fails with "not a valid Win32 application", which used to wipe the
/// version string while still reporting Installed.
pub(crate) fn is_direct_spawnable(path: &Path) -> bool {
    #[cfg(not(windows))]
    {
        let _ = path;
        true
    }
    #[cfg(windows)]
    {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("cmd" | "bat" | "exe" | "com") => true,
            Some(_) => false,
            None => !file_starts_with_shebang(path),
        }
    }
}

#[cfg(windows)]
fn file_starts_with_shebang(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .map(|_| buf == *b"#!")
        .unwrap_or(false)
}

/// Last `prefix=` in npmrc text. Comments skipped; `~` / `$HOME` / `${HOME}` expand via `home`.
pub(crate) fn parse_npmrc_global_prefix(text: &str, home: &Path) -> Option<PathBuf> {
    let mut found = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "prefix" {
            continue;
        }
        let mut value = value.trim();
        if let Some(stripped) = value
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        {
            value = stripped.trim();
        }
        if value.is_empty() {
            continue;
        }
        found = Some(expand_npmrc_path(value, home));
    }
    found
}

fn expand_npmrc_path(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        return home.join(rest);
    }
    let home_str = home.to_string_lossy();
    PathBuf::from(
        value
            .replace("${HOME}", home_str.as_ref())
            .replace("$HOME", home_str.as_ref()),
    )
}

/// `npm prefix -g` stdout → shim dir (Windows: prefix root; Unix: `prefix/bin`).
pub(crate) fn npm_prefix_stdout_to_bin_dir(stdout: &str) -> Option<PathBuf> {
    let prefix = stdout.trim();
    if prefix.is_empty() {
        return None;
    }
    Some(npm_prefix_to_bin_dir(PathBuf::from(prefix)))
}

/// Directories that commonly hold the `npm` CLI when the GUI PATH is stale.
pub(crate) fn well_known_npm_cli_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(pf) = std::env::var(key) {
                push_unique_dir(&mut dirs, PathBuf::from(pf).join("nodejs"));
            }
        }
        let _ = home;
    }
    #[cfg(not(windows))]
    {
        push_unique_dir(&mut dirs, PathBuf::from("/opt/homebrew/bin"));
        push_unique_dir(&mut dirs, PathBuf::from("/usr/local/bin"));
        push_unique_dir(&mut dirs, home.join(".local").join("bin"));
        let nvm = std::env::var_os("NVM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".nvm"));
        push_latest_nvm_node_bin(&mut dirs, &nvm);
    }
    dirs
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

#[cfg(not(windows))]
fn push_latest_nvm_node_bin(dirs: &mut Vec<PathBuf>, nvm_root: &Path) {
    let Ok(rd) = std::fs::read_dir(nvm_root.join("versions").join("node")) else {
        return;
    };
    let mut versions: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();
    if let Some(latest) = versions.pop() {
        push_unique_dir(dirs, latest.join("bin"));
    }
}

fn path_with_dir_prepended(dir: &Path) -> std::ffi::OsString {
    let mut out = dir.as_os_str().to_os_string();
    if let Some(rest) = std::env::var_os("PATH") {
        out.push(if cfg!(windows) { ";" } else { ":" });
        out.push(rest);
    }
    out
}

fn find_npm_cli(home: &Path) -> Option<PathBuf> {
    use which::which;
    if let Ok(path) = which("npm").or_else(|_| which("npm.cmd")) {
        if is_direct_spawnable(&path) {
            return Some(path);
        }
        if let Some(parent) = path.parent() {
            if let Some(sibling) =
                first_existing_named_bin(&[parent.to_path_buf()], &expand_binary_names("npm"))
            {
                return Some(sibling);
            }
        }
    }
    first_existing_named_bin(&well_known_npm_cli_dirs(home), &expand_binary_names("npm"))
}

fn npm_global_bin_dirs_from_cli(home: &Path) -> Vec<PathBuf> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Vec<PathBuf>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((at, dirs)) = guard.as_ref() {
            if at.elapsed() < NPM_PREFIX_CACHE_TTL {
                return dirs.clone();
            }
        }
    }
    let dirs = npm_global_bin_dirs_from_cli_uncached(home);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), dirs.clone()));
    }
    dirs
}

fn npm_global_bin_dirs_from_cli_uncached(home: &Path) -> Vec<PathBuf> {
    use crate::utils::process::run_capture_with_env;
    let Some(npm) = find_npm_cli(home) else {
        return Vec::new();
    };
    // npm.cmd / `#!/usr/bin/env node` need sibling `node` even when GUI PATH is empty.
    let extra_env: Vec<(String, String)> = npm
        .parent()
        .map(|dir| {
            vec![(
                "PATH".to_string(),
                path_with_dir_prepended(dir).to_string_lossy().into_owned(),
            )]
        })
        .unwrap_or_default();
    let output = match run_capture_with_env(&npm, &["prefix", "-g"], &extra_env) {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    npm_prefix_stdout_to_bin_dir(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .collect()
}

fn npm_global_bin_dirs_from_npmrc(home: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(home.join(".npmrc")) else {
        return Vec::new();
    };
    parse_npmrc_global_prefix(&text, home)
        .map(npm_prefix_to_bin_dir)
        .into_iter()
        .filter(|dir| !is_under_agenthub_user_npm_prefix(dir))
        .collect()
}

pub(crate) fn npm_global_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in npm_global_bin_dirs_from_cli(home) {
        if !is_under_agenthub_user_npm_prefix(&dir) {
            push_unique_dir(&mut dirs, dir);
        }
    }
    for dir in npm_global_bin_dirs_from_npmrc(home) {
        push_unique_dir(&mut dirs, dir);
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            push_unique_dir(&mut dirs, PathBuf::from(appdata).join("npm"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            push_unique_dir(&mut dirs, PathBuf::from(local).join("npm"));
        }
        // Fallback when APPDATA is missing (rare): classic Roaming path under home.
        push_unique_dir(&mut dirs, home.join("AppData").join("Roaming").join("npm"));
    }
    #[cfg(not(windows))]
    {
        // Common npm global bin locations on macOS/Linux (PATH may omit them in GUI).
        push_unique_dir(&mut dirs, PathBuf::from("/opt/homebrew/bin"));
        push_unique_dir(&mut dirs, PathBuf::from("/usr/local/bin"));
        push_unique_dir(&mut dirs, home.join(".npm-global").join("bin"));
        push_unique_dir(&mut dirs, home.join(".local").join("bin"));
        let nvm = std::env::var_os("NVM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".nvm"));
        push_latest_nvm_node_bin(&mut dirs, &nvm);
    }
    dirs
}

/// Prefer concrete channel (`npm` / `native`) over ambiguous hints like `npm-or-native`.
pub(crate) fn infer_channel(path: &Path, hint: Option<&str>) -> String {
    // npm global shims on Homebrew/nvm are commonly symlinks from `bin/`
    // into `lib/node_modules/...`. Prefer the canonical target, so even a
    // shim under a generic `~/.local/bin` is not misclassified as native.
    let canonical = std::fs::canonicalize(path).ok();
    let from_path = canonical
        .as_deref()
        .and_then(infer_channel_from_path)
        .or_else(|| infer_channel_from_path(path));

    if let Some(c) = from_path {
        return c.to_string();
    }
    match hint {
        Some("npm") | Some("native") => hint.unwrap().to_string(),
        Some(h) if h.contains("npm") && !h.contains("native") => "npm".into(),
        Some(h) if h.contains("native") && !h.contains("npm") => "native".into(),
        // Ambiguous (e.g. claude "npm-or-native"): default native when path looks like home bin.
        _ => "native".into(),
    }
}

fn infer_channel_from_path(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy().to_ascii_lowercase();
    if s.contains(std::path::MAIN_SEPARATOR) {
        // Windows npm shim: ...\AppData\Roaming\npm\xxx.cmd
        // Unix npm: .../node_modules/... or .../npm-global/...
        if s.contains(&format!("{sep}npm{sep}", sep = std::path::MAIN_SEPARATOR))
            || s.contains("/npm/")
            || s.ends_with(".cmd")
            || s.contains("node_modules")
            || s.contains("npm-global")
            || s.contains(".agenthub") && s.contains("npm")
        {
            Some("npm")
        } else if s.contains("programs") && s.contains("openai") && s.contains("codex") {
            Some("native")
        } else if s.contains(".local")
            || s.contains(".grok")
            || s.contains(".kimi-code")
            || s.contains(".kimi")
            || s.contains(".codex")
            || s.contains(".pi")
            || s.contains("workbuddy")
            || s.contains("programs") && s.contains("workbuddy")
            || s.contains("cursor-agent")
            || s.contains(".cursor")
        {
            Some("native")
        } else {
            None
        }
    } else {
        None
    }
}

/// Reject Windows/cmd noise that is clearly not a version string.
pub(crate) fn looks_like_version_line(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.len() > 120 {
        return false;
    }
    let lower = l.to_ascii_lowercase();
    if lower.contains("not recognized")
        || lower.contains("not found")
        || lower.contains("cannot find")
        || lower.contains("is not recognized")
        || l.contains("不是内部或外部命令")
        || l.contains("无法识别")
        || l.contains("系统找不到")
    {
        return false;
    }
    // Prefer lines that look like versions (digit present or known CLI name prefixes).
    l.chars().any(|c| c.is_ascii_digit())
        || lower.starts_with("claude")
        || lower.starts_with("codex")
        || lower.starts_with("kimi")
        || lower.starts_with("grok")
        || lower.starts_with("pi")
        || lower.starts_with("cursor")
        || lower.starts_with("cursor-agent")
}

/// Extract a display / compare-friendly version token from CLI `--version` output.
///
/// Examples:
/// - `codex-cli 0.144.5` → `0.144.5`
/// - `2.1.220 (Claude Code)` → `2.1.220`
/// - `grok 0.2.118 (1e1687c1cf)` → `0.2.118`
/// - `0.83.0` → `0.83.0`
///
/// If no digit-leading token is found, returns the trimmed original line.
pub(crate) fn extract_version_token(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // Split on whitespace and parentheses so "2.1.220 (Claude Code)" yields "2.1.220".
    let token = s
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .find(|p| {
            let p = p.trim_start_matches(['v', 'V']);
            p.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .unwrap_or(s);
    let token = token.trim_start_matches(['v', 'V']);
    let cleaned = token
        .trim_matches(|c: char| c == ',' || c == ';' || c == ')' || c == '(')
        .to_string();
    if cleaned.chars().any(|c| c.is_ascii_digit()) {
        cleaned
    } else {
        s.to_string()
    }
}

/// Binary was resolved on disk — always `Installed`. Version probe timeout stays
/// Installed with empty version + note (never map timeout → NotFound).
fn finish_detect(
    agent: AgentId,
    path: std::path::PathBuf,
    version_args: &[&str],
    channel_hint: Option<&str>,
    env_ready: bool,
    via_well_known: bool,
    extra_env: &[(String, String)],
) -> DetectResult {
    use crate::models::DetectStatus;
    use crate::utils::process::{run_capture_with_env, stdout_first_line};

    let mut notes = Vec::new();
    if via_well_known {
        notes.push(format!(
            "found via well-known path (not on process PATH): {}; \
             restart AgentHub after installs if PATH still incomplete",
            path.display()
        ));
    }

    let version = match run_capture_with_env(&path, version_args, extra_env) {
        Ok(o) => {
            if o.status.success() {
                stdout_first_line(&o)
                    .filter(|l| looks_like_version_line(l))
                    .map(|l| extract_version_token(&l))
                    .filter(|l| !l.is_empty())
            } else {
                // Some CLIs print version on stderr; never treat shell/PATH errors as a version.
                let err = String::from_utf8_lossy(&o.stderr);
                let candidate = err
                    .lines()
                    .next()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && looks_like_version_line(l))
                    .map(|l| extract_version_token(&l))
                    .filter(|l| !l.is_empty());
                if candidate.is_none() {
                    let out = String::from_utf8_lossy(&o.stdout);
                    let hint = err
                        .lines()
                        .chain(out.lines())
                        .next()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty());
                    if let Some(h) = hint {
                        let safe = crate::utils::redact::redact_text(h);
                        notes.push(format!(
                            "version probe failed (binary present at {}): {safe}",
                            path.display()
                        ));
                        tracing::debug!(
                            target: crate::logging::targets::DETECT,
                            module = crate::logging::targets::DETECT,
                            op = "version_probe",
                            agent = agent.as_str(),
                            path = %path.display(),
                            "version probe non-zero: {safe}"
                        );
                    }
                }
                candidate
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            notes.push(format!(
                "version probe timed out (binary present at {})",
                path.display()
            ));
            tracing::warn!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "version_probe",
                agent = agent.as_str(),
                path = %path.display(),
                "version probe timed out (binary still counted as Installed)"
            );
            None
        }
        Err(e) => {
            let err_msg = redact_text(&e.to_string());
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "version_probe",
                agent = agent.as_str(),
                path = %path.display(),
                error = %err_msg,
                "version probe spawn/io failed"
            );
            None
        }
    };

    let channel = channel_hint.map(|s| {
        if s == "npm" || s == "native" {
            s.to_string()
        } else {
            infer_channel(&path, Some(s))
        }
    });

    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "finish_detect",
        agent = agent.as_str(),
        channel = channel.as_deref().unwrap_or("-"),
        version = version.as_deref().unwrap_or("-"),
        via_well_known,
        env_ready,
        path = %path.display(),
        "agent marked Installed"
    );

    DetectResult {
        agent,
        status: DetectStatus::Installed,
        version,
        binary_path: Some(path),
        channel,
        env_ready,
        notes,
        extra_copies: Vec::new(),
    }
}
