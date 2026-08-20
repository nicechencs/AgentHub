//! Node.js / npm / PowerShell / Git detection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use which::which;

use crate::models::{EnvStatus, EnvStatusKind, RuntimeId};
use crate::utils::process::{run_capture, stdout_first_line};

use crate::catalog::limits::{NODE_MIN_MAJOR, PI_NODE_MIN_MAJOR, PI_NODE_MIN_MINOR};

/// Detect note / update-check copy when Pi is installed but Node < 22.
pub const PI_NODE_TOO_OLD_NOTE: &str =
    "Node too old: Pi requires Node.js >= 22 (engines.node >= 22.19.0)";

/// A Node binary that satisfied [`resolve_node_at_least`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNode {
    pub path: PathBuf,
    pub version: String,
    pub major: u64,
}

impl ResolvedNode {
    pub fn bin_dir(&self) -> Option<PathBuf> {
        self.path.parent().map(Path::to_path_buf)
    }
}

pub fn detect_nodejs() -> EnvStatus {
    match resolve_binary(&["node", "node.exe"]) {
        Some(path) => match run_capture(&path, &["-v"]) {
            Ok(out) if out.status.success() => {
                let version =
                    stdout_first_line(&out).map(|v| v.trim_start_matches('v').to_string());
                let status = match version.as_deref().and_then(parse_major) {
                    Some(major) if major >= NODE_MIN_MAJOR => EnvStatusKind::Ok,
                    Some(_) => EnvStatusKind::Outdated,
                    None => EnvStatusKind::BrokenPath,
                };
                EnvStatus {
                    id: RuntimeId::NodeJs,
                    status,
                    version,
                    path: Some(path),
                    min_required: Some(format!(">={NODE_MIN_MAJOR}")),
                    remediation: None,
                    notes: vec![],
                }
            }
            _ => EnvStatus {
                id: RuntimeId::NodeJs,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: Some(format!(">={NODE_MIN_MAJOR}")),
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::NodeJs,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: Some(format!(">={NODE_MIN_MAJOR}")),
            remediation: None,
            notes: vec![],
        },
    }
}

pub fn detect_npm() -> EnvStatus {
    match resolve_binary(&["npm", "npm.cmd", "npm.exe"]) {
        Some(path) => match run_capture(&path, &["-v"]) {
            Ok(out) if out.status.success() => EnvStatus {
                id: RuntimeId::Npm,
                status: EnvStatusKind::Ok,
                version: stdout_first_line(&out),
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
            _ => EnvStatus {
                id: RuntimeId::Npm,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::Npm,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![],
        },
    }
}

/// Detect PowerShell availability.
///
/// - **Windows**: probes Windows PowerShell 5.1 (`powershell`) and PowerShell 7+ (`pwsh`)
///   separately; either is enough for `RuntimeId::PowerShell` readiness (native install scripts).
///   Prefer `pwsh` as the primary path/version when both work.
/// - **macOS / Linux**: PowerShell is not a shared runtime.  Doctor skips it via
///   [`super::detect::host_runtimes`]; this function only remains for explicit
///   Windows-only native install resolution and tests.
pub fn detect_powershell() -> EnvStatus {
    #[cfg(not(windows))]
    {
        return EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Ok,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![
                "Windows PowerShell 5.1: not applicable on this platform".into(),
                "PowerShell 7 (pwsh): not required (native installers use bash/sh)".into(),
            ],
        };
    }

    #[cfg(windows)]
    {
        let mut notes = Vec::new();
        let mut last_broken: Option<PathBuf> = None;

        let ps51 = probe_powershell_candidate(
            &["powershell", "powershell.exe"],
            Some(PathBuf::from(
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            )),
        );

        let pwsh = probe_powershell_candidate(&["pwsh", "pwsh.exe"], None);

        match &ps51 {
            PsProbe::Ok { path, version } => {
                notes.push(format!(
                    "Windows PowerShell 5.1: {} @ {}",
                    version.as_deref().unwrap_or("ok"),
                    path.display()
                ));
            }
            PsProbe::Broken { path } => {
                notes.push(format!(
                    "Windows PowerShell 5.1: broken @ {}",
                    path.display()
                ));
                last_broken = Some(path.clone());
            }
            PsProbe::Missing => {
                notes.push("Windows PowerShell 5.1: missing".into());
            }
        }

        match &pwsh {
            PsProbe::Ok { path, version } => {
                notes.push(format!(
                    "PowerShell 7 (pwsh): {} @ {}",
                    version.as_deref().unwrap_or("ok"),
                    path.display()
                ));
            }
            PsProbe::Broken { path } => {
                notes.push(format!("PowerShell 7 (pwsh): broken @ {}", path.display()));
                last_broken = Some(path.clone());
            }
            PsProbe::Missing => {
                notes.push("PowerShell 7 (pwsh): missing".into());
            }
        }

        // Aggregate readiness: any working interpreter is enough for native channel.
        // Prefer pwsh as the reported path/version for install execution affinity.
        let status = if let PsProbe::Ok { path, version } = &pwsh {
            EnvStatus {
                id: RuntimeId::PowerShell,
                status: EnvStatusKind::Ok,
                version: version.clone(),
                path: Some(path.clone()),
                min_required: None,
                remediation: None,
                notes,
            }
        } else if let PsProbe::Ok { path, version } = &ps51 {
            EnvStatus {
                id: RuntimeId::PowerShell,
                status: EnvStatusKind::Ok,
                version: version.clone(),
                path: Some(path.clone()),
                min_required: None,
                remediation: None,
                notes,
            }
        } else if let Some(path) = last_broken {
            EnvStatus {
                id: RuntimeId::PowerShell,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: None,
                remediation: None,
                notes,
            }
        } else {
            EnvStatus {
                id: RuntimeId::PowerShell,
                status: EnvStatusKind::Missing,
                version: None,
                path: None,
                min_required: None,
                remediation: None,
                notes,
            }
        };

        for n in &status.notes {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect_powershell",
                status = ?status.status,
                "{n}"
            );
        }
        if status.status != EnvStatusKind::Ok {
            tracing::info!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect_powershell",
                status = ?status.status,
                path = %status
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "-".into()),
                "PowerShell runtime not fully ready (5.1 and/or 7 may be missing)"
            );
        }

        status
    } // #[cfg(windows)]
}

/// Windows-only dual-version probe helpers.  On macOS/Linux `detect_powershell`
/// short-circuits without spawning interpreters.
#[cfg(windows)]
#[derive(Clone)]
enum PsProbe {
    Ok {
        path: PathBuf,
        version: Option<String>,
    },
    Broken {
        path: PathBuf,
    },
    Missing,
}

#[cfg(windows)]
fn probe_powershell_candidate(names: &[&str], fallback: Option<PathBuf>) -> PsProbe {
    if let Some(path) = resolve_binary(names) {
        return probe_ps_path(&path);
    }
    if let Some(fb) = fallback {
        if fb.is_file() {
            return probe_ps_path(&fb);
        }
    }
    PsProbe::Missing
}

#[cfg(windows)]
fn probe_ps_path(path: &Path) -> PsProbe {
    match run_capture(
        path,
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    ) {
        Ok(out) if out.status.success() => PsProbe::Ok {
            path: path.to_path_buf(),
            version: stdout_first_line(&out),
        },
        _ => PsProbe::Broken {
            path: path.to_path_buf(),
        },
    }
}

/// Resolve the first existing binary from PATH or supported platform fallbacks.
///
/// Keep all runtime detection and install execution on this resolver: GUI-launched
/// macOS applications often lack Homebrew in PATH, while an absolute npm binary
/// under `/opt/homebrew/bin` or `/usr/local/bin` remains executable.
pub fn resolve_binary(names: &[&str]) -> Option<PathBuf> {
    for name in names {
        if let Ok(p) = which(name) {
            return Some(p);
        }
    }
    // GUI-launched macOS apps often inherit a minimal PATH that omits the
    // Homebrew prefix. Probe both supported Homebrew locations directly so a
    // freshly installed runtime is visible without restarting the shell.
    for candidate in platform_binary_candidates(names) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Well-known non-PATH binary locations for the current platform.
///
/// Kept as a pure helper so path coverage is testable without mutating PATH or
/// requiring Homebrew to be installed on the test host.
fn platform_binary_candidates(names: &[&str]) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return homebrew_binary_candidates(names);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = names;
        Vec::new()
    }
}

/// Homebrew binary locations, kept platform-independent for deterministic tests.
#[cfg(any(target_os = "macos", test))]
fn homebrew_binary_candidates(names: &[&str]) -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin"]
        .into_iter()
        .flat_map(|prefix| {
            names
                .iter()
                .map(move |name| PathBuf::from(prefix).join(name))
        })
        .collect()
}

/// Detect Git CLI (`git --version`).
///
/// Needed by Skills market / `skill install <git-url>` (`git clone` / `git pull`).
/// Not an Agent install-channel hard dependency, but listed as a shared runtime
/// so doctor / Agents env bar can detect and guide install.
pub fn detect_git() -> EnvStatus {
    match resolve_binary(&["git", "git.exe"]) {
        Some(path) => match run_capture(&path, &["--version"]) {
            Ok(out) if out.status.success() => {
                let raw = stdout_first_line(&out);
                let version = raw.as_deref().map(parse_git_version);
                EnvStatus {
                    id: RuntimeId::Git,
                    status: EnvStatusKind::Ok,
                    version,
                    path: Some(path),
                    min_required: None,
                    remediation: None,
                    notes: vec![],
                }
            }
            _ => EnvStatus {
                id: RuntimeId::Git,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::Git,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![],
        },
    }
}

/// Prefer PowerShell 7 (`pwsh`), else Windows PowerShell 5.1.
/// Used by native install script runner — logs should include the chosen path.
pub fn resolve_powershell_for_native() -> Option<PathBuf> {
    // Prefer 7 for modern script compatibility.
    if let Some(p) = resolve_binary(&["pwsh", "pwsh.exe"]) {
        return Some(p);
    }
    if let Some(p) = resolve_binary(&["powershell", "powershell.exe"]) {
        return Some(p);
    }
    #[cfg(windows)]
    {
        let fallback = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        if fallback.is_file() {
            return Some(fallback);
        }
    }
    None
}

fn parse_major(version: &str) -> Option<u64> {
    let major = version.split('.').next()?;
    major.parse().ok()
}

/// Discover a Node.js binary with `major >= min_major` (minor `0`).
///
/// Search order: every PATH `node`/`node.exe`, then versioned manager /
/// well-known trees. First candidate whose `node -v` meets the floor wins.
/// Global doctor still uses [`NODE_MIN_MAJOR`] (18) and first-PATH `which`.
pub fn resolve_node_at_least(min_major: u64) -> Option<ResolvedNode> {
    resolve_node_meeting(min_major, 0)
}

/// Pi probe + Chat: Node `>=22.19.0` if present anywhere we know how to look.
pub fn resolve_pi_node() -> Option<ResolvedNode> {
    resolve_node_meeting(PI_NODE_MIN_MAJOR, PI_NODE_MIN_MINOR)
}

fn resolve_node_meeting(min_major: u64, min_minor: u64) -> Option<ResolvedNode> {
    let home = crate::utils::paths::home_dir().ok();
    resolve_node_at_least_from(
        live_path_and_platform_nodes(),
        home.as_deref(),
        min_major,
        min_minor,
        NodeManagerRoots::from_env_and_home(home.as_deref()),
        probe_node_version,
    )
}

/// `PATH=<bin_dir>:$PATH` (or `;` on Windows) so child processes see Node 22 first.
pub fn path_with_prefixed_bin(bin_dir: &Path, current_path: &str) -> String {
    let prefix = bin_dir.to_string_lossy();
    if current_path.is_empty() {
        return prefix.into_owned();
    }
    format!("{prefix}{sep}{current_path}", sep = path_list_sep())
}

/// Extra env so a child process prefers `bin_dir` over the inherited PATH.
pub fn prefixed_path_env(bin_dir: Option<&Path>) -> Vec<(String, String)> {
    let Some(dir) = bin_dir else {
        return Vec::new();
    };
    let current = std::env::var("PATH").unwrap_or_default();
    vec![("PATH".into(), path_with_prefixed_bin(dir, &current))]
}

/// Well-known Node 22+ locations under `home` (no env, so tests stay hermetic).
pub fn node_versioned_home_candidates(home: &Path, min_major: u64) -> Vec<PathBuf> {
    node_versioned_candidates(
        Some(home),
        min_major,
        &NodeManagerRoots::from_home_only(home),
    )
}

/// Testable resolver: every PATH node first, then versioned home / manager trees.
pub fn resolve_node_at_least_from(
    path_nodes: impl IntoIterator<Item = PathBuf>,
    home: Option<&Path>,
    min_major: u64,
    min_minor: u64,
    roots: NodeManagerRoots,
    mut probe: impl FnMut(&Path) -> Option<(String, u64, u64)>,
) -> Option<ResolvedNode> {
    let mut seen = HashSet::new();
    let mut candidates: Vec<PathBuf> = path_nodes.into_iter().collect();
    candidates.extend(node_versioned_candidates(home, min_major, &roots));
    for path in candidates {
        if !seen.insert(path.clone()) {
            continue;
        }
        if let Some((version, major, minor)) = probe(&path) {
            if version_meets(major, minor, min_major, min_minor) {
                return Some(ResolvedNode {
                    path,
                    version,
                    major,
                });
            }
        }
    }
    None
}

/// Roots for nvm / fnm / volta / n / nvm-windows / Program Files.
/// Live detection merges env; tests pass home-only (unix trees, no Windows roots).
#[derive(Debug, Clone, Default)]
pub struct NodeManagerRoots {
    pub nvm_dir: Option<PathBuf>,
    /// nvm-windows root (`{NVM_HOME}\v22.x.x\node.exe`), not `versions/node`.
    pub nvm_home: Option<PathBuf>,
    pub fnm_dirs: Vec<PathBuf>,
    pub volta_home: Option<PathBuf>,
    pub n_prefix: Option<PathBuf>,
    /// Concrete `node.exe` files (Program Files).
    pub system_nodes: Vec<PathBuf>,
}

impl NodeManagerRoots {
    /// Unix-style trees only so unit tests stay hermetic.
    pub fn from_home_only(home: &Path) -> Self {
        Self {
            nvm_dir: Some(home.join(".nvm")),
            nvm_home: None,
            fnm_dirs: vec![
                home.join(".local").join("share").join("fnm"),
                home.join(".fnm"),
            ],
            volta_home: Some(home.join(".volta")),
            n_prefix: Some(home.join("n")),
            system_nodes: Vec::new(),
        }
    }

    pub fn from_env_and_home(home: Option<&Path>) -> Self {
        let mut roots = home.map(Self::from_home_only).unwrap_or_default();
        if let Some(v) = nonempty_env("NVM_DIR") {
            roots.nvm_dir = Some(v);
        }
        if let Some(v) = nonempty_env("NVM_HOME") {
            roots.nvm_home = Some(v);
        }
        if let Some(v) = nonempty_env("FNM_DIR") {
            roots.fnm_dirs.insert(0, v);
        }
        if let Some(v) = nonempty_env("VOLTA_HOME") {
            roots.volta_home = Some(v);
        }
        if let Some(v) = nonempty_env("N_PREFIX") {
            roots.n_prefix = Some(v);
        }
        #[cfg(windows)]
        {
            apply_windows_manager_fallbacks(&mut roots);
        }
        roots
    }
}

fn nonempty_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

#[cfg(windows)]
fn apply_windows_manager_fallbacks(roots: &mut NodeManagerRoots) {
    if roots.nvm_home.is_none() {
        if let Some(appdata) = nonempty_env("APPDATA") {
            let candidate = appdata.join("nvm");
            if candidate.is_dir() {
                roots.nvm_home = Some(candidate);
            }
        }
    }
    append_existing_dir(
        &mut roots.fnm_dirs,
        nonempty_env("LOCALAPPDATA").map(|p| p.join("fnm")),
    );
    append_existing_dir(
        &mut roots.fnm_dirs,
        nonempty_env("APPDATA").map(|p| p.join("fnm")),
    );
    if roots.volta_home.is_none() {
        if let Some(local) = nonempty_env("LOCALAPPDATA") {
            let candidate = local.join("Volta");
            if candidate.is_dir() {
                roots.volta_home = Some(candidate);
            }
        }
    }
    push_program_files_node(&mut roots.system_nodes, nonempty_env("ProgramFiles"));
    push_program_files_node(&mut roots.system_nodes, nonempty_env("ProgramFiles(x86)"));
}

#[cfg(windows)]
fn append_existing_dir(dirs: &mut Vec<PathBuf>, candidate: Option<PathBuf>) {
    let Some(path) = candidate else {
        return;
    };
    if path.is_dir() && !dirs.iter().any(|d| d == &path) {
        dirs.push(path);
    }
}

#[cfg(windows)]
fn push_program_files_node(out: &mut Vec<PathBuf>, prefix: Option<PathBuf>) {
    let Some(prefix) = prefix else {
        return;
    };
    let exe = prefix.join("nodejs").join("node.exe");
    if exe.is_file() {
        out.push(exe);
    }
}

/// PATH nodes in order, then Homebrew prefixes the GUI PATH often omits.
fn live_path_and_platform_nodes() -> Vec<PathBuf> {
    let mut out = path_node_binaries();
    for extra in platform_binary_candidates(node_bin_names()) {
        if extra.is_file() && !out.iter().any(|p| p == &extra) {
            out.push(extra);
        }
    }
    out
}

/// Every `node`/`node.exe` on PATH, in PATH order. `which` would hide later Node 22.
fn path_node_binaries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        for name in node_bin_names() {
            let candidate = dir.join(name);
            if candidate.is_file() && seen.insert(candidate.clone()) {
                out.push(candidate);
            }
        }
    }
    out
}

fn version_meets(major: u64, minor: u64, min_major: u64, min_minor: u64) -> bool {
    major > min_major || (major == min_major && minor >= min_minor)
}

fn node_versioned_candidates(
    home: Option<&Path>,
    min_major: u64,
    roots: &NodeManagerRoots,
) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(home) = home {
        // Official-ish unpacked trees: ~/.local/share/node-v22.19.0/bin/node
        push_version_dir_nodes(
            &mut out,
            &home.join(".local").join("share"),
            min_major,
            true,
        );
    }

    if let Some(nvm) = &roots.nvm_dir {
        push_version_dir_nodes(
            &mut out,
            &nvm.join("versions").join("node"),
            min_major,
            false,
        );
    }

    if let Some(nvm_home) = &roots.nvm_home {
        push_version_dir_nodes(&mut out, nvm_home, min_major, false);
    }

    for fnm in &roots.fnm_dirs {
        push_version_dir_nodes(&mut out, &fnm.join("node-versions"), min_major, false);
    }

    if let Some(volta) = &roots.volta_home {
        push_version_dir_nodes(
            &mut out,
            &volta.join("tools").join("image").join("node"),
            min_major,
            false,
        );
        push_node_in_bin_dir(&mut out, &volta.join("bin"));
    }

    if let Some(n_prefix) = &roots.n_prefix {
        push_node_in_bin_dir(&mut out, &n_prefix.join("bin"));
        push_version_dir_nodes(
            &mut out,
            &n_prefix.join("versions").join("node"),
            min_major,
            false,
        );
        push_version_dir_nodes(
            &mut out,
            &n_prefix.join("n").join("versions").join("node"),
            min_major,
            false,
        );
    }
    // System `n` default prefix (not under $HOME).
    push_version_dir_nodes(
        &mut out,
        Path::new("/usr/local/n/versions/node"),
        min_major,
        false,
    );

    out.extend(roots.system_nodes.iter().cloned());
    out
}

fn push_version_dir_nodes(
    out: &mut Vec<PathBuf>,
    parent: &Path,
    min_major: u64,
    require_node_v_prefix: bool,
) {
    let Ok(rd) = std::fs::read_dir(parent) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter(|p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if require_node_v_prefix && !name.starts_with("node-v") {
                return false;
            }
            dir_name_major(name).is_some_and(|m| m >= min_major)
        })
        .collect();
    // Filename order ranks v22.9 above v22.10; parsed semver does not.
    dirs.sort_by(|a, b| dir_semver(b).cmp(&dir_semver(a)));
    for dir in dirs {
        push_nodes_in_version_dir(out, &dir);
    }
}

/// Each manager uses a different bin layout under the version dir.
fn push_nodes_in_version_dir(out: &mut Vec<PathBuf>, dir: &Path) {
    push_node_in_bin_dir(out, dir);
    push_node_in_bin_dir(out, &dir.join("bin"));
    push_node_in_bin_dir(out, &dir.join("installation"));
    push_node_in_bin_dir(out, &dir.join("installation").join("bin"));
}

fn push_node_in_bin_dir(out: &mut Vec<PathBuf>, bin_dir: &Path) {
    for name in node_bin_names() {
        let candidate = bin_dir.join(name);
        if candidate.is_file() {
            out.push(candidate);
        }
    }
}

fn node_bin_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["node.exe", "node"]
    } else {
        &["node"]
    }
}

fn dir_name_major(name: &str) -> Option<u64> {
    parse_dir_semver(name).map(|(m, _, _)| m)
}

fn dir_semver(path: &Path) -> (u64, u64, u64) {
    path.file_name()
        .and_then(|s| s.to_str())
        .and_then(parse_dir_semver)
        .unwrap_or((0, 0, 0))
}

fn parse_dir_semver(name: &str) -> Option<(u64, u64, u64)> {
    let s = name.strip_prefix("node-").unwrap_or(name);
    let s = s.trim_start_matches('v').trim_start_matches('V');
    parse_semver_parts(s)
}

fn parse_semver_parts(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(parse_numeric_prefix).unwrap_or(0);
    let patch = parts.next().and_then(parse_numeric_prefix).unwrap_or(0);
    Some((major, minor, patch))
}

fn parse_numeric_prefix(s: &str) -> Option<u64> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    s[..end].parse().ok()
}

fn path_list_sep() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

fn probe_node_version(path: &Path) -> Option<(String, u64, u64)> {
    let out = run_capture(path, &["-v"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let version = stdout_first_line(&out)?.trim_start_matches('v').to_string();
    let (major, minor, _) = parse_semver_parts(&version)?;
    Some((version, major, minor))
}

/// `git version 2.43.0.windows.1` → `2.43.0.windows.1`
fn parse_git_version(line: &str) -> String {
    let trimmed = line.trim();
    const PREFIX: &str = "git version ";
    if let Some(rest) = trimmed
        .strip_prefix(PREFIX)
        .or_else(|| trimmed.strip_prefix("git version"))
    {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
#[path = "nodejs/tests.rs"]
mod tests;
