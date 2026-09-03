use super::*;
use crate::adapters::{is_under_agenthub_user_npm_prefix, register_all, AgentAdapter};
use crate::catalog::install::{
    native_ps1_url, native_setup_url, native_sh_url, npm_install_extra_flags, npm_package,
};
use crate::error::AppError;
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, EnvStatusKind, InstallChannel,
    RunOptions, RunSpec,
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
    let user_prefix = detect_scanned_user_npm_prefix()
        .expect("user-writable npm prefix")
        .display()
        .to_string();
    assert!(
        commands.iter().any(|c| {
            c.contains("install")
                && c.contains("-g")
                && c.contains("--prefix")
                && c.contains(&user_prefix)
                && !c.contains(".agenthub")
                && c.contains("--ignore-scripts")
                && c.contains("@agenthub/p1-2-fake-npm")
        }),
        "expected contribution npm install into detect-scanned user prefix {user_prefix}, got {commands:?}"
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
    let _env = crate::utils::test_env::lock_test_env();
    let _codex = crate::integrations::agents::codex::leftover::lock_codex_home();
    let custom = tempfile::tempdir().unwrap();
    let _codex_home = crate::utils::test_env::EnvVarGuard::set("CODEX_HOME", custom.path());

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

    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("custom agent config"));
    assert!(calls.lock().unwrap().is_empty());
}

fn purge_must_fail_closed_for_env_override(agent: AgentId, key: &'static str) {
    // Caller holds the shared test-env lock; the guard restores the key even
    // if the expect_err below panics.
    let custom = tempfile::tempdir().unwrap();
    let _guard = crate::utils::test_env::EnvVarGuard::set(key, custom.path());

    let calls = Arc::new(Mutex::new(Vec::new()));
    let executor = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let db_dir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&db_dir.path().join("ah.db")).unwrap();
    let error = uninstall_agent(&AdapterRegistry::new(), &db, agent, true, &executor)
        .expect_err("custom config roots must fail closed");

    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("custom agent config"));
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn custom_kimi_code_home_purge_fails_before_external_executor() {
    let _env = crate::utils::test_env::lock_test_env();
    purge_must_fail_closed_for_env_override(AgentId::Kimi, "KIMI_CODE_HOME");
}

#[test]
fn custom_pi_config_dir_purge_fails_before_external_executor() {
    let _env = crate::utils::test_env::lock_test_env();
    purge_must_fail_closed_for_env_override(AgentId::Pi, "PI_CODING_AGENT_DIR");
}

#[test]
fn custom_workbuddy_config_dir_purge_fails_before_external_executor() {
    let _env = crate::utils::test_env::lock_test_env();
    purge_must_fail_closed_for_env_override(AgentId::WorkBuddy, "WORKBUDDY_CONFIG_DIR");
}

#[test]
fn custom_codebuddy_config_dir_purge_fails_before_external_executor() {
    let _env = crate::utils::test_env::lock_test_env();
    let _workbuddy = crate::utils::test_env::EnvVarGuard::remove("WORKBUDDY_CONFIG_DIR");
    purge_must_fail_closed_for_env_override(AgentId::WorkBuddy, "CODEBUDDY_CONFIG_DIR");
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
fn special_channel_kind_blocks_program_uninstall_copy() {
    assert_eq!(special_channel_kind(Some("ide")), Some("ide"));
    assert_eq!(special_channel_kind(Some("DESKTOP")), Some("desktop"));
    assert_eq!(special_channel_kind(Some("npm")), None);
    assert_eq!(special_channel_kind(None), None);
    let ide = special_uninstall_program_message("ide");
    assert!(ide.contains("IDE"), "{ide}");
    let desktop = special_uninstall_program_message("desktop");
    assert!(
        desktop.contains("Microsoft Store") || desktop.contains("桌面"),
        "{desktop}"
    );
}

#[test]
fn resolve_in_app_upgrade_channel_blocks_ide_and_desktop() {
    assert_eq!(resolve_in_app_upgrade_channel(Some("npm")).unwrap(), "npm");
    assert_eq!(
        resolve_in_app_upgrade_channel(Some("native")).unwrap(),
        "native"
    );
    assert_eq!(resolve_in_app_upgrade_channel(None).unwrap(), "native");
    let ide = resolve_in_app_upgrade_channel(Some("ide")).unwrap_err();
    assert!(ide.contains("IDE"), "{ide}");
    let desktop = resolve_in_app_upgrade_channel(Some("desktop")).unwrap_err();
    assert!(desktop.contains("桌面"), "{desktop}");
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
    assert!(
        logs.iter().any(|line| line.contains("官网安装页")),
        "expected Chinese setup-guide copy, got {logs:?}"
    );
    assert!(
        logs.iter()
            .all(|line| !line.to_ascii_lowercase().contains("failed")),
        "opening the official page is not an installer failure: {logs:?}"
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "setup guide opens the official page without the install executor: {:?}",
        calls.lock().unwrap()
    );
}

#[test]
fn workbuddy_native_install_is_setup_guide_not_failure() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let out = install_agent(&registry, AgentId::WorkBuddy, "native", false, &ex).unwrap();
    if out.ok {
        // Already installed on this machine: still must not have installed
        // into leftover AgentHub data-dir npm.
        assert_ne!(out.code.as_deref(), Some("setup_guide"));
        return;
    }
    assert!(
        calls.lock().unwrap().is_empty(),
        "setup guide opens the official page without the install executor: {:?}",
        calls.lock().unwrap()
    );
    assert_eq!(out.code.as_deref(), Some("setup_guide"));
    assert!(
        !out.message.contains("失败"),
        "UI must not treat opening the official page as install failed: {}",
        out.message
    );
    assert!(
        out.message.contains("官网安装页"),
        "expected Chinese setup-guide message, got {}",
        out.message
    );
    assert_eq!(
        out.logs.first().map(String::as_str),
        Some(setup_guide_diagnosis())
    );
    let joined = out.logs.join("\n");
    assert!(joined.contains("诊断："));
    assert!(!joined.contains("安装命令未成功退出"));
}

#[test]
fn setup_guide_contribution_is_not_command_failure() {
    struct GuideContrib;
    impl InstallContribution for GuideContrib {
        fn agent_key(&self) -> AgentKey {
            AgentKey::parse("guide-only").unwrap()
        }
        fn native_setup_url(&self) -> Option<&'static str> {
            Some("https://www.codebuddy.cn/work/")
        }
    }

    let key = AgentKey::parse("guide-only").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let out = install_from_contribution(&key, &GuideContrib, "native", false, &ex).unwrap();
    assert!(!out.ok);
    assert_eq!(out.code.as_deref(), Some("setup_guide"));
    assert!(!out.message.contains("失败"));
    assert!(out.message.contains("官网安装页"));
    assert_eq!(
        out.logs.first().map(String::as_str),
        Some(setup_guide_diagnosis())
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "setup guide opens the official page without the install executor: {:?}",
        calls.lock().unwrap()
    );
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
fn pick_nodejs_macos_lts_pkg_selects_newest_lts_pkg() {
    let json = r#"[
      {"version":"v23.1.0","lts":false,"files":["pkg"]},
      {"version":"v22.19.0","lts":"Jod","files":["osx-arm64-tar","pkg"]},
      {"version":"v20.19.5","lts":"Iron","files":["pkg"]}
    ]"#;
    let (version, url) = pick_nodejs_macos_lts_pkg(json).expect("lts pkg");
    assert_eq!(version, "22.19.0");
    assert_eq!(url, "https://nodejs.org/dist/v22.19.0/node-v22.19.0.pkg");
}

#[test]
fn pick_nodejs_macos_lts_pkg_rejects_unsafe_version() {
    let json = r#"[{"version":"v22.19.0-evil/../../tmp","lts":"Jod","files":["pkg"]}]"#;
    assert!(pick_nodejs_macos_lts_pkg(json).is_none());
    assert!(!is_safe_node_version("22.19.0-rc.1"));
    assert!(is_safe_node_version("22.19.0"));
}

#[cfg(target_os = "macos")]
#[test]
fn osascript_installer_script_uses_quoted_form() {
    let script = osascript_installer_script(Path::new("/tmp/agenthub-node-test.pkg"));
    assert!(script.contains("quoted form of"));
    assert!(script.contains("/usr/sbin/installer"));
    assert!(script.contains("/tmp/agenthub-node-test.pkg"));
    assert!(script.contains("administrator privileges"));
}

#[cfg(target_os = "macos")]
#[test]
fn install_runtime_nodejs_without_brew_fetches_official_index() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let out = install_runtime(RuntimeId::NodeJs, "brew", &ex).unwrap();
    let cmds = calls.lock().unwrap();
    if cmds.iter().any(|c| c.contains("brew")) {
        return;
    }
    assert!(
        cmds.iter()
            .any(|c| c.contains("https://nodejs.org/dist/index.json"))
            || out.code.as_deref() == Some("env.not_ready"),
        "expected official Node index fetch when brew is missing, got cmds={cmds:?} out={out:?}"
    );
    assert!(!out.ok);
}

#[test]
fn runtime_package_action_upgrades_ready_or_outdated() {
    assert_eq!(
        runtime_package_action(EnvStatusKind::Ok),
        RuntimePackageAction::Upgrade
    );
    assert_eq!(
        runtime_package_action(EnvStatusKind::Outdated),
        RuntimePackageAction::Upgrade
    );
    assert_eq!(
        runtime_package_action(EnvStatusKind::Missing),
        RuntimePackageAction::Install
    );
    assert_eq!(
        runtime_package_action(EnvStatusKind::BrokenPath),
        RuntimePackageAction::Install
    );
    assert_eq!(package_manager_verb(RuntimePackageAction::Install), "install");
    assert_eq!(package_manager_verb(RuntimePackageAction::Upgrade), "upgrade");
    assert_eq!(package_manager_zh(RuntimePackageAction::Install), "安装");
    assert_eq!(package_manager_zh(RuntimePackageAction::Upgrade), "升级");
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
                    c.contains("brew") && (c.contains("install git") || c.contains("upgrade git"))
                } else if cfg!(windows) {
                    c.contains("Git.Git") && (c.contains("install") || c.contains("upgrade"))
                } else {
                    false
                }
            }),
            "expected platform package install or upgrade, got {cmds:?}"
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
        "未找到 Homebrew，无法一键安装。请先安装 Homebrew（https://brew.sh/），或从官网手动安装。完成后完全退出并重启 AgentHub 再检测。",
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
        remediations.iter().any(|rem| {
            rem["url"].as_str() == Some("https://brew.sh/")
                || rem["url"].as_str() == Some("https://nodejs.org/")
                || rem["command"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Homebrew/install")
        }),
        "missing Homebrew must point at brew.sh or the official runtime page: {remediations:?}"
    );
    assert!(
        remediations
            .iter()
            .all(|rem| rem["command"].as_str() != Some("brew install node")),
        "must not suggest brew install when Homebrew is missing: {remediations:?}"
    );
    assert!(
        out.logs.iter().any(|line| line.contains("https://")
            || line.contains("可复制命令")
            || line.contains("打开页面")),
        "logs should print install steps: {:?}",
        out.logs
    );
    assert!(
        out.logs.iter().all(|line| !line.contains("remediation:")),
        "logs must not use internal remediation prefixes: {:?}",
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
            out.logs.iter().any(|line| line.contains("https://")
                || line.contains("可复制命令")
                || line.contains("打开页面")),
            "logs should print install steps: {:?}",
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
fn leftover_agenthub_npm_prefix_is_under_data_dir() {
    let prefix = leftover_agenthub_npm_prefix().unwrap();
    let data = crate::utils::paths::resolve_data_dir(None).unwrap();
    assert_eq!(prefix, data.join("npm"));
}

#[test]
fn detect_scanned_user_npm_prefix_is_not_leftover() {
    let prefix = detect_scanned_user_npm_prefix().unwrap();
    let leftover = leftover_agenthub_npm_prefix().unwrap();
    assert_ne!(
        prefix, leftover,
        "Codex/Pi/DSH must not install into leftover data-dir npm"
    );
    assert!(
        !is_under_agenthub_user_npm_prefix(&prefix),
        "user install prefix {prefix:?} must stay outside leftover roots"
    );
    assert!(
        leftover_agenthub_npm_prefix_candidates()
            .iter()
            .all(|p| p != &prefix),
        "user prefix {prefix:?} must not match leftover candidates"
    );
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
    assert_ne!(out.code.as_deref(), Some("setup_guide"));
    assert!(out.message.contains("失败"), "msg={}", out.message);
    assert!(
        out.message.contains("EACCES") || out.message.contains("权限"),
        "msg={}",
        out.message
    );
    assert!(
        out.logs
            .first()
            .is_some_and(|line| line.starts_with("诊断：") && line.contains("写入权限")),
        "diagnosis must sit above raw installer output, got {:?}",
        out.logs
    );
    let joined = out.logs.join("\n");
    assert!(!joined.contains("可能已成功"), "logs={joined}");
    assert!(
        !out.message.contains("重新检测未找到二进制"),
        "msg={}",
        out.message
    );
    assert!(!out.message.contains("请检查 PATH"), "msg={}", out.message);
}

#[test]
fn installer_fail_panel_collapses_npm_http_progress() {
    let noisy: Vec<String> = (0..80)
        .map(|i| {
            format!("npm http fetch GET 200 https://registry.npmjs.org/@openai/codex/-/{i}.tgz")
        })
        .collect();
    let mut body = vec!["$ npm install -g @openai/codex".into()];
    body.extend(noisy);
    body.push("npm ERR! code EACCES".into());
    body.push("npm ERR! syscall mkdir".into());
    let summarized = summarize_installer_output_lines(body);
    assert_eq!(summarized[0], "$ npm install -g @openai/codex");
    assert!(
        summarized
            .iter()
            .any(|line| line.contains("已省略") && line.contains("下载进度")),
        "expected collapsed download progress, got {summarized:?}"
    );
    assert!(summarized.iter().any(|line| line.contains("EACCES")));
    assert!(
        summarized
            .iter()
            .filter(|line| line.contains("http fetch") || line.contains("registry.npmjs.org"))
            .count()
            < 5,
        "raw npm HTTP must not be the fail-panel body: {summarized:?}"
    );
    assert!(
        summarized.len() < 20,
        "too many lines: {}",
        summarized.len()
    );
}

#[test]
fn npm_nonzero_permission_failure_keeps_diagnosis_and_collapses_http() {
    let registry = register_all();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut stderr = String::new();
    for i in 0..60 {
        stderr.push_str(&format!(
            "npm http fetch GET 200 https://registry.npmjs.org/pkg-{i}\n"
        ));
    }
    stderr.push_str("npm ERR! code EACCES\nnpm ERR! syscall mkdir\n");
    let ex = MockExecutor {
        calls: Arc::clone(&calls),
        exit_code: 243,
        stdout: String::new(),
        stderr,
    };
    if !runtime::is_ready(&[RuntimeId::NodeJs, RuntimeId::Npm]) {
        return;
    }
    let out = install_agent(&registry, AgentId::Codex, "npm", false, &ex).unwrap();
    assert!(!out.ok);
    assert!(
        out.logs
            .first()
            .is_some_and(|line| line.starts_with("诊断：") && line.contains("写入权限")),
        "diagnosis first, got {:?}",
        out.logs
    );
    assert!(
        out.logs
            .iter()
            .any(|line| line.contains("已省略") && line.contains("下载进度")),
        "fail panel must collapse npm HTTP, got {:?}",
        out.logs
    );
    assert!(
        out.logs
            .iter()
            .filter(|line| line.contains("http fetch"))
            .count()
            < 3,
        "raw HTTP dumped: {:?}",
        out.logs
    );
    let commands = calls.lock().unwrap();
    let user_prefix = detect_scanned_user_npm_prefix()
        .expect("user-writable npm prefix")
        .display()
        .to_string();
    assert!(
        commands.iter().any(|c| {
            c.contains("install")
                && c.contains("-g")
                && c.contains("--prefix")
                && c.contains(&user_prefix)
                && !c.contains(".agenthub")
        }),
        "expected npm install into detect-scanned user prefix {user_prefix}, got {commands:?}"
    );
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
            extra_copies: Vec::new(),
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
fn npm_uninstall_uses_global_then_optional_leftover_prefix() {
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
    let (out, commands) = uninstall_calls_for(usize::MAX, &ex);
    let uninstalls: Vec<_> = commands
        .iter()
        .filter(|c| c.contains("uninstall"))
        .cloned()
        .collect();
    assert!(
        !uninstalls.is_empty(),
        "expected at least a global npm uninstall; got {commands:?}"
    );
    assert!(
        uninstalls
            .iter()
            .any(|c| c.contains("uninstall") && !c.contains("--prefix")),
        "legacy global uninstall required: {uninstalls:?}"
    );
    if leftover_agenthub_npm_prefix_present().is_some() {
        assert!(
            uninstalls
                .iter()
                .any(|c| c.contains("--prefix") && c.contains("npm")),
            "leftover data-dir npm must also be uninstalled: {uninstalls:?}"
        );
    }
    assert!(
        out.logs
            .iter()
            .any(|l| l.contains("# npm uninstall -g ") && !l.contains("--prefix")),
        "logs must record global uninstall: {:?}",
        out.logs
    );
    assert!(
        out.logs.iter().all(|l| !l.contains("~/.agenthub/npm")),
        "must not hardcode ~/.agenthub/npm: {:?}",
        out.logs
    );
}

#[test]
fn npm_uninstall_always_emits_global_uninstall() {
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
    let (_out, commands) = uninstall_calls_for(1, &ex);
    let uninstalls: Vec<_> = commands
        .iter()
        .filter(|c| c.contains("uninstall"))
        .cloned()
        .collect();
    assert!(
        uninstalls
            .iter()
            .any(|c| c.contains("uninstall") && !c.contains("--prefix")),
        "global uninstall required: {uninstalls:?}"
    );
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
    assert!(
        uninstalls
            .iter()
            .any(|c| c.contains("uninstall") && !c.contains("--prefix")),
        "contribution uninstall must use real npm global: {commands:?}"
    );
    assert!(out.ok, "msg={}", out.message);
    assert!(
        out.logs.iter().all(|l| !l.contains("~/.agenthub/npm")),
        "must not hardcode ~/.agenthub/npm: {:?}",
        out.logs
    );
}
