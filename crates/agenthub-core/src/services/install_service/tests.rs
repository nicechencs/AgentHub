use super::*;
use crate::adapters::{register_all, AgentAdapter};
use crate::catalog::install::{
    native_ps1_url, native_setup_url, native_sh_url, npm_install_extra_flags, npm_package,
};
use crate::error::AppError;
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, InstallChannel, RunOptions,
    RunSpec,
};
use crate::platform::install::{builtin_install_registry, InstallContribution};
use crate::platform::AgentKey;
use crate::services::ProviderService;
use crate::utils::command_exec::ExecRequest;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct MockExecutor {
    calls: Arc<Mutex<Vec<String>>>,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

impl CommandExecutor for MockExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        let cmd = format!("{} {}", req.program, req.args.join(" "));
        self.calls.lock().unwrap().push(cmd.clone());
        ExecResult {
            command: cmd,
            exit_code: Some(self.exit_code),
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            timed_out: false,
            spawn_error: None,
        }
    }
}

#[test]
fn install_runtime_powershell_refuses() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::PowerShell, "winget", &ex).unwrap();
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("unsupported"));
    assert!(out.message.contains("不支持") || out.message.contains("PowerShell"));
    assert!(ex.calls.lock().unwrap().is_empty());
}

#[test]
fn runtime_default_channel_matches_platform() {
    if cfg!(target_os = "macos") {
        assert_eq!(default_runtime_channel(), "brew");
    } else if cfg!(windows) {
        assert_eq!(default_runtime_channel(), "winget");
    } else {
        assert_eq!(default_runtime_channel(), "manual");
    }
}

#[cfg(not(windows))]
#[test]
fn native_sh_install_does_not_probe_powershell() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };

    // Grok has an allowlisted install.sh URL.  The mock executor prevents any
    // network/process side effects; this only exercises dependency selection
    // and command construction.
    let _ = install_agent(&registry, AgentId::Grok, "native", false, &ex).unwrap();
    let commands = calls.lock().unwrap();
    assert!(
        commands.iter().any(|command| command.contains("bash -lc")),
        "expected POSIX shell command, got {commands:?}"
    );
    assert!(commands
        .iter()
        .all(|command| !command.to_ascii_lowercase().contains("powershell")));
}

#[cfg(not(windows))]
#[test]
fn native_shell_selection_keeps_sh_compatible_scripts_on_resolved_sh() {
    let sh = std::path::Path::new("/usr/bin/sh");
    let selected = select_native_shell(NativeShellRequirement::Posix, None, Some(sh)).unwrap();
    assert_eq!(selected, sh);

    let (args, program) = native_shell_invocation(&selected, "https://example.test/install.sh");
    assert_eq!(program, "/usr/bin/sh");
    assert_eq!(args[0], "-c");
    assert!(args[1].ends_with("| '/usr/bin/sh'"));
    assert!(!args[1].ends_with("| bash"));
}

#[cfg(not(windows))]
#[test]
fn native_shell_selection_requires_bash_when_documented_script_needs_it() {
    let error = select_native_shell(
        NativeShellRequirement::Bash,
        None,
        Some(std::path::Path::new("/usr/bin/sh")),
    )
    .unwrap_err();

    assert_eq!(error.code(), "not_found");
    assert!(error.to_string().contains("requires bash"));
}

#[test]
fn install_agent_env_not_ready_without_deps() {
    // Use a mock that should never be called if env missing and no install_deps.
    // On machines without node, codex npm channel fails env ensure.
    let registry = register_all();
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    // Force path: if env is ready on this machine, the test still asserts shape.
    let out = install_agent(&registry, AgentId::Codex, "npm", false, &ex).unwrap();
    if runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        // Env ready: install may run and redetect — ok either way, but should have logs.
        assert!(!out.logs.is_empty());
    } else {
        assert!(!out.ok);
        assert!(out.message.contains("环境") || out.message.contains("未就绪"));
        assert!(ex.calls.lock().unwrap().is_empty());
    }
}

#[test]
fn install_from_contribution_uses_npm_allowlist_without_agent_id() {
    use crate::platform::install::InstallContribution;
    use crate::platform::AgentKey;

    struct FakeContrib;
    impl InstallContribution for FakeContrib {
        fn agent_key(&self) -> AgentKey {
            AgentKey::parse("p1-2-fake-npm").unwrap()
        }
        fn npm_package(&self) -> Option<&'static str> {
            Some("@agenthub/p1-2-fake-npm")
        }
        fn npm_install_extra_flags(&self) -> &'static [&'static str] {
            &["--ignore-scripts"]
        }
    }

    let key = AgentKey::parse("p1-2-fake-npm").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };

    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        let out = install_from_contribution(&key, &FakeContrib, "npm", false, &ex).unwrap();
        assert!(!out.ok);
        assert_eq!(out.code.as_deref(), Some("env.not_ready"));
        assert!(calls.lock().unwrap().is_empty());
        return;
    }

    let out = install_from_contribution(&key, &FakeContrib, "npm", false, &ex).unwrap();
    assert!(out.ok, "msg={}", out.message);
    let commands = calls.lock().unwrap();
    assert!(
        commands.iter().any(|c| {
            c.contains("install")
                && c.contains("-g")
                && c.contains("--prefix")
                && c.contains("--ignore-scripts")
                && c.contains("@agenthub/p1-2-fake-npm")
        }),
        "expected contribution npm install command, got {commands:?}"
    );
}

#[test]
fn install_agent_with_contribution_prefers_passed_npm_package() {
    use crate::platform::install::InstallContribution;
    use crate::platform::AgentKey;

    struct OverrideClaude;
    impl InstallContribution for OverrideClaude {
        fn agent_key(&self) -> AgentKey {
            AgentKey::parse("claude").unwrap()
        }
        fn npm_package(&self) -> Option<&'static str> {
            Some("@agenthub/override-claude-pkg")
        }
    }

    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };

    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }

    let _ = install_agent_with_contribution(
        &registry,
        AgentId::Claude,
        &OverrideClaude,
        "npm",
        false,
        &ex,
    )
    .unwrap();
    let commands = calls.lock().unwrap();
    assert!(
        commands
            .iter()
            .any(|c| c.contains("@agenthub/override-claude-pkg")),
        "must use contribution package, not catalog default; got {commands:?}"
    );
    assert!(
        commands
            .iter()
            .all(|c| !c.contains("@anthropic-ai/claude-code")),
        "must not fall back to catalog npm package; got {commands:?}"
    );
}

#[test]
fn npm_and_native_plans_are_defined_for_all_agents() {
    for agent in AgentId::ALL {
        // npm package is optional (WorkBuddy is Setup-only).
        if let Some(pkg) = npm_package(agent) {
            assert!(!pkg.is_empty(), "{}", agent.as_str());
        }
        // Native script URLs are optional (pi npm-only; workbuddy setup URL).
        if let Some(url) = native_ps1_url(agent) {
            assert!(url.starts_with("https://"), "{}", agent.as_str());
        }
        if let Some(url) = native_setup_url(agent) {
            assert!(url.starts_with("https://"), "{}", agent.as_str());
        }
        // Every agent must have at least one install path: npm, ps1/sh, or setup URL.
        let has_plan = npm_package(agent).is_some()
            || native_ps1_url(agent).is_some()
            || native_sh_url(agent).is_some()
            || native_setup_url(agent).is_some();
        assert!(has_plan, "{} must have an install plan", agent.as_str());
    }
    // Historical agents with Windows ps1.
    for agent in [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
    ] {
        assert!(native_ps1_url(agent).is_some(), "{}", agent.as_str());
    }
    // macOS/Linux sh: codex + pi intentionally unsupported (npm-only).
    assert!(native_sh_url(AgentId::Claude).is_some());
    assert!(native_sh_url(AgentId::Kimi).is_some());
    assert!(native_sh_url(AgentId::Grok).is_some());
    assert!(native_sh_url(AgentId::Codex).is_none());
    assert!(native_ps1_url(AgentId::Pi).is_none());
    assert!(native_sh_url(AgentId::Pi).is_none());
    assert_eq!(
        npm_package(AgentId::Pi),
        Some("@earendil-works/pi-coding-agent")
    );
    assert_eq!(npm_install_extra_flags(AgentId::Pi), &["--ignore-scripts"]);
    // WorkBuddy: Setup URL only.
    assert!(npm_package(AgentId::WorkBuddy).is_none());
    assert!(native_ps1_url(AgentId::WorkBuddy).is_none());
    assert!(native_sh_url(AgentId::WorkBuddy).is_none());
    assert_eq!(
        native_setup_url(AgentId::WorkBuddy),
        Some("https://www.codebuddy.cn/work/")
    );
    // Cursor: native script URLs (no npm, no setup-guide-only).
    assert!(npm_package(AgentId::Cursor).is_none());
    assert_eq!(
        native_ps1_url(AgentId::Cursor),
        Some("https://cursor.com/install?win32=true")
    );
    assert_eq!(
        native_sh_url(AgentId::Cursor),
        Some("https://cursor.com/install")
    );
    assert!(native_setup_url(AgentId::Cursor).is_none());
}

#[test]
fn native_uninstall_paths_are_under_home_allowlist() {
    let home = crate::utils::paths::home_dir().unwrap();
    let registry = builtin_install_registry();
    for agent in AgentId::ALL {
        let contribution = registry
            .get_agent_id(agent)
            .expect("builtin install contribution");
        for p in native_uninstall_bin_paths(contribution.as_ref()) {
            assert!(
                p.starts_with(&home),
                "uninstall path must be under home: {}",
                p.display()
            );
        }
        // External uninstallers (WorkBuddy) are path-allowlisted separately.
        for (program, args) in native_uninstaller_specs(contribution.as_ref()) {
            assert!(
                !args.is_empty() || program.as_os_str().len() > 0,
                "uninstaller entry must be non-empty"
            );
            let s = program.to_string_lossy().to_ascii_lowercase();
            assert!(
                s.contains("workbuddy") || s.contains("uninstall"),
                "unexpected uninstaller path: {}",
                program.display()
            );
        }
    }
}

#[test]
fn uninstall_not_installed_fails_without_exec() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: calls.clone(),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    // Use a unique fake: if all real agents are installed on this machine,
    // still assert failure path for NotFound by checking message shape when
    // we force detect — skip if somehow all installed and we can't isolate.
    // Prefer Grok only when not installed.
    let detect = registry.get(AgentId::Grok).unwrap().detect();
    if detect.status == DetectStatus::Installed {
        // Smoke: uninstall path runs and returns a structured outcome (ok may be true/false).
        // Do NOT actually delete real binaries in unit tests — exercise NotFound only when safe.
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&dir.path().join("ah.db")).unwrap();
    let out = uninstall_agent(&registry, &db, AgentId::Grok, false, &ex).unwrap();
    assert!(!out.ok);
    assert!(
        out.message.contains("未安装") || out.message.contains("not"),
        "message={}",
        out.message
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn purge_is_excluded_by_a_provider_live_saga_before_uninstall_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&dir.path().join("ah.db")).unwrap();
    let providers = ProviderService::new(db.clone());
    let guard = providers.begin_live_saga(AgentId::Claude).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let executor = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };

    let error = uninstall_agent(
        &AdapterRegistry::new(),
        &db,
        AgentId::Claude,
        true,
        &executor,
    )
    .unwrap_err();
    assert_eq!(error.code(), "provider.lock");
    assert!(calls.lock().unwrap().is_empty());
    drop(guard);
}

#[test]
fn custom_agent_home_purge_fails_before_external_executor() {
    let _codex = crate::integrations::agents::codex::leftover::lock_codex_home();
    let previous = std::env::var_os("CODEX_HOME");
    let custom = tempfile::tempdir().unwrap();
    std::env::set_var("CODEX_HOME", custom.path());

    let calls = Arc::new(Mutex::new(Vec::new()));
    let executor = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let db_dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&db_dir.path().join("ah.db")).unwrap();
    let error = uninstall_agent(
        &AdapterRegistry::new(),
        &db,
        AgentId::Codex,
        true,
        &executor,
    )
    .expect_err("custom config roots must fail closed");

    match previous {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("custom agent config"));
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn public_purge_entry_rejects_data_dir_unrelated_to_database_authority() {
    let db_dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&db_dir.path().join("ah.db")).unwrap();
    let authority = LiveWriteAuthority::try_from_database(&db).unwrap();
    let unrelated_dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let executor = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };

    let error = uninstall_agent_with_authority_at_data_dir(
        &AdapterRegistry::new(),
        &authority,
        unrelated_dir.path(),
        AgentId::Codex,
        true,
        &executor,
    )
    .expect_err("purge must use the database authority's data root");

    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("does not match"));
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn upgrade_not_installed_fails_closed() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: calls.clone(),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    // Pick agent that is NotFound if any; otherwise skip to avoid network install.
    for agent in AgentId::ALL {
        let d = registry.get(agent).unwrap().detect();
        if d.status != DetectStatus::Installed {
            let out = upgrade_agent(&registry, agent, &ex).unwrap();
            assert!(!out.ok, "upgrade of missing agent must fail");
            assert!(out.message.contains("未安装") || out.message.contains("无法升级"));
            assert!(calls.lock().unwrap().is_empty());
            return;
        }
    }
    // All installed on this machine — still validate API shape via already-installed path
    // is not exercised here (would run real upgrade).
}

#[test]
fn workbuddy_setup_channel_never_reports_success() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let mut logs = Vec::new();

    // WorkBuddy's native channel is an official Setup page, not a scripted
    // installer.  The helper must force a non-success result even when opening
    // the page itself succeeds, so upgrade cannot claim the old binary was
    // upgraded.
    let contribution = builtin_install_registry()
        .get_agent_id(AgentId::WorkBuddy)
        .expect("workbuddy contribution");
    let result = run_native_install(
        contribution.as_ref(),
        AgentId::WorkBuddy.as_str(),
        Some(AgentId::WorkBuddy),
        &ex,
        &mut logs,
    )
    .unwrap();
    assert!(!result.success());
    assert!(logs.iter().any(|line| line.contains("Setup")));
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn failed_upgrade_command_does_not_claim_existing_binary_is_upgraded() {
    assert!(!upgrade_succeeded(false, &DetectStatus::Installed));
    assert!(!upgrade_succeeded(true, &DetectStatus::NotFound));
    assert!(upgrade_succeeded(true, &DetectStatus::Installed));
}

#[test]
fn install_runtime_powershell_logs_dual_version_context() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::PowerShell, "winget", &ex).unwrap();
    assert!(!out.ok);
    // Firefighting: refuse message + notes from detect_powershell should appear in logs.
    assert!(
        out.logs
            .iter()
            .any(|l| l.contains("PowerShell") || l.contains("pwsh") || l.contains("5.1")),
        "logs should include PS dual-version context: {:?}",
        out.logs
    );
}

#[test]
fn install_runtime_git_uses_winget_git_package() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: "Successfully installed".into(),
            stderr: String::new(),
    };
    // Even if git is already present, we still invoke the platform package
    // manager (then redetect). If it is missing, resolution fails before the
    // executor runs.
    let channel = default_runtime_channel();
    let out = install_runtime(RuntimeId::Git, channel, &ex).unwrap();
    let cmds = calls.lock().unwrap();
    if cmds.is_empty() {
        // Linux is always manual. Other hosts reach this when brew/winget is missing.
        assert!(!out.ok);
        assert_eq!(out.code.as_deref(), Some("env.not_ready"));
        assert!(
            out.message.contains("winget")
                || out.message.contains("Homebrew")
                || out.message.contains("Linux")
                || out.message.contains("manual")
                || out.logs.iter().any(|l| {
                    l.contains("winget")
                        || l.contains("brew")
                        || l.contains("apt-get")
                        || l.contains("dnf")
                        || l.contains("pacman")
                        || l.contains("manual")
                }),
            "expected package-manager-missing or Linux manual path: msg={} logs={:?}",
            out.message,
            out.logs
        );
    } else {
        assert!(
            cmds.iter().any(|c| {
                if cfg!(target_os = "macos") {
                    c.contains("brew") && c.contains("install git")
                } else if cfg!(windows) {
                    c.contains("Git.Git")
                } else {
                    false
                }
            }),
            "expected platform package install, got {cmds:?}"
        );
        assert!(
            out.logs
                .iter()
                .any(|l| l.contains("Git.Git") || l.contains("git") || l.contains("brew")),
            "logs should mention git package: {:?}",
            out.logs
        );
    }
}

#[test]
fn host_remediations_omit_foreign_package_managers() {
    for id in [RuntimeId::NodeJs, RuntimeId::Git] {
        let remediations = host_remediations(id);
        assert!(
            !remediations.is_empty(),
            "expected at least one host remediation for {}",
            id.as_str()
        );
        for rem in remediations {
            if cfg!(windows) {
                assert_ne!(rem.kind, "brew", "Windows must not suggest brew");
                if let Some(command) = rem.command.as_deref() {
                    assert!(
                        !command.to_ascii_lowercase().contains("apt"),
                        "Windows must not suggest apt: {command}"
                    );
                }
            } else if cfg!(target_os = "macos") {
                assert_ne!(rem.kind, "winget", "macOS must not suggest winget");
                if let Some(command) = rem.command.as_deref() {
                    assert!(
                        !command.to_ascii_lowercase().contains("apt"),
                        "macOS must not suggest apt: {command}"
                    );
                }
            } else {
                assert_ne!(rem.kind, "winget", "Linux must not suggest winget");
                assert_ne!(rem.kind, "brew", "Linux must not suggest brew");
            }
        }
    }
}

#[test]
fn missing_package_manager_outcome_is_env_not_ready() {
    let out = missing_package_manager_outcome(
        "env_install",
        vec!["# install runtime nodejs via brew (node)".into()],
        "brew",
        RuntimeId::NodeJs,
        "未找到 Homebrew。请先安装 Homebrew（https://brew.sh/）后重试。",
    );
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("env.not_ready"));
    let details = out.details.expect("env.not_ready details");
    assert!(details["agent"].is_null());
    assert_eq!(details["channel"], "brew");
    assert_eq!(details["missing"], serde_json::json!(["nodejs"]));
    let remediations = details["remediations"].as_array().expect("remediations");
    assert!(!remediations.is_empty());
    for rem in remediations {
        let kind = rem["kind"].as_str().unwrap_or_default();
        if cfg!(windows) {
            assert_ne!(kind, "brew");
        } else {
            assert_ne!(kind, "winget");
        }
        assert!(
            rem.get("command").is_some() || rem.get("url").is_some() || rem.get("text").is_some(),
            "remediation should carry command, url, or text: {rem}"
        );
    }
    assert!(details["hint"]
        .as_str()
        .unwrap_or_default()
        .contains("Homebrew"));
    assert!(
        out.logs
            .iter()
            .any(|line| line.contains("remediation") || line.contains("https://")),
        "logs should print remediations: {:?}",
        out.logs
    );
}

#[test]
fn missing_winget_outcome_normalizes_npm_to_nodejs() {
    let out = missing_package_manager_outcome(
        "env_install",
        Vec::new(),
        "winget",
        RuntimeId::NodeJs,
        "未找到 winget。请手动安装 Node.js LTS 后重新检测。",
    );
    assert_eq!(out.code.as_deref(), Some("env.not_ready"));
    let details = out.details.expect("details");
    assert_eq!(details["channel"], "winget");
    assert_eq!(details["missing"], serde_json::json!(["nodejs"]));
}

#[test]
fn install_runtime_unsupported_channel_is_coded() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let channel = if cfg!(all(not(windows), not(target_os = "macos"))) {
        "chocolatey"
    } else {
        "apt"
    };
    let out = install_runtime(RuntimeId::NodeJs, channel, &ex).unwrap();
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("unsupported"));
    let details = out.details.expect("unsupported details");
    assert_eq!(details["channel"], channel);
    assert!(
        details["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("winget")
            || details["hint"]
                .as_str()
                .unwrap_or_default()
                .contains("brew")
            || details["hint"]
                .as_str()
                .unwrap_or_default()
                .contains("manual")
            || details["hint"]
                .as_str()
                .unwrap_or_default()
                .contains("Linux"),
        "details.hint should mention the platform channel: {details}"
    );
    assert!(ex.calls.lock().unwrap().is_empty());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn install_runtime_brew_channel_unsupported_off_macos() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::Git, "brew", &ex).unwrap();
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("unsupported"));
    assert_eq!(out.details.as_ref().unwrap()["channel"], "brew");
    assert!(ex.calls.lock().unwrap().is_empty());
}

#[test]
fn install_runtime_missing_winget_is_env_not_ready() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::Npm, "winget", &ex).unwrap();
    let cmds = calls.lock().unwrap();
    if cfg!(not(windows)) {
        assert!(cmds.is_empty());
        assert!(!out.ok);
        assert_eq!(out.code.as_deref(), Some("unsupported"));
        assert_eq!(out.details.as_ref().unwrap()["channel"], "winget");
        return;
    }
    if cmds.is_empty() {
        assert!(!out.ok);
        assert_eq!(out.code.as_deref(), Some("env.not_ready"));
        let details = out.details.expect("env.not_ready details");
        assert!(details["agent"].is_null());
        assert_eq!(details["channel"], "winget");
        assert_eq!(details["missing"], serde_json::json!(["nodejs"]));
        let remediations = details["remediations"].as_array().expect("remediations");
        assert!(!remediations.is_empty());
        for rem in remediations {
            if cfg!(windows) {
                assert_ne!(rem["kind"], "brew");
            } else {
                assert_ne!(rem["kind"], "winget");
            }
        }
        assert!(
            out.logs
                .iter()
                .any(|line| line.contains("remediation") || line.contains("https://")),
            "logs should print remediations: {:?}",
            out.logs
        );
    } else {
        // winget is on PATH: executor ran, so this is the redetect path, not env.not_ready.
        assert_ne!(out.code.as_deref(), Some("env.not_ready"));
        assert_ne!(out.code.as_deref(), Some("unsupported"));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn install_runtime_missing_brew_is_env_not_ready() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::Git, "brew", &ex).unwrap();
    let cmds = calls.lock().unwrap();
    if cmds.is_empty() {
        assert!(!out.ok);
        assert_eq!(out.code.as_deref(), Some("env.not_ready"));
        let details = out.details.expect("env.not_ready details");
        assert_eq!(details["channel"], "brew");
        assert_eq!(details["missing"], serde_json::json!(["git"]));
        let remediations = details["remediations"].as_array().expect("remediations");
        assert!(!remediations.is_empty());
        for rem in remediations {
            assert_ne!(rem["kind"], "winget");
        }
    } else {
        assert_ne!(out.code.as_deref(), Some("env.not_ready"));
        assert_ne!(out.code.as_deref(), Some("unsupported"));
    }
}

#[test]
fn finalize_runtime_install_does_not_set_business_code() {
    let res = ExecResult {
        command: "winget install -e --id Git.Git".into(),
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "failed".into(),
        timed_out: false,
        spawn_error: None,
    };
    // Prefer a runtime that is not ready so this stays on the execute-then-redetect
    // failure path. If every runtime is already ok, success-after-nonzero also
    // must not carry env.not_ready / unsupported.
    let id = [
        RuntimeId::PowerShell,
        RuntimeId::Git,
        RuntimeId::NodeJs,
        RuntimeId::Npm,
    ]
    .into_iter()
    .find(|id| runtime::detect_one(*id).status != EnvStatusKind::Ok)
    .unwrap_or(RuntimeId::Git);
    let out = finalize_runtime_install(id, vec!["# ran winget".into()], res);
    if runtime::detect_one(id).status != EnvStatusKind::Ok && id != RuntimeId::NodeJs {
        assert!(!out.ok, "redetect of missing {} must fail", id.as_str());
    }
    assert!(
        out.code.is_none(),
        "executed install path must stay install.failed, got {:?}",
        out.code
    );
    assert!(out.details.is_none());
}

#[test]
fn grok_native_url_is_official_cli_allowlist() {
    let url = native_ps1_url(AgentId::Grok).unwrap();
    assert!(url.starts_with("https://"));
    assert!(
        url.contains("x.ai") && url.contains("install.ps1"),
        "unexpected grok url {url}"
    );
    let sh = native_sh_url(AgentId::Grok).unwrap();
    assert!(sh.contains("install.sh"));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_default_runtime_install_is_manual_and_does_not_spawn() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::NodeJs, "", &ex).unwrap();
    assert!(calls.lock().unwrap().is_empty());
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("env.not_ready"));
    let details = out.details.expect("manual remediations");
    assert_eq!(details["channel"], "manual");
    assert_eq!(details["missing"], serde_json::json!(["nodejs"]));
    let remediations = details["remediations"].as_array().expect("remediations");
    assert!(!remediations.is_empty());
    for rem in remediations {
        assert_ne!(rem["kind"], "winget");
        assert_ne!(rem["kind"], "brew");
    }
    assert!(
        out.message.contains("Linux")
            || out.message.contains("manual")
            || out.message.contains("包管理器"),
        "expected a Linux manual message: {}",
        out.message
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_rejects_explicit_winget_and_brew_without_spawning() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    for channel in ["winget", "brew"] {
        let out = install_runtime(RuntimeId::Git, channel, &ex).unwrap();
        assert!(!out.ok, "{channel}");
        assert_eq!(out.code.as_deref(), Some("unsupported"), "{channel}");
        assert_eq!(out.details.as_ref().unwrap()["channel"], channel);
        assert!(ex.calls.lock().unwrap().is_empty());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_accepts_apt_as_copy_command_channel_without_spawning() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
            stderr: String::new(),
    };
    for channel in ["apt", "dnf", "pacman", "zypper", "apk"] {
        let out = install_runtime(RuntimeId::Git, channel, &ex).unwrap();
        assert!(!out.ok, "{channel}");
        assert_eq!(out.code.as_deref(), Some("env.not_ready"), "{channel}");
        let details = out.details.as_ref().expect(channel);
        assert_eq!(details["channel"], channel, "{channel}");
        let remediations = details["remediations"].as_array().expect(channel);
        assert!(!remediations.is_empty(), "{channel}");
        for rem in remediations {
            assert_ne!(rem["kind"], "winget", "{channel}");
            assert_ne!(rem["kind"], "brew", "{channel}");
        }
        assert!(calls.lock().unwrap().is_empty(), "{channel}");
    }
}

#[test]
fn user_npm_prefix_is_under_agenthub_data_dir() {
    let prefix = user_npm_prefix().unwrap();
    let data = crate::utils::paths::resolve_data_dir(None).unwrap();
    assert_eq!(prefix, data.join("npm"));
}

#[test]
fn npm_nonzero_permission_failure_is_not_blamed_on_path() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 243,
        stdout: String::new(),
        stderr: "npm ERR! code EACCES\nnpm ERR! syscall mkdir".into(),
    };
    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }
    let out = install_agent(&registry, AgentId::Codex, "npm", false, &ex).unwrap();
    assert!(!out.ok);
    assert!(out.message.contains("失败"), "msg={}", out.message);
    assert!(out.message.contains("EACCES") || out.message.contains("权限"), "msg={}", out.message);
    let joined = out.logs.join("\n");
    assert!(!joined.contains("可能已成功"), "logs={joined}");
    assert!(!out.message.contains("重新检测未找到二进制"), "msg={}", out.message);
    assert!(!out.message.contains("请检查 PATH"), "msg={}", out.message);
    let commands = calls.lock().unwrap();
    assert!(commands.iter().any(|c| c.contains("--prefix")), "expected prefix, got {commands:?}");
}

struct StickyNpmAdapter {
    id: AgentId,
    /// Number of `detect` calls already observed. 0 = preflight (Installed).
    detect_calls: Arc<AtomicUsize>,
    /// After this many detect calls (inclusive start), report NotFound.
    not_found_from: usize,
}

impl AgentAdapter for StickyNpmAdapter {
    fn id(&self) -> AgentId {
        self.id
    }

    fn detect(&self) -> DetectResult {
        let n = self.detect_calls.fetch_add(1, Ordering::SeqCst);
        let installed = n < self.not_found_from;
        DetectResult {
            agent: self.id,
            status: if installed {
                DetectStatus::Installed
            } else {
                DetectStatus::NotFound
            },
            version: Some("1.0.0".into()),
            binary_path: installed.then(|| PathBuf::from("/tmp/fake-npm-agent")),
            channel: Some("npm".into()),
            env_ready: true,
            notes: vec![],
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn read_auth(&self) -> Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn capability(&self, _cap: Capability) -> CapabilityState {
        CapabilityState::unsupported("fake")
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![]
    }

    fn build_run_spec(&self, _binary: &Path, _prompt: &str, _opts: &RunOptions) -> Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn uninstall_calls_for(
    not_found_from: usize,
    executor: &MockExecutor,
) -> (InstallOutcome, Vec<String>) {
    let detect_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(StickyNpmAdapter {
        id: AgentId::Codex,
        detect_calls,
        not_found_from,
    }));
    let dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&dir.path().join("ah.db")).unwrap();
    let out = uninstall_agent(&registry, &db, AgentId::Codex, false, executor).unwrap();
    let commands = executor.calls.lock().unwrap().clone();
    (out, commands)
}

#[test]
fn npm_uninstall_retries_without_prefix_when_detect_still_installed() {
    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    // detect 0 preflight Installed; after prefix still Installed → second uninstall.
    let (out, commands) = uninstall_calls_for(usize::MAX, &ex);
    let uninstalls: Vec<_> = commands
        .iter()
        .filter(|c| c.contains("uninstall"))
        .cloned()
        .collect();
    assert_eq!(
        uninstalls.len(),
        2,
        "prefix success + still Installed must retry legacy global; got {commands:?}"
    );
    assert!(
        uninstalls[0].contains("--prefix"),
        "first uninstall must use user prefix: {}",
        uninstalls[0]
    );
    assert!(
        !uninstalls[1].contains("--prefix"),
        "second uninstall must be legacy global: {}",
        uninstalls[1]
    );
    assert!(
        out.logs.iter().any(|l| l.contains("uninstall") && l.contains("--prefix")),
        "logs must record prefix uninstall with actual path: {:?}",
        out.logs
    );
    assert!(
        out.logs
            .iter()
            .any(|l| l.contains("# npm uninstall -g ") && !l.contains("--prefix")),
        "logs must record legacy uninstall: {:?}",
        out.logs
    );
    assert!(
        out.logs.iter().all(|l| !l.contains("~/.agenthub/npm")),
        "must not hardcode ~/.agenthub/npm: {:?}",
        out.logs
    );
}

#[test]
fn npm_uninstall_skips_legacy_when_prefix_clears_detect() {
    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    // detect 0 preflight Installed; after prefix (detect 1) NotFound → no retry.
    let (_out, commands) = uninstall_calls_for(1, &ex);
    let uninstalls: Vec<_> = commands
        .iter()
        .filter(|c| c.contains("uninstall"))
        .cloned()
        .collect();
    assert_eq!(
        uninstalls.len(),
        1,
        "clean prefix uninstall must not emit legacy global; got {commands:?}"
    );
    assert!(uninstalls[0].contains("--prefix"), "{}", uninstalls[0]);
}

#[test]
fn contribution_uninstall_retries_legacy_global_without_prefix() {
    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }

    struct FakeContrib;
    impl InstallContribution for FakeContrib {
        fn agent_key(&self) -> AgentKey {
            AgentKey::parse("p1-2-fake-npm").unwrap()
        }
        fn npm_package(&self) -> Option<&'static str> {
            Some("@agenthub/p1-2-fake-npm")
        }
    }

    let key = AgentKey::parse("p1-2-fake-npm").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let out = uninstall_from_contribution(&key, &FakeContrib, false, &ex).unwrap();
    let commands = calls.lock().unwrap();
    let uninstalls: Vec<_> = commands
        .iter()
        .filter(|c| c.contains("uninstall"))
        .cloned()
        .collect();
    assert_eq!(
        uninstalls.len(),
        2,
        "contribution uninstall has no detect; must try prefix then legacy: {commands:?}"
    );
    assert!(uninstalls[0].contains("--prefix"), "{}", uninstalls[0]);
    assert!(!uninstalls[1].contains("--prefix"), "{}", uninstalls[1]);
    assert!(out.ok, "msg={}", out.message);
    assert!(
        out.logs.iter().all(|l| !l.contains("~/.agenthub/npm")),
        "must not hardcode ~/.agenthub/npm: {:?}",
        out.logs
    );
}
