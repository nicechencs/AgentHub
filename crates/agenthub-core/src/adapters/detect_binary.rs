//! Shared binary detection for production adapters.

use std::path::{Path, PathBuf};

use crate::models::{AgentId, DetectResult};
use crate::utils::redact::redact_text;

/// Shared detect helper used by adapters.
///
/// Platform matrix:
/// - PATH / `which` with Win suffixes (`.cmd` / `.exe` / bare)
/// - well-known install dirs that do **not** require the GUI process PATH
///   (Tauri often inherits a stale PATH after installs)
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

    // AgentHub user npm prefix first — PATH may still point at a leftover
    // 0.2.3 global shim and would otherwise hide ~/.agenthub/npm.
    if let Some(path) = first_existing_named_bin(&agenthub_user_npm_bin_dirs(), &names) {
        tracing::debug!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect_binary",
            agent = agent.as_str(),
            via = "user_npm_prefix",
            channel = "npm",
            path = %path.display(),
            "agent binary resolved in AgentHub user npm prefix"
        );
        return finish_detect(
            agent,
            path,
            version_args,
            Some("npm"),
            env_ready,
            true,
            extra_env,
        );
    }

    for name in &names {
        if let Ok(path) = which(name) {
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

    // Remaining well-known dirs (legacy global npm, ~/.local/bin, …).
    // User-prefix hits were already considered above.
    for (path, channel) in well_known_bin_paths(agent) {
        if is_under_agenthub_user_npm_prefix(&path) {
            continue;
        }
        if path.is_file() {
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
    }
}

/// Surfaced in DetectResult.notes and searchable in doctor / GUI when binary is missing.
pub(crate) const NOT_FOUND_FIREFIGHTING_NOTE: &str =
    "未在 PATH 或常见安装目录中找到该命令。若刚完成安装，请完全退出并重启 AgentHub。";

/// Expand a base command name with platform-typical suffixes.
pub(crate) fn expand_binary_names(base: &str) -> Vec<String> {
    let mut out = vec![base.to_string()];
    if cfg!(windows)
        && !base.ends_with(".cmd")
        && !base.ends_with(".exe")
        && !base.ends_with(".ps1")
    {
        // npm global shims are often `name.cmd`; native bins are `name.exe`.
        out.push(format!("{base}.cmd"));
        out.push(format!("{base}.exe"));
        out.push(format!("{base}.ps1"));
    }
    out
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
        #[cfg(windows)]
        {
            paths.push((dir.join(format!("{name}.cmd")), "npm"));
            paths.push((dir.join(format!("{name}.ps1")), "npm"));
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
        }
        AgentId::Codex => {
            // Codex is primarily npm; native install may also land under ~/.local/bin.
            for npm_dir in npm_global_bin_dirs(&home) {
                push_npm(&mut paths, npm_dir);
            }
            push_native(&mut paths, home.join(".local").join("bin"));
            // Some native/codex layouts
            push_native(&mut paths, home.join(".codex").join("bin"));
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

/// AgentHub-managed npm prefix roots (`<data>/npm` and `~/.agenthub/npm`).
///
/// These are **not** the OS/global npm prefix (AppData\Roaming\npm, /usr/local).
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
pub(crate) fn first_existing_named_bin(dirs: &[PathBuf], names: &[String]) -> Option<PathBuf> {
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn npm_global_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = agenthub_user_npm_bin_dirs();
    let home_fallback = npm_prefix_to_bin_dir(home.join(".agenthub").join("npm"));
    if !dirs.iter().any(|p| p == &home_fallback) {
        dirs.push(home_fallback);
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("npm"));
        }
        // Fallback when APPDATA is missing (rare): classic Roaming path under home.
        dirs.push(home.join("AppData").join("Roaming").join("npm"));
    }
    #[cfg(not(windows))]
    {
        // Common npm global bin locations on macOS/Linux (PATH may omit them in GUI).
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        dirs.push(home.join(".local").join("bin"));
        // nvm default current
        if let Ok(nvm) = std::env::var("NVM_DIR") {
            let nvm_dir = PathBuf::from(nvm);
            if let Ok(rd) = std::fs::read_dir(nvm_dir.join("versions").join("node")) {
                // Prefer newest version dir by name (v22.x > v18.x).
                let mut versions: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();
                if let Some(latest) = versions.pop() {
                    dirs.push(latest.join("bin"));
                }
            }
        }
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
    }
}
