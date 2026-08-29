use super::auth_revision::AuthCredentialMetadata;
use super::codex_copies::ide_codex_bins_under;
#[cfg(not(windows))]
use super::detect_binary::well_known_npm_cli_dirs;
use super::detect_binary::{
    agenthub_user_npm_prefix_roots, attach_extra_binary_copies, detect_binary, expand_binary_names,
    first_existing_named_bin, infer_channel, is_under_agenthub_user_npm_prefix,
    npm_global_bin_dirs, npm_prefix_stdout_to_bin_dir, parse_npmrc_global_prefix,
    user_writable_npm_bin_dir, user_writable_npm_prefix, well_known_bin_paths,
    NOT_FOUND_FIREFIGHTING_NOTE,
};
use super::*;
use crate::error::AppError;
use crate::models::{
    AccountKind, AgentConfig, AgentId, Capability, CapabilityLevel, DetectResult, DetectStatus,
    DetectedBinaryCopy,
};
use crate::utils::atomic::atomic_write;
use serde_json::json;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

// Serializes tests that mutate HOME / PATH / AGENTHUB_HOME.
static DETECT_ENV_LOCK: Mutex<()> = Mutex::new(());

fn restore_env(key: &str, prev: Option<OsString>) {
    match prev {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

#[test]
fn expand_binary_names_adds_windows_suffixes() {
    let names = expand_binary_names("codex");
    assert!(names.iter().any(|n| n == "codex"));
    #[cfg(windows)]
    {
        assert_eq!(names[0], "codex.cmd");
        assert!(names.iter().any(|n| n == "codex.exe"));
        assert!(
            names.iter().all(|n| !n.ends_with(".ps1")),
            ".ps1 must not be probed: CreateProcess cannot spawn PowerShell scripts and is_direct_spawnable rejects them: {names:?}"
        );
        let cmd = names.iter().position(|n| n == "codex.cmd").unwrap();
        let bare = names.iter().position(|n| n == "codex").unwrap();
        assert!(
            cmd < bare,
            "Windows must probe .cmd before the Unix shebang shim: {names:?}"
        );
    }
}

#[cfg(windows)]
#[test]
fn first_existing_named_bin_skips_unix_shebang_prefers_cmd() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("npm");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("codex"), "#!/bin/sh\necho 0.1.0\n").unwrap();
    std::fs::write(dir.join("codex.cmd"), "@echo 0.149.0\n").unwrap();

    let found = first_existing_named_bin(&[dir.clone()], &expand_binary_names("codex"));
    assert_eq!(
        found,
        Some(dir.join("codex.cmd")),
        "must not pick the Unix shebang `codex` that CreateProcess cannot run"
    );
}

#[test]
fn well_known_paths_include_codex_npm_and_native_homes() {
    let paths = well_known_bin_paths(AgentId::Codex);
    assert!(!paths.is_empty(), "Codex must have well-known fallbacks");
    assert!(
        paths.iter().any(|(_, ch)| *ch == "npm"),
        "Codex should probe npm global bin"
    );
    let kimi = well_known_bin_paths(AgentId::Kimi);
    assert!(kimi
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains("kimi-code")));
    let grok = well_known_bin_paths(AgentId::Grok);
    assert!(grok
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains(".grok")));
}

#[test]
fn well_known_paths_include_claude_native_and_legacy_local() {
    let paths = well_known_bin_paths(AgentId::Claude);
    assert!(
        paths.iter().any(|(p, ch)| {
            *ch == "native"
                && p.to_string_lossy()
                    .replace('\\', "/")
                    .contains(".local/bin/claude")
        }),
        "Claude native ~/.local/bin missing: {paths:?}"
    );
    assert!(
        paths.iter().any(|(_, ch)| *ch == "npm"),
        "Claude should probe npm global bin"
    );
    assert!(
        paths.iter().any(|(p, ch)| {
            *ch == "npm"
                && p.to_string_lossy()
                    .replace('\\', "/")
                    .contains(".claude/local")
        }),
        "Claude legacy ~/.claude/local missing: {paths:?}"
    );
}

fn write_spawnable_probe(path: &std::path::Path, version: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    #[cfg(windows)]
    std::fs::write(path, format!("@echo {version}\r\n")).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, format!("#!/bin/sh\necho {version}\n")).unwrap();
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o755)).unwrap();
    }
}

#[test]
fn attach_extra_binary_copies_skips_primary_leftover_and_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let primary = tmp.path().join("primary.cmd");
    #[cfg(not(windows))]
    let primary = tmp.path().join("primary");
    #[cfg(windows)]
    let extra = tmp.path().join("extra.cmd");
    #[cfg(not(windows))]
    let extra = tmp.path().join("extra");
    #[cfg(windows)]
    let leftover = tmp.path().join("leftover.cmd");
    #[cfg(not(windows))]
    let leftover = tmp.path().join("leftover");
    write_spawnable_probe(&primary, "1.0.0");
    write_spawnable_probe(&extra, "2.0.0");
    write_spawnable_probe(&leftover, "9.9.9");

    let mut result = DetectResult {
        agent: AgentId::Claude,
        status: DetectStatus::Installed,
        version: Some("1.0.0".into()),
        binary_path: Some(primary.clone()),
        channel: Some("native".into()),
        env_ready: true,
        notes: Vec::new(),
        extra_copies: vec![DetectedBinaryCopy::from_kind(
            AgentId::Claude,
            leftover.clone(),
            "leftover-agenthub",
            Some("9.9.9".into()),
            Some("npm".into()),
        )],
    };
    attach_extra_binary_copies(
        &mut result,
        vec![
            (primary.clone(), "native"),
            (extra.clone(), "npm"),
            (extra.clone(), "npm"),
            (leftover.clone(), "npm"),
        ],
        &["--version"],
        &[],
    );

    assert_eq!(
        result.binary_path.as_deref(),
        Some(primary.as_path()),
        "spawn target must stay the primary"
    );
    assert_eq!(
        result.extra_copies.len(),
        2,
        "leftover kept + one extra npm, no primary/dupes: {:?}",
        result.extra_copies
    );
    let extra_row = result
        .extra_copies
        .iter()
        .find(|c| c.kind == "npm")
        .expect("npm extra copy");
    assert_eq!(extra_row.path, extra);
    assert_eq!(extra_row.channel.as_deref(), Some("npm"));
    if let Some(v) = extra_row.version.as_deref() {
        assert_eq!(v, "2.0.0");
    }
    assert!(
        result
            .notes
            .iter()
            .any(|n| n.contains("另有 1 份 Claude Code")),
        "channel extra note must name the agent and skip leftover: {:?}",
        result.notes
    );
}

#[test]
fn attach_extra_binary_copies_refreshes_note_when_ide_copy_is_added() {
    let tmp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let npm = tmp.path().join("npm.cmd");
    #[cfg(not(windows))]
    let npm = tmp.path().join("npm-copy");
    #[cfg(windows)]
    let ide = tmp.path().join("ide.exe");
    #[cfg(not(windows))]
    let ide = tmp.path().join("ide-copy");
    write_spawnable_probe(&npm, "0.1.0");
    write_spawnable_probe(&ide, "0.2.0");

    let mut result = DetectResult {
        agent: AgentId::Codex,
        status: DetectStatus::Installed,
        version: Some("0.1.0".into()),
        binary_path: Some(tmp.path().join("primary-missing")),
        channel: Some("npm".into()),
        env_ready: true,
        notes: vec!["另有 1 份 Codex：stale".into()],
        extra_copies: vec![DetectedBinaryCopy::from_kind(
            AgentId::Codex,
            npm.clone(),
            "npm",
            Some("0.1.0".into()),
            Some("npm".into()),
        )],
    };
    attach_extra_binary_copies(&mut result, vec![(ide.clone(), "ide")], &["--version"], &[]);

    assert_eq!(result.extra_copies.len(), 2);
    assert_eq!(result.extra_copies[1].kind, "ide");
    assert!(result.extra_copies[1].channel.is_none());
    let extra_notes: Vec<_> = result
        .notes
        .iter()
        .filter(|n| n.starts_with("另有 "))
        .collect();
    assert_eq!(extra_notes.len(), 1, "{:?}", result.notes);
    assert!(
        extra_notes[0].contains("另有 2 份 Codex"),
        "second attach must rebuild one combined note: {:?}",
        result.notes
    );
}

#[test]
fn attach_extra_binary_copies_promotes_desktop_when_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let leftover = tmp.path().join("leftover.cmd");
    #[cfg(not(windows))]
    let leftover = tmp.path().join("leftover");
    #[cfg(windows)]
    let desktop = tmp.path().join("desktop.exe");
    #[cfg(not(windows))]
    let desktop = tmp.path().join("desktop-copy");
    #[cfg(windows)]
    let ide = tmp.path().join("ide.exe");
    #[cfg(not(windows))]
    let ide = tmp.path().join("ide-copy");
    write_spawnable_probe(&leftover, "9.9.9");
    write_spawnable_probe(&desktop, "0.50.0");
    write_spawnable_probe(&ide, "0.49.0");

    let mut result = DetectResult {
        agent: AgentId::Codex,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready: true,
        notes: vec![NOT_FOUND_FIREFIGHTING_NOTE.into()],
        extra_copies: vec![DetectedBinaryCopy::from_kind(
            AgentId::Codex,
            leftover.clone(),
            "leftover-agenthub",
            Some("9.9.9".into()),
            Some("npm".into()),
        )],
    };
    attach_extra_binary_copies(
        &mut result,
        vec![(ide.clone(), "ide"), (desktop.clone(), "desktop")],
        &["--version"],
        &[],
    );

    assert_eq!(result.status, DetectStatus::Installed);
    assert_eq!(result.binary_path.as_deref(), Some(desktop.as_path()));
    assert_eq!(result.channel.as_deref(), Some("desktop"));
    if let Some(v) = result.version.as_deref() {
        assert_eq!(v, "0.50.0");
    }
    assert!(
        !result
            .notes
            .iter()
            .any(|n| n == NOT_FOUND_FIREFIGHTING_NOTE),
        "PATH-miss firefighting note must drop after promote: {:?}",
        result.notes
    );
    assert!(
        result.notes.iter().any(|n| n.contains("desktop")),
        "promote note should name the desktop copy: {:?}",
        result.notes
    );
    assert_eq!(
        result.extra_copies.len(),
        2,
        "leftover + ide remain extra: {:?}",
        result.extra_copies
    );
    assert!(result
        .extra_copies
        .iter()
        .any(|c| c.kind == "leftover-agenthub" && c.path == leftover));
    assert!(result
        .extra_copies
        .iter()
        .any(|c| c.kind == "ide" && c.path == ide));
    assert!(result.extra_copies.iter().all(|c| c.path != desktop));
    let ide_copy = result
        .extra_copies
        .iter()
        .find(|c| c.kind == "ide")
        .expect("ide copy");
    assert_eq!(ide_copy.source, "ide");
    assert_eq!(ide_copy.update_via, "ide");
    assert_eq!(ide_copy.uninstall_via, "ide");
    let leftover_copy = result
        .extra_copies
        .iter()
        .find(|c| c.kind == "leftover-agenthub")
        .expect("leftover");
    assert_eq!(leftover_copy.uninstall_via, "leftover");
}

#[test]
fn install_lifecycle_aligns_all_agents() {
    use crate::models::install_lifecycle;
    for agent in AgentId::ALL {
        let npm = install_lifecycle(agent, "npm");
        assert_eq!(npm.source, "npm");
        assert_eq!(npm.update_via, "in_app");
        assert_eq!(npm.uninstall_via, "in_app");
        let native = install_lifecycle(agent, "native");
        assert_eq!(native.source, "native");
        if matches!(agent, AgentId::WorkBuddy | AgentId::Zcode) {
            assert_eq!(native.update_via, "official");
            assert_eq!(native.uninstall_via, "in_app");
        } else {
            assert_eq!(native.update_via, "in_app");
            assert_eq!(native.uninstall_via, "in_app");
        }
        let ide = install_lifecycle(agent, "ide");
        assert_eq!(
            (ide.source, ide.update_via, ide.uninstall_via),
            ("ide", "ide", "ide")
        );
        let desktop = install_lifecycle(agent, "desktop");
        assert_eq!(
            (desktop.source, desktop.update_via, desktop.uninstall_via),
            ("desktop", "desktop", "desktop")
        );
    }
}

#[test]
fn leftover_extra_copy_alone_does_not_count_as_installed() {
    let tmp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let leftover = tmp.path().join("leftover.cmd");
    #[cfg(not(windows))]
    let leftover = tmp.path().join("leftover");
    write_spawnable_probe(&leftover, "9.9.9");

    let mut result = DetectResult {
        agent: AgentId::Codex,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready: true,
        notes: vec![NOT_FOUND_FIREFIGHTING_NOTE.into()],
        extra_copies: vec![DetectedBinaryCopy::from_kind(
            AgentId::Codex,
            leftover.clone(),
            "leftover-agenthub",
            Some("9.9.9".into()),
            Some("npm".into()),
        )],
    };
    attach_extra_binary_copies(&mut result, Vec::new(), &["--version"], &[]);

    assert_eq!(result.status, DetectStatus::NotFound);
    assert!(result.binary_path.is_none());
    assert_eq!(result.extra_copies.len(), 1);
    assert_eq!(result.extra_copies[0].path, leftover);
}

#[test]
fn well_known_paths_cover_all_agents_non_empty() {
    for agent in AgentId::ALL {
        let paths = well_known_bin_paths(agent);
        assert!(
            !paths.is_empty(),
            "{} must have at least one well-known path",
            agent.as_str()
        );
        for (p, ch) in &paths {
            assert!(
                *ch == "npm" || *ch == "native",
                "channel must be npm|native, got {ch} for {}",
                p.display()
            );
        }
    }
}

#[test]
fn infer_channel_from_npm_and_native_paths() {
    #[cfg(windows)]
    {
        let npm = PathBuf::from(r"C:\Users\demo\AppData\Roaming\npm\codex.cmd");
        assert_eq!(infer_channel(&npm, None), "npm");
        let native = PathBuf::from(r"C:\Users\demo\.grok\bin\grok.exe");
        assert_eq!(infer_channel(&native, None), "native");
        let kimi = PathBuf::from(r"C:\Users\demo\.kimi-code\bin\kimi.exe");
        assert_eq!(infer_channel(&kimi, Some("native")), "native");
        let official =
            PathBuf::from(r"C:\Users\demo\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe");
        assert_eq!(infer_channel(&official, Some("npm")), "native");
    }
    #[cfg(not(windows))]
    {
        let native = PathBuf::from("/Users/demo/.grok/bin/grok");
        assert_eq!(infer_channel(&native, None), "native");
        let npm_global = PathBuf::from("/Users/demo/.npm-global/bin/codex");
        assert_eq!(infer_channel(&npm_global, None), "npm");
    }
}

#[cfg(unix)]
#[test]
fn infer_channel_follows_unix_npm_shim_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir
        .path()
        .join("nvm")
        .join("versions")
        .join("node")
        .join("v22.0.0")
        .join("lib")
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin")
        .join("codex.js");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "#!/usr/bin/env node\n").unwrap();
    // A generic ~/.local/bin shim looks native without following the link.
    let shim_dir = dir.path().join(".local").join("bin");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let shim = shim_dir.join("codex");
    symlink(&target, &shim).unwrap();

    assert_eq!(infer_channel(&shim, None), "npm");
}

#[test]
fn looks_like_version_line_accepts_cli_versions() {
    assert!(looks_like_version_line("2.1.220 (Claude Code)"));
    assert!(looks_like_version_line("codex-cli 0.144.5"));
    assert!(looks_like_version_line("0.29.1"));
    assert!(looks_like_version_line("grok 0.2.118 (1e1687c1cf)"));
    assert!(looks_like_version_line("claude"));
    // pi --version prints bare semver (measured: "0.83.0")
    assert!(looks_like_version_line("0.83.0"));
    assert!(looks_like_version_line("pi 0.83.0"));
}

#[test]
fn extract_version_token_strips_cli_name_noise() {
    assert_eq!(extract_version_token("codex-cli 0.144.5"), "0.144.5");
    assert_eq!(extract_version_token("2.1.220 (Claude Code)"), "2.1.220");
    assert_eq!(
        extract_version_token("grok 0.2.118 (1e1687c1cf)"),
        "0.2.118"
    );
    assert_eq!(extract_version_token("0.83.0"), "0.83.0");
    assert_eq!(extract_version_token("v1.2.3"), "1.2.3");
    assert_eq!(extract_version_token("pi 0.83.0"), "0.83.0");
}

#[test]
fn looks_like_version_line_rejects_shell_errors() {
    assert!(!looks_like_version_line(""));
    assert!(!looks_like_version_line(
        "'node' is not recognized as an internal or external command"
    ));
    assert!(!looks_like_version_line("command not found: node"));
    assert!(!looks_like_version_line("不是内部或外部命令"));
    assert!(!looks_like_version_line("系统找不到指定的文件"));
    // too long
    assert!(!looks_like_version_line(&"x".repeat(200)));
}

#[test]
fn not_found_firefighting_note_mentions_path_and_restart() {
    assert!(NOT_FOUND_FIREFIGHTING_NOTE.contains("PATH"));
    assert!(
        NOT_FOUND_FIREFIGHTING_NOTE.contains("常见安装目录")
            || NOT_FOUND_FIREFIGHTING_NOTE.contains("well-known")
    );
    assert!(
        NOT_FOUND_FIREFIGHTING_NOTE.contains("重启")
            || NOT_FOUND_FIREFIGHTING_NOTE.contains("restart")
    );
}

#[test]
fn first_existing_named_bin_prefers_earlier_user_prefix_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let user_prefix = tmp.path().join("agenthub").join("npm").join("bin");
    let legacy_global = tmp.path().join("usr").join("local").join("bin");
    std::fs::create_dir_all(&user_prefix).unwrap();
    std::fs::create_dir_all(&legacy_global).unwrap();
    std::fs::write(user_prefix.join("codex"), b"user-prefix").unwrap();
    std::fs::write(legacy_global.join("codex"), b"legacy-global").unwrap();

    let found = first_existing_named_bin(
        &[user_prefix.clone(), legacy_global.clone()],
        &["codex".into()],
    );
    assert_eq!(found, Some(user_prefix.join("codex")));
}

/// Leftover `~/.agenthub/npm` must not beat a PATH/`which` hit.
#[cfg(unix)]
#[test]
fn detect_binary_path_wins_over_leftover_agenthub_npm_prefix() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = DETECT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let path_dir = tmp.path().join("path-bin");
    let user_bin = data.join("npm").join("bin");
    std::fs::create_dir_all(&user_bin).unwrap();
    std::fs::create_dir_all(&path_dir).unwrap();

    let name = "agenthub-h4-probe";
    let user_probe = user_bin.join(name);
    let path_probe = path_dir.join(name);
    std::fs::write(&user_probe, "#!/bin/sh\necho 9.9.9\n").unwrap();
    std::fs::write(&path_probe, "#!/bin/sh\necho 1.0.0\n").unwrap();
    std::fs::set_permissions(&user_probe, PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&path_probe, PermissionsExt::from_mode(0o755)).unwrap();

    let prev_home = std::env::var_os("AGENTHUB_HOME");
    let prev_path = std::env::var_os("PATH");
    std::env::set_var("AGENTHUB_HOME", &data);
    let mut path = OsString::from(&path_dir);
    path.push(":");
    if let Some(rest) = &prev_path {
        path.push(rest);
    }
    std::env::set_var("PATH", &path);

    let found = std::panic::catch_unwind(|| {
        detect_binary(AgentId::Codex, &[name], &["--version"], None, true)
    });
    restore_env("AGENTHUB_HOME", prev_home);
    restore_env("PATH", prev_path);
    let result = found.expect("detect_binary must not panic");

    assert_eq!(
        result.status,
        crate::models::DetectStatus::Installed,
        "PATH probe must count as Installed: {:?}",
        result.notes
    );
    assert_eq!(
        result.binary_path.as_deref(),
        Some(path_probe.as_path()),
        "leftover AgentHub npm prefix {user_probe:?} must not shadow PATH/which hit"
    );
    assert_eq!(
        result.version.as_deref(),
        Some("1.0.0"),
        "version must come from the PATH binary, not leftover data-dir npm"
    );
    assert!(
        result
            .extra_copies
            .iter()
            .any(|c| c.kind == "leftover-agenthub"),
        "leftover data-dir npm must be listed as extra, not spawned: {:?}",
        result.extra_copies
    );
}

#[test]
fn well_known_scans_user_writable_npm_for_codex_pi_dsh() {
    let prefix = user_writable_npm_prefix().expect("user-writable npm prefix");
    assert!(
        !is_under_agenthub_user_npm_prefix(&prefix),
        "install prefix {prefix:?} must not be leftover ~/.agenthub/npm"
    );
    let bin = user_writable_npm_bin_dir().expect("user-writable npm bin dir");
    for agent in [AgentId::Codex, AgentId::Pi, AgentId::Dsh] {
        let paths = well_known_bin_paths(agent);
        assert!(
            paths
                .iter()
                .any(|(p, ch)| *ch == "npm" && p.starts_with(&bin)),
            "{} must scan user npm bin {} so install can redetect without restart: {paths:?}",
            agent.as_str(),
            bin.display()
        );
    }
}

#[test]
fn leftover_agenthub_npm_is_never_the_spawn_target() {
    let _guard = DETECT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    #[cfg(windows)]
    let leftover_dir = data.join("npm");
    #[cfg(not(windows))]
    let leftover_dir = data.join("npm").join("bin");
    std::fs::create_dir_all(&leftover_dir).unwrap();

    let name = "agenthub-leftover-only-probe";
    #[cfg(windows)]
    let leftover = leftover_dir.join(format!("{name}.cmd"));
    #[cfg(not(windows))]
    let leftover = leftover_dir.join(name);
    #[cfg(windows)]
    std::fs::write(&leftover, "@echo 9.9.9\r\n").unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&leftover, "#!/bin/sh\necho 9.9.9\n").unwrap();
        std::fs::set_permissions(&leftover, PermissionsExt::from_mode(0o755)).unwrap();
    }

    let prev_home = std::env::var_os("AGENTHUB_HOME");
    let prev_path = std::env::var_os("PATH");
    std::env::set_var("AGENTHUB_HOME", &data);
    // Leftover must not become the spawn target even when it is on PATH.
    std::env::set_var("PATH", &leftover_dir);

    let found = std::panic::catch_unwind(|| {
        // Dsh has no copy on this machine's PATH/npm global; Pi/Codex would
        // still hit `%APPDATA%\npm` via well-known dirs.
        detect_binary(AgentId::Dsh, &[name], &["--version"], None, true)
    });
    restore_env("AGENTHUB_HOME", prev_home);
    restore_env("PATH", prev_path);
    let result = found.expect("detect_binary must not panic");

    // A real dsh may legitimately live in npm-global well-known dirs on this
    // machine. The guarded invariant: the leftover data-dir copy is never the
    // spawn target and never comes from the agenthub data dir; when no real
    // install exists, the leftover alone must not count as installed.
    if let Some(target) = &result.binary_path {
        assert_ne!(
            target, &leftover,
            "leftover data-dir npm must not become the spawn target"
        );
        assert!(
            !target.starts_with(&data),
            "spawn target must not come from the agenthub data dir: {}",
            target.display()
        );
    } else {
        assert_eq!(
            result.status,
            crate::models::DetectStatus::NotFound,
            "leftover data-dir npm must not count as installed: {:?}",
            result.notes
        );
    }
    assert!(
        result
            .extra_copies
            .iter()
            .any(|c| c.kind == "leftover-agenthub" && c.path == leftover),
        "leftover must be extra_copies only: {:?}",
        result.extra_copies
    );
}

#[test]
fn is_under_agenthub_user_npm_prefix_excludes_legacy_global() {
    let roots = agenthub_user_npm_prefix_roots();
    assert!(
        !roots.is_empty(),
        "user npm prefix roots must include data-dir and/or ~/.agenthub/npm"
    );
    for root in &roots {
        let hit = if cfg!(windows) {
            root.join("codex.cmd")
        } else {
            root.join("bin").join("codex")
        };
        assert!(
            is_under_agenthub_user_npm_prefix(&hit),
            "expected {} under user prefix",
            hit.display()
        );
    }
    assert!(!is_under_agenthub_user_npm_prefix(std::path::Path::new(
        "/usr/local/bin/codex"
    )));
    if let Ok(home) = crate::utils::paths::home_dir() {
        assert!(!is_under_agenthub_user_npm_prefix(
            &home.join(".local").join("bin").join("codex")
        ));
        assert!(!is_under_agenthub_user_npm_prefix(
            &home.join(".npm-global").join("bin").join("codex")
        ));
        #[cfg(windows)]
        {
            assert!(!is_under_agenthub_user_npm_prefix(
                &home
                    .join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .join("codex.cmd")
            ));
        }
    }
}

#[test]
fn parse_npmrc_global_prefix_last_wins_and_expands_home() {
    let home = PathBuf::from("/Users/demo");
    assert_eq!(
        parse_npmrc_global_prefix("prefix=~/.npm-global\n", &home),
        Some(home.join(".npm-global"))
    );
    assert_eq!(
        parse_npmrc_global_prefix("prefix=\"${HOME}\"\n", &home),
        Some(home.clone())
    );
    assert_eq!(
        parse_npmrc_global_prefix("prefix=$HOME\n", &home),
        Some(home.clone())
    );
    let text = "# ignore\nprefix=/first\nprefix = /second\n; prefix=/commented\n";
    assert_eq!(
        parse_npmrc_global_prefix(text, &home),
        Some(PathBuf::from("/second"))
    );
    assert_eq!(parse_npmrc_global_prefix("; prefix=/no\n", &home), None);
    assert_eq!(parse_npmrc_global_prefix("cache=/tmp\n", &home), None);
}

#[test]
fn npm_prefix_stdout_to_bin_dir_trims_and_maps_platform_bin() {
    assert_eq!(npm_prefix_stdout_to_bin_dir("   \n"), None);
    #[cfg(not(windows))]
    assert_eq!(
        npm_prefix_stdout_to_bin_dir("  /opt/homebrew \n"),
        Some(PathBuf::from("/opt/homebrew/bin"))
    );
    #[cfg(windows)]
    assert_eq!(
        npm_prefix_stdout_to_bin_dir("  C:\\Users\\demo\\AppData\\Roaming\\npm \r\n"),
        Some(PathBuf::from(r"C:\Users\demo\AppData\Roaming\npm"))
    );
}

#[cfg(not(windows))]
#[test]
fn well_known_npm_cli_dirs_include_homebrew_without_path() {
    let home = PathBuf::from("/Users/demo");
    let dirs = well_known_npm_cli_dirs(&home);
    assert!(
        dirs.contains(&PathBuf::from("/opt/homebrew/bin")),
        "macOS GUI PATH often omits Homebrew: {dirs:?}"
    );
    assert!(dirs.contains(&PathBuf::from("/usr/local/bin")), "{dirs:?}");
}

#[cfg(not(windows))]
#[test]
fn well_known_npm_cli_dirs_use_home_nvm_when_nvm_dir_unset() {
    let _guard = DETECT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let bin = home
        .join(".nvm")
        .join("versions")
        .join("node")
        .join("v22.11.0")
        .join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let prev = std::env::var_os("NVM_DIR");
    std::env::remove_var("NVM_DIR");
    let dirs = well_known_npm_cli_dirs(home);
    restore_env("NVM_DIR", prev);
    assert!(
        dirs.contains(&bin),
        "GUI env often omits NVM_DIR; ~/.nvm must still be probed: {dirs:?}"
    );
}

#[test]
fn npm_global_bin_dirs_read_npmrc_without_npm_on_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let prefix = home.join("custom-npm-prefix");
    std::fs::write(
        home.join(".npmrc"),
        format!("prefix={}\n", prefix.display()),
    )
    .unwrap();
    let dirs = npm_global_bin_dirs(home);
    #[cfg(windows)]
    let expected = prefix.clone();
    #[cfg(not(windows))]
    let expected = prefix.join("bin");
    assert!(
        dirs.iter().any(|d| d == &expected),
        "custom ~/.npmrc prefix must be scanned when PATH has no npm: {dirs:?}"
    );
}

#[test]
fn well_known_codex_npm_paths_include_npm_global_bin_dirs() {
    let Ok(home) = crate::utils::paths::home_dir() else {
        return;
    };
    let dirs = npm_global_bin_dirs(&home);
    let paths = well_known_bin_paths(AgentId::Codex);
    for dir in dirs {
        assert!(
            paths
                .iter()
                .any(|(p, ch)| *ch == "npm" && p.starts_with(&dir)),
            "Codex well-known npm scan must include {dir:?}: {paths:?}"
        );
    }
}

#[test]
fn detect_binary_prefers_path_or_well_known_when_agent_installed() {
    // Integration smoke on developer machines: at least one of claude/codex/kimi/grok
    // is commonly present. If none, still validates NotFound note shape.
    let mut any = false;
    for (agent, name) in [
        (AgentId::Claude, "claude"),
        (AgentId::Codex, "codex"),
        (AgentId::Kimi, "kimi"),
        (AgentId::Grok, "grok"),
        (AgentId::Pi, "pi"),
    ] {
        let r = detect_binary(agent, &[name], &["--version"], None, true);
        if r.status == crate::models::DetectStatus::Installed {
            any = true;
            assert!(r.binary_path.is_some());
            assert!(
                r.channel.as_deref() == Some("npm") || r.channel.as_deref() == Some("native"),
                "channel must be concrete npm|native, got {:?}",
                r.channel
            );
            // Version may be empty if probe fails; must never be shell-error garbage.
            if let Some(v) = &r.version {
                assert!(
                    looks_like_version_line(v),
                    "version must look like a version, got {v:?}"
                );
            }
        } else {
            assert_eq!(
                r.notes.first().map(String::as_str),
                Some(NOT_FOUND_FIREFIGHTING_NOTE),
                "not-found notes must start with the firefighting copy, got {:?}",
                r.notes
            );
        }
    }
    let _ = any;
}

#[test]
fn json_writer_replaces_complete_claude_config() {
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("settings.json");
    std::fs::write(&json_path, b"old").unwrap();
    let claude = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "secret"}}),
    };
    write_json_config(&json_path, &claude).unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(stored, claude.raw);
}

#[test]
fn toml_writer_replaces_provider_keys_and_preserves_unmanaged_sections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let live = r#"# user comment
model = "old-model"
base_url = "https://old.example"
api_key = "old-secret"
theme = "dark"

[mcp_servers.demo]
command = "demo"
"#;
    std::fs::write(&path, live).unwrap();
    let desired = r#"model = "new-model"
api_key = "new-secret"
"#;
    let grok = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": desired}),
    };

    write_toml_config(AgentId::Grok, &path, &grok).unwrap();

    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("# user comment"));
    assert!(stored.contains("theme = \"dark\""));
    assert!(stored.contains("[mcp_servers.demo]"));
    assert!(stored.contains("command = \"demo\""));
    assert!(stored.contains("model = \"new-model\""));
    assert!(stored.contains("api_key = \"new-secret\""));
    assert!(!stored.contains("old-model"));
    assert!(!stored.contains("old.example"));
    assert!(!stored.contains("old-secret"));
    assert!(!stored.contains("base_url"));
}

#[test]
fn toml_writer_accepts_ccswitch_config_alias() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let codex = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "format": "toml",
            "config": "model = \"from-config-alias\"\n",
            "auth": { "OPENAI_API_KEY": "sk-x" }
        }),
    };
    write_toml_config(AgentId::Codex, &path, &codex).unwrap();
    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("from-config-alias"));
}

#[test]
fn toml_writer_replaces_codex_provider_table_but_keeps_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let live = r#"model = "old"

[model_providers.old]
base_url = "https://old.example"

# keep this integration
[mcp_servers.keep]
command = "keep"
"#;
    std::fs::write(&path, live).unwrap();
    let desired = r#"model = "new"

[model_providers.new]
base_url = "https://new.example"
"#;
    let codex = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({"format": "toml", "content": desired}),
    };

    write_toml_config(AgentId::Codex, &path, &codex).unwrap();

    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("model = \"new\""));
    assert!(stored.contains("[model_providers.new]"));
    assert!(stored.contains("https://new.example"));
    assert!(!stored.contains("[model_providers.old]"));
    assert!(!stored.contains("https://old.example"));
    assert!(stored.contains("# keep this integration"));
    assert!(stored.contains("[mcp_servers.keep]"));
}

#[test]
fn toml_writer_rejects_invalid_documents_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let valid_live = "model = \"old\"\n";
    std::fs::write(&path, valid_live).unwrap();

    let invalid_target = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": "model = ["}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &invalid_target)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), valid_live);

    let invalid_live = "not valid toml";
    std::fs::write(&path, invalid_live).unwrap();
    let valid_target = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": "model = \"new\"\n"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &valid_target)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid_live);
}

#[test]
fn config_writers_reject_agent_and_shape_mismatches_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, b"keep").unwrap();

    let wrong_agent = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"format": "toml", "content": "replace"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &wrong_agent)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );

    let wrong_shape = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "preview": "truncated"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &wrong_shape)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
}

#[test]
fn matrix_covers_all_agents_and_capabilities() {
    let reg = register_all();
    let matrix = reg.matrix();
    assert_eq!(matrix.len(), AgentId::ALL.len());
    for agent in AgentId::ALL {
        let row = matrix.get(&agent).expect("agent row");
        assert_eq!(row.len(), Capability::ALL.len());
    }
}

#[test]
fn non_full_capabilities_must_explain_themselves() {
    for adapter in register_all().all() {
        for cap in Capability::ALL {
            let state = adapter.capability(cap);
            if state.level != CapabilityLevel::Full {
                assert!(
                    state.reason.is_some(),
                    "{}/{:?} 缺少 reason",
                    adapter.id().as_str(),
                    cap
                );
            }
        }
    }
}

#[test]
fn declared_capabilities_match_actual_behavior() {
    for adapter in register_all().all() {
        let id = adapter.id();

        if adapter.capability(Capability::Skills).is_blocked() {
            assert!(
                adapter.skills_dir().is_none(),
                "{} 声明无 skills 却给了目录",
                id.as_str()
            );
        }
        if adapter.capability(Capability::ConfigWrite).is_blocked() {
            let probe = AgentConfig {
                agent: id,
                raw: json!({}),
            };
            assert!(
                matches!(adapter.write_config(&probe), Err(AppError::Unsupported(_))),
                "{} 声明无 ConfigWrite 但 write_config 未 fail-closed",
                id.as_str()
            );
        }
        if adapter.capability(Capability::LiveBackup).is_blocked() {
            assert!(
                adapter.live_backup_paths().is_empty(),
                "{} 声明无 LiveBackup 却返回路径",
                id.as_str()
            );
        }
        if adapter
            .capability(Capability::StructuredStream)
            .is_blocked()
        {
            assert!(
                !supports_structured_stream(id),
                "{} 声明无 StructuredStream",
                id.as_str()
            );
        }
        if adapter.capability(Capability::ProviderPresets).is_blocked() {
            assert!(
                crate::presets::list_for(id).is_empty(),
                "{} 声明无 ProviderPresets 却有预设",
                id.as_str()
            );
        }
    }
}

#[test]
fn require_blocks_unsupported_and_allows_full() {
    let reg = register_all();
    assert!(reg.require(AgentId::Claude, Capability::Skills).is_ok());
    let err = match reg.require(AgentId::Kimi, Capability::Skills) {
        Ok(_) => panic!("kimi skills should be blocked"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("技能"));
    assert!(err.to_string().contains("不支持"));
}

#[test]
fn require_planned_uses_distinct_copy_from_unsupported() {
    let reg = register_all();
    // Claude Usage is Full; SessionResume is Partial (print+resume). Grok SessionResume stays Planned.
    assert!(reg.require(AgentId::Claude, Capability::Usage).is_ok());
    assert!(reg
        .require(AgentId::Claude, Capability::SessionResume)
        .is_ok());
    let planned = match reg.require(AgentId::Grok, Capability::SessionResume) {
        Ok(_) => panic!("grok session resume should be planned/blocked"),
        Err(e) => e,
    };
    let unsupported = match reg.require(AgentId::Kimi, Capability::Skills) {
        Ok(_) => panic!("kimi skills should be unsupported"),
        Err(e) => e,
    };
    assert_eq!(planned.code(), "unsupported");
    assert_eq!(unsupported.code(), "unsupported");
    assert!(
        planned.to_string().contains("尚未接入"),
        "planned copy: {}",
        planned
    );
    assert!(
        unsupported.to_string().contains("不支持"),
        "unsupported copy: {}",
        unsupported
    );
    assert!(!planned.to_string().contains("不支持"));
}

#[test]
fn require_allows_partial_dangerous_mode_for_kimi() {
    let reg = register_all();
    let adapter = reg
        .require(AgentId::Kimi, Capability::DangerousMode)
        .expect("partial should pass");
    assert_eq!(adapter.id(), AgentId::Kimi);
    let state = adapter.capability(Capability::DangerousMode);
    assert_eq!(state.level, CapabilityLevel::Partial);
    assert!(state.reason.is_some());
}

#[test]
fn matrix_matches_documented_boundary_cells() {
    let reg = register_all();
    let matrix = reg.matrix();
    assert_eq!(
        matrix[&AgentId::Kimi][&Capability::Skills].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Cursor][&Capability::AccountSwitch].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Codex][&Capability::ApiKeyAccount].level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        matrix[&AgentId::Cursor][&Capability::Usage].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Claude][&Capability::Usage].level,
        CapabilityLevel::Full
    );
    assert_eq!(
        matrix[&AgentId::Zcode][&Capability::Usage].level,
        CapabilityLevel::Full
    );
    assert_eq!(
        matrix[&AgentId::Zcode][&Capability::ProjectHistory].level,
        CapabilityLevel::Partial
    );
    assert!(supports_structured_stream(AgentId::Claude));
    assert!(!supports_structured_stream(AgentId::WorkBuddy));
    assert!(!supports_structured_stream(AgentId::Cursor));
}

#[test]
fn write_verified_json_object_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("creds.json");
    let body = serde_json::json!({"format": "api_key", "api_key": "sk-test"});
    write_verified_json_object(&path, &body).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed, body);
}

#[test]
fn write_verified_json_object_rejects_non_object() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    let err = write_verified_json_object(&path, &serde_json::json!(["x"])).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn require_api_key_and_live_account_helpers() {
    assert_eq!(require_api_key("  sk-abc  ").unwrap(), "sk-abc");
    assert_eq!(require_api_key("").unwrap_err().code(), "invalid_arg");
    assert_eq!(require_api_key("***").unwrap_err().code(), "invalid_arg");
    assert_eq!(
        require_api_key("$AGENTHUB_CONNECTION_SECRET$")
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    let live = api_key_live_account(
        AgentId::Claude,
        "sk-abcdefghijklmnop",
        serde_json::json!({"format": "api_key", "api_key": "sk-abcdefghijklmnop"}),
        "API Key",
        serde_json::json!({"source": "manual"}),
    );
    assert_eq!(live.agent, AgentId::Claude);
    assert_eq!(live.kind, AccountKind::ApiKey);
    assert!(live.label_hint.as_ref().unwrap().contains("API Key"));
    assert!(!live
        .label_hint
        .as_ref()
        .unwrap()
        .contains("sk-abcdefghijklmnop"));
}

#[test]
fn supports_structured_stream_uses_shared_registry() {
    // Twice: must not rebuild via register_all each call (OnceLock).
    assert_eq!(
        supports_structured_stream(AgentId::Claude),
        supports_structured_stream(AgentId::Claude)
    );
    assert!(supports_structured_stream(AgentId::Grok));
    assert!(!supports_structured_stream(AgentId::Cursor));
}

#[test]
fn default_authorization_key_api_key_stable_and_distinct() {
    let a = json!({"format": "api_key", "api_key": "sk-same"});
    let b = json!({"format": "api_key", "api_key": "sk-same"});
    let c = json!({"format": "api_key", "api_key": "sk-other"});
    let ka = default_authorization_key(AccountKind::ApiKey, &a).unwrap();
    let kb = default_authorization_key(AccountKind::ApiKey, &b).unwrap();
    let kc = default_authorization_key(AccountKind::ApiKey, &c).unwrap();
    assert_eq!(ka, kb);
    assert_ne!(ka, kc);
    assert!(ka.starts_with("apikey:sha256:"));
    // Must not embed raw key
    assert!(!ka.contains("sk-same"));
}

#[test]
fn default_authorization_key_oauth_prefers_refresh_then_access() {
    let with_refresh = json!({
        "format": "credentials_json",
        "body": {
            "refresh_token": "refresh-aaa",
            "access_token": "access-should-be-ignored"
        }
    });
    let with_access = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "u@x.com", "key": "access-bbb" } }
    });
    let kr = default_authorization_key(AccountKind::Oauth, &with_refresh).unwrap();
    let ka = default_authorization_key(AccountKind::Oauth, &with_access).unwrap();
    assert!(kr.starts_with("oauth:refresh_sha:"));
    assert!(ka.starts_with("oauth:access_sha:"));
    assert_ne!(kr, ka);

    // Same email, different access keys → different authorizations
    let other = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "u@x.com", "key": "access-ccc" } }
    });
    assert_ne!(
        default_authorization_key(AccountKind::Oauth, &with_access).unwrap(),
        default_authorization_key(AccountKind::Oauth, &other).unwrap()
    );
}

#[test]
fn default_identity_label_uses_email_not_for_auth_key() {
    let creds = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "person@example.com", "key": "tok" } }
    });
    let label = default_identity_label(AccountKind::Oauth, &creds, Some("hint")).unwrap();
    assert_eq!(label, "person@example.com");
    let key = default_authorization_key(AccountKind::Oauth, &creds).unwrap();
    assert!(!key.contains("person@example.com"));
    assert!(key.starts_with("oauth:access_sha:"));
}

#[test]
fn auth_metadata_recognizes_nested_refresh_and_expiry_without_values() {
    let value = json!({
        "account": {
            "tokens": {
                "accessToken": "fake-access-token",
                "refreshToken": "fake-refresh-token",
                "expiresAt": 1
            }
        }
    });
    let metadata = inspect_auth_credentials(&value);
    assert!(metadata.has_access_token);
    assert!(metadata.has_refresh_token);
    assert_eq!(metadata.access_expired, Some(true));
    let debug = format!("{metadata:?}");
    assert!(!debug.contains("fake-access-token"));
    assert!(!debug.contains("fake-refresh-token"));
}

#[test]
fn auth_metadata_parses_absolute_expiry_but_ignores_relative_expires_in() {
    let seconds = inspect_auth_credentials(&json!({ "access_token": "access", "expires_at": 1 }));
    assert_eq!(seconds.access_expired, Some(true));

    let millis =
        inspect_auth_credentials(&json!({ "access_token": "access", "expires_at": 1_000 }));
    assert_eq!(millis.access_expired, Some(true));

    let rfc3339 = inspect_auth_credentials(&json!({
        "access_token": "access",
        "expires_at": "1970-01-01T00:00:01Z"
    }));
    assert_eq!(rfc3339.access_expired, Some(true));

    let relative = inspect_auth_credentials(&json!({
        "access_token": "access",
        "expires_in": 1,
        "expiresin": "60"
    }));
    assert_eq!(relative.access_expired, None);
}

#[test]
fn oauth_health_requires_both_expired_tokens_before_needs_login() {
    let expired_access_and_refresh = AuthCredentialMetadata {
        has_access_token: true,
        has_refresh_token: true,
        access_expired: Some(true),
        refresh_expired: Some(true),
        ..Default::default()
    };
    assert_eq!(
        oauth_auth_health(expired_access_and_refresh),
        crate::models::AuthHealth::NeedsLogin
    );

    let renewable = AuthCredentialMetadata {
        has_access_token: true,
        has_refresh_token: true,
        access_expired: Some(true),
        refresh_expired: None,
        ..Default::default()
    };
    assert_eq!(
        oauth_auth_health(renewable),
        crate::models::AuthHealth::Renewable
    );

    for access_expired in [Some(false), None] {
        let stale_refresh = AuthCredentialMetadata {
            has_access_token: true,
            has_refresh_token: true,
            access_expired,
            refresh_expired: Some(true),
            ..Default::default()
        };
        assert_eq!(
            oauth_auth_health(stale_refresh),
            crate::models::AuthHealth::Configured
        );
    }

    let missing_access_with_expired_refresh = AuthCredentialMetadata {
        has_refresh_token: true,
        refresh_expired: Some(true),
        ..Default::default()
    };
    assert_eq!(
        oauth_auth_health(missing_access_with_expired_refresh),
        crate::models::AuthHealth::NeedsLogin
    );

    for refresh_expired in [Some(false), None] {
        let missing_access_with_renewable_refresh = AuthCredentialMetadata {
            has_refresh_token: true,
            refresh_expired,
            ..Default::default()
        };
        assert_eq!(
            oauth_auth_health(missing_access_with_renewable_refresh),
            crate::models::AuthHealth::Renewable
        );
    }
}

#[test]
fn kimi_malformed_auth_inputs_are_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let cred = dir.path().join("credentials").join("kimi-code.json");
    std::fs::write(&config, "providers = [").unwrap();

    let malformed_config = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(malformed_config.health, crate::models::AuthHealth::Unknown);
    assert_eq!(malformed_config.source.as_deref(), Some("kimi:config.toml"));

    std::fs::write(&config, "").unwrap();
    std::fs::create_dir_all(cred.parent().unwrap()).unwrap();
    std::fs::write(&cred, "{").unwrap();

    let malformed_credentials = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(
        malformed_credentials.health,
        crate::models::AuthHealth::Unknown
    );
    assert_eq!(
        malformed_credentials.source.as_deref(),
        Some("kimi:credentials/kimi-code.json")
    );
}

#[test]
fn cursor_status_parser_handles_negative_authenticated_phrase() {
    assert_eq!(
        cursor::cursor_status_health("Status: not authenticated"),
        crate::models::AuthHealth::NeedsLogin
    );
    assert_eq!(
        cursor::cursor_status_health("Authenticated: true"),
        crate::models::AuthHealth::Verified
    );
    for false_status in [
        "Authenticated: false",
        "logged in: false",
        "is authenticated: false",
        r#"{"authenticated": false}"#,
    ] {
        assert_eq!(
            cursor::cursor_status_health(false_status),
            crate::models::AuthHealth::NeedsLogin,
            "{false_status}"
        );
    }
    assert_eq!(
        cursor::cursor_status_health("status unavailable"),
        crate::models::AuthHealth::Unknown
    );
}

#[test]
fn auth_file_revision_is_opaque_and_contains_no_secret_or_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    std::fs::write(&path, br#"{"access_token":"fake-secret-token"}"#).unwrap();
    let revision = auth_file_revision(&path).unwrap();
    let canonical = std::fs::canonicalize(&path).unwrap();
    let path_text = path.to_string_lossy();
    let canonical_text = canonical.to_string_lossy();
    assert!(revision.starts_with("file:sha256:"));
    assert!(!revision.contains("fake-secret-token"));
    assert!(!revision.contains(path_text.as_ref()));
    assert!(!revision.contains(canonical_text.as_ref()));
}

#[test]
fn auth_file_revision_changes_after_same_length_atomic_replacement_with_forced_mtime() {
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, UNIX_EPOCH};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    std::fs::write(&path, b"credential-one").unwrap();
    let forced_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(forced_time))
        .unwrap();
    let before = auth_file_revision(&path).unwrap();

    // The helper uses the production atomic-replace path.  Reset the mtime
    // after the replacement to prove detection does not rely on coarse mtime
    // or length alone.
    atomic_write(&path, b"credential-two").unwrap();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(forced_time))
        .unwrap();
    let after = auth_file_revision(&path).unwrap();

    assert_eq!(b"credential-one".len(), b"credential-two".len());
    assert_ne!(before, after);
}

#[test]
fn pi_auth_health_is_provider_scoped_and_order_independent() {
    use crate::models::AuthHealth;

    let expired_oauth = json!({
        "type": "oauth",
        "access": "expired-access",
        "refresh": "expired-refresh",
        "expires": 1,
        "refresh_expires": 1,
    });
    let api_key = json!({ "type": "api_key", "key": "configured-key" });
    let expired = pi::pi_provider_auth_health(&expired_oauth);
    let configured = pi::pi_provider_auth_health(&api_key);
    assert_eq!(expired, AuthHealth::NeedsLogin);
    assert_eq!(configured, AuthHealth::Configured);
    assert_eq!(
        pi::aggregate_pi_provider_auth_health([expired, configured]),
        AuthHealth::Configured
    );
    assert_eq!(
        pi::aggregate_pi_provider_auth_health([configured, expired]),
        AuthHealth::Configured
    );
    assert_eq!(
        pi::aggregate_pi_provider_auth_health([expired, expired]),
        AuthHealth::NeedsLogin
    );
}

#[test]
fn claude_keychain_oauth_apply_is_explicitly_unsupported() {
    let error =
        claude::ensure_claude_oauth_file_apply_source(claude::ClaudeOauthSource::MacosKeychain)
            .unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(error.to_string().contains("Keychain"));
    assert!(claude::ensure_claude_oauth_file_apply_source(
        claude::ClaudeOauthSource::CredentialsFile
    )
    .is_ok());
}

fn assert_auth_state_hides_secrets(state: &crate::models::AuthState, secrets: &[&str]) {
    let dumped = serde_json::to_string(state).expect("serialize auth state");
    for secret in secrets {
        assert!(
            !dumped.contains(secret),
            "live AuthState must not embed credential material"
        );
    }
}

fn assert_also_present_empty(state: &crate::models::AuthState) {
    assert!(state.also_present.is_empty());
    let value = serde_json::to_value(state).unwrap();
    assert!(
        value.get("alsoPresent").is_none(),
        "empty also_present must be omitted from JSON"
    );
}

#[test]
fn claude_settings_token_and_oauth_sets_also_present() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{
  "env": { "ANTHROPIC_AUTH_TOKEN": "sk-settings-fixture" }
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"tok-oauth-fixture","expiresAt":9999999999}}"#,
    )
    .unwrap();
    let state = claude::claude_auth_state(dir.path()).unwrap();

    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert!(state.also_present.iter().any(|kind| kind == "oauth"));
    assert_auth_state_hides_secrets(&state, &["sk-settings-fixture", "tok-oauth-fixture"]);
}

#[test]
fn claude_oauth_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"tok-oauth-only-fixture","expiresAt":9999999999}}"#,
    )
    .unwrap();
    let state = claude::claude_auth_state(dir.path()).unwrap();

    assert_eq!(state.kind.as_deref(), Some("oauth"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(&state, &["tok-oauth-only-fixture"]);
}

#[test]
fn claude_api_key_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{
  "env": { "ANTHROPIC_AUTH_TOKEN": "sk-settings-only-fixture" }
}
"#,
    )
    .unwrap();
    let state = claude::claude_auth_state(dir.path()).unwrap();

    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(&state, &["sk-settings-only-fixture"]);
}

#[test]
fn claude_api_key_and_unclassifiable_credentials_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{
  "env": { "ANTHROPIC_AUTH_TOKEN": "sk-settings-dirty-fixture" }
}
"#,
    )
    .unwrap();
    for dirty in [r#"{}"#, r#"{"mcpOAuth":{}}"#] {
        std::fs::write(dir.path().join(".credentials.json"), dirty).unwrap();
        let state = claude::claude_auth_state(dir.path()).unwrap();
        assert_eq!(state.kind.as_deref(), Some("api_key"), "{dirty}");
        assert_also_present_empty(&state);
        assert_auth_state_hides_secrets(&state, &["sk-settings-dirty-fixture"]);
    }
}

#[test]
fn kimi_api_key_and_oauth_sets_also_present() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let cred = dir.path().join("credentials").join("kimi-code.json");
    std::fs::write(
        &config,
        r#"default_provider = "moonshot"

[providers.moonshot]
api_key = "kimi-key-fixture"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(cred.parent().unwrap()).unwrap();
    std::fs::write(
        &cred,
        r#"{"access_token":"kimi-access-fixture","refresh_token":"kimi-refresh-fixture"}"#,
    )
    .unwrap();

    let state = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert!(state.also_present.iter().any(|kind| kind == "oauth"));
    assert_auth_state_hides_secrets(&state, &["kimi-key-fixture", "kimi-access-fixture"]);
}

#[test]
fn kimi_api_key_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let cred = dir.path().join("credentials").join("kimi-code.json");
    std::fs::write(
        &config,
        r#"default_provider = "moonshot"

[providers.moonshot]
api_key = "kimi-key-only-fixture"
"#,
    )
    .unwrap();

    let state = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(&state, &["kimi-key-only-fixture"]);
}

#[test]
fn kimi_oauth_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let cred = dir.path().join("credentials").join("kimi-code.json");
    std::fs::write(&config, "default_provider = \"moonshot\"\n").unwrap();
    std::fs::create_dir_all(cred.parent().unwrap()).unwrap();
    std::fs::write(
        &cred,
        r#"{"access_token":"kimi-oauth-only-fixture","refresh_token":"kimi-refresh-only-fixture"}"#,
    )
    .unwrap();

    let state = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(state.kind.as_deref(), Some("oauth"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(
        &state,
        &["kimi-oauth-only-fixture", "kimi-refresh-only-fixture"],
    );
}

#[test]
fn kimi_api_key_and_garbage_credentials_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let cred = dir.path().join("credentials").join("kimi-code.json");
    std::fs::write(
        &config,
        r#"default_provider = "moonshot"

[providers.moonshot]
api_key = "kimi-key-garbage-fixture"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(cred.parent().unwrap()).unwrap();
    std::fs::write(&cred, "this is not json {").unwrap();

    let state = kimi::kimi_auth_state(&config, &cred);
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(&state, &["kimi-key-garbage-fixture"]);
}

#[test]
fn codex_api_key_and_oauth_sets_also_present() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    std::fs::write(
        &auth,
        r#"{
  "OPENAI_API_KEY": "sk-codex-fixture",
  "tokens": {
    "access_token": "codex-access-fixture",
    "refresh_token": "codex-refresh-fixture"
  }
}
"#,
    )
    .unwrap();

    let state = codex::codex_auth_state(&auth);
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert!(state.also_present.iter().any(|kind| kind == "oauth"));
    assert_auth_state_hides_secrets(&state, &["sk-codex-fixture", "codex-access-fixture"]);
}

#[test]
fn codex_api_key_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    std::fs::write(
        &auth,
        r#"{
  "OPENAI_API_KEY": "sk-codex-only-fixture"
}
"#,
    )
    .unwrap();

    let state = codex::codex_auth_state(&auth);
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(&state, &["sk-codex-only-fixture"]);
    let hash = state.secret_hash.expect("live api key hash");
    assert_eq!(hash.len(), 64);
    assert_eq!(
        hash,
        crate::utils::redact::secret_sha256_hex("sk-codex-only-fixture")
    );
}

#[test]
fn codex_oauth_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    std::fs::write(
        &auth,
        r#"{
  "tokens": {
    "access_token": "codex-oauth-only-fixture",
    "refresh_token": "codex-refresh-only-fixture"
  }
}
"#,
    )
    .unwrap();

    let state = codex::codex_auth_state(&auth);
    assert_eq!(state.kind.as_deref(), Some("oauth"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(
        &state,
        &["codex-oauth-only-fixture", "codex-refresh-only-fixture"],
    );
}

#[test]
fn pi_mixed_auth_sets_also_present() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    std::fs::write(
        &auth,
        serde_json::to_vec(&json!({
            "anthropic": {
                "type": "oauth",
                "access": "pi-access-fixture",
                "refresh": "pi-refresh-fixture"
            },
            "openai": { "type": "api_key", "key": "pi-key-fixture" }
        }))
        .unwrap(),
    )
    .unwrap();
    let state = pi::pi_auth_state(&auth);

    assert_eq!(state.kind.as_deref(), Some("mixed"));
    assert!(state.also_present.iter().any(|kind| kind == "oauth"));
    assert!(state.also_present.iter().any(|kind| kind == "api_key"));
    assert_auth_state_hides_secrets(&state, &["pi-access-fixture", "pi-key-fixture"]);
}

#[test]
fn pi_oauth_only_leaves_also_present_empty() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    std::fs::write(
        &auth,
        serde_json::to_vec(&json!({
            "anthropic": {
                "type": "oauth",
                "access": "pi-oauth-only-fixture",
                "refresh": "pi-refresh-only-fixture"
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let state = pi::pi_auth_state(&auth);

    assert_eq!(state.kind.as_deref(), Some("oauth"));
    assert_also_present_empty(&state);
    assert_auth_state_hides_secrets(
        &state,
        &["pi-oauth-only-fixture", "pi-refresh-only-fixture"],
    );
}

#[test]
fn pi_auth_json_slots_match_official_api_key_keys() {
    use super::pi_auth::{is_pi_auth_json_slot, pi_api_key_auth_entry, PI_AUTH_JSON_SLOTS};

    assert_eq!(
        PI_AUTH_JSON_SLOTS,
        &[
            "anthropic",
            "ant-ling",
            "azure-openai-responses",
            "openai",
            "deepseek",
            "nvidia",
            "google",
            "amazon-bedrock",
        ]
    );
    for id in PI_AUTH_JSON_SLOTS {
        assert!(is_pi_auth_json_slot(id), "{id} must be an official slot");
    }
    assert!(!is_pi_auth_json_slot("custom"));
    assert!(!is_pi_auth_json_slot("openai-codex"));
    assert_eq!(
        pi_api_key_auth_entry("sk-1"),
        json!({ "type": "api_key", "key": "sk-1" })
    );
}

#[test]
fn apply_pi_api_key_to_dir_writes_openai_and_preserves_anthropic_oauth() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        serde_json::to_vec_pretty(&json!({
            "anthropic": {
                "type": "oauth",
                "access": "keep-access",
                "refresh": "keep-refresh"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    super::pi_auth::apply_pi_api_key_to_dir(dir.path(), "openai", "sk-test-openai").unwrap();

    let body: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(body["openai"]["type"], "api_key");
    assert_eq!(body["openai"]["key"], "sk-test-openai");
    assert_eq!(body["anthropic"]["type"], "oauth");
    assert_eq!(body["anthropic"]["access"], "keep-access");
    assert_eq!(body["anthropic"]["refresh"], "keep-refresh");
}

#[test]
fn apply_pi_api_key_to_dir_rejects_unknown_slot() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        serde_json::to_vec_pretty(&json!({
            "anthropic": { "type": "oauth", "access": "keep-access" }
        }))
        .unwrap(),
    )
    .unwrap();

    let err = super::pi_auth::apply_pi_api_key_to_dir(dir.path(), "custom", "sk-x").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let message = err.to_string();
    assert!(message.contains("custom"), "{message}");
    assert!(message.contains("models.json"), "{message}");

    let body: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(
        body,
        json!({ "anthropic": { "type": "oauth", "access": "keep-access" } })
    );
}

#[test]
fn apply_pi_api_key_to_dir_rejects_empty_provider() {
    let dir = tempfile::tempdir().unwrap();
    for provider in ["", "   "] {
        let err =
            super::pi_auth::apply_pi_api_key_to_dir(dir.path(), provider, "sk-x").unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        let message = err.to_string();
        assert!(
            message.contains("provider slot") || message.contains("anthropic/openai"),
            "{message}"
        );
    }
    assert!(!dir.path().join("auth.json").exists());
}

#[test]
fn print_resume_flags_match_official_cli() {
    let mut opts = crate::models::RunOptions {
        process_mode: crate::models::ProcessMode::Auto,
        native_session_id: Some("sess-1".into()),
        ..crate::models::RunOptions::default()
    };
    let claude = register_all()
        .get(AgentId::Claude)
        .unwrap()
        .build_run_spec(std::path::Path::new("claude"), "hi", &opts)
        .unwrap();
    assert_eq!(claude.args[0], "-p");
    assert_eq!(claude.args[1], "--resume");
    assert_eq!(claude.args[2], "sess-1");
    assert_eq!(claude.args[3], "hi");

    opts.allow_dangerous = true;
    let codex = register_all()
        .get(AgentId::Codex)
        .unwrap()
        .build_run_spec(std::path::Path::new("codex"), "hi", &opts)
        .unwrap();
    assert_eq!(codex.args[0], "exec");
    assert_eq!(codex.args[1], "--skip-git-repo-check");
    assert_eq!(codex.args[2], "resume");
    assert_eq!(codex.args[3], "sess-1");
    assert!(codex.args.contains(&"--json".into()));
    assert!(codex
        .args
        .contains(&"--dangerously-bypass-approvals-and-sandbox".into()));
    assert_eq!(codex.args.last().map(String::as_str), Some("hi"));
}

#[test]
fn codex_chat_run_spec_skips_git_repo_trust_check() {
    let spec = register_all()
        .get(AgentId::Codex)
        .unwrap()
        .build_run_spec(
            std::path::Path::new("codex"),
            "ping",
            &crate::models::RunOptions::default(),
        )
        .unwrap();
    assert_eq!(spec.args[0], "exec");
    assert!(
        spec.args.contains(&"--skip-git-repo-check".into()),
        "AgentHub Chat workdirs are often not trusted git repos: {spec:?}"
    );
    assert_eq!(spec.args.last().map(String::as_str), Some("ping"));
}

#[test]
fn write_kimi_api_key_ensures_models_table_for_default_model() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"default_model = "kimi-k2"
[providers.moonshot]
base_url = "https://mytokens.cc/v1"
api_key = "old"
"#,
    )
    .unwrap();
    kimi::write_kimi_api_key(&path, "sk-new-key").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("sk-new-key"), "{text}");
    assert!(
        text.contains("[models.\"kimi-k2\"]") || text.contains("[models.kimi-k2]"),
        "{text}"
    );
    assert!(text.contains("provider = \"moonshot\""), "{text}");
    assert!(text.contains("model = \"kimi-k2\""), "{text}");
    assert!(text.contains("max_context_size = 131072"), "{text}");
    assert!(text.contains("type = \"openai\""), "{text}");
    assert!(text.contains("default_provider = \"moonshot\""), "{text}");
}

#[test]
fn kimi_switch_write_keeps_account_model_instead_of_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_toml_config(
        AgentId::Kimi,
        &path,
        &AgentConfig {
            agent: AgentId::Kimi,
            raw: json!({
                "format": "toml",
                "content": "default_model = \"grok-4.5\"\n\n[providers.moonshot]\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"sk-pool\"\n",
            }),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("default_model = \"grok-4.5\""), "{text}");
    assert!(!text.contains("kimi-k2"), "{text}");
    assert!(
        text.contains("[models.\"grok-4.5\"]") || text.contains("[models.grok-4.5]"),
        "{text}"
    );
}

#[test]
fn kimi_write_config_points_base_url_at_loopback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "default_provider = \"moonshot\"\n\n[providers.moonshot]\nbase_url = \"https://api.moonshot.cn/v1\"\napi_key = \"old\"\n",
    )
    .unwrap();
    write_toml_config(
        AgentId::Kimi,
        &path,
        &AgentConfig {
            agent: AgentId::Kimi,
            raw: json!({
                "format": "toml",
                "content": "default_provider = \"agenthub_codex_bridge\"\n\n[providers.agenthub_codex_bridge]\nname = \"AgentHub Codex Route\"\nbase_url = \"http://127.0.0.1:32123/v1\"\napi_key = \"ahb_local\"\n",
            }),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("http://127.0.0.1:32123/v1"));
    assert!(text.contains("agenthub_codex_bridge"));
    assert!(!text.contains("gpt-"));
}

#[test]
fn kimi_switch_write_backfills_models_table_from_incomplete_pool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_toml_config(
        AgentId::Kimi,
        &path,
        &AgentConfig {
            agent: AgentId::Kimi,
            raw: json!({
                "format": "toml",
                "content": "default_model = \"kimi-k2\"\n\n[providers.moonshot]\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"sk-pool\"\n",
            }),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("sk-pool"), "{text}");
    assert!(
        text.contains("[models.\"kimi-k2\"]") || text.contains("[models.kimi-k2]"),
        "{text}"
    );
    assert!(text.contains("provider = \"moonshot\""), "{text}");
    assert!(text.contains("max_context_size = 131072"), "{text}");
    assert!(text.contains("type = \"openai\""), "{text}");
}

#[test]
fn ide_codex_bins_under_finds_openai_chatgpt_extension() {
    let dir = tempfile::tempdir().unwrap();
    let ext = dir
        .path()
        .join("openai.chatgpt-26.818.61809-win32-x64")
        .join("bin");
    #[cfg(windows)]
    let bin = ext.join("windows-x86_64").join("codex.exe");
    #[cfg(target_os = "macos")]
    let bin = ext.join("darwin-arm64").join("codex");
    #[cfg(all(unix, not(target_os = "macos")))]
    let bin = ext.join("linux-x86_64").join("codex");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, b"fake").unwrap();
    let found = ide_codex_bins_under(dir.path());
    assert!(
        found.iter().any(|p| p == &bin),
        "expected {bin:?} in {found:?}"
    );
}

#[cfg(windows)]
#[test]
fn well_known_codex_paths_include_official_native_home() {
    let paths = well_known_bin_paths(AgentId::Codex);
    assert!(
        paths.iter().any(|(p, ch)| {
            *ch == "native"
                && p.to_string_lossy()
                    .replace('/', "\\")
                    .to_ascii_lowercase()
                    .contains(r"programs\openai\codex\bin")
        }),
        "official Windows native dest missing: {paths:?}"
    );
}
