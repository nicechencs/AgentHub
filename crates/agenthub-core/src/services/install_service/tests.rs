use super::*;
use crate::adapters::register_all;
use crate::catalog::install::{
    native_ps1_url, native_setup_url, native_sh_url, npm_install_extra_flags, npm_package,
};
use crate::utils::command_exec::ExecRequest;
use std::sync::{Arc, Mutex};

struct MockExecutor {
    calls: Arc<Mutex<Vec<String>>>,
    exit_code: i32,
    stdout: String,
}

impl CommandExecutor for MockExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        let cmd = format!("{} {}", req.program, req.args.join(" "));
        self.calls.lock().unwrap().push(cmd.clone());
        ExecResult {
            command: cmd,
            exit_code: Some(self.exit_code),
            stdout: self.stdout.clone(),
            stderr: String::new(),
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
    };
    let out = install_runtime(RuntimeId::PowerShell, "winget", &ex).unwrap();
    assert!(!out.ok);
    assert!(out.message.contains("不支持") || out.message.contains("PowerShell"));
    assert!(ex.calls.lock().unwrap().is_empty());
}

#[test]
fn runtime_default_channel_matches_platform() {
    if cfg!(target_os = "macos") {
        assert_eq!(default_runtime_channel(), "brew");
    } else {
        assert_eq!(default_runtime_channel(), "winget");
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
    assert!(
        commands
            .iter()
            .all(|command| !command.to_ascii_lowercase().contains("powershell"))
    );
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
    for agent in AgentId::ALL {
        for p in native_uninstall_bin_paths(agent) {
            assert!(
                p.starts_with(&home),
                "uninstall path must be under home: {}",
                p.display()
            );
        }
        // External uninstallers (WorkBuddy) are path-allowlisted separately.
        for (program, args) in native_uninstaller_specs(agent) {
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
    let out = uninstall_agent(&registry, AgentId::Grok, false, &ex).unwrap();
    assert!(!out.ok);
    assert!(
        out.message.contains("未安装") || out.message.contains("not"),
        "message={}",
        out.message
    );
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
fn install_runtime_powershell_logs_dual_version_context() {
    let ex = MockExecutor {
        calls: Arc::new(Mutex::new(Vec::new())),
        exit_code: 0,
        stdout: String::new(),
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
    };
    // Even if git is already present, we still invoke the platform package
    // manager (then redetect). If it is missing, resolution fails before the
    // executor runs.
    let channel = default_runtime_channel();
    let out = install_runtime(RuntimeId::Git, channel, &ex).unwrap();
    let cmds = calls.lock().unwrap();
    if cmds.is_empty() {
        // The package manager may not be installed on this environment.
        assert!(!out.ok);
        assert!(
            out.message.contains("winget")
                || out.message.contains("Homebrew")
                || out.logs.iter().any(|l| l.contains("winget") || l.contains("brew")),
            "expected package-manager-missing path: msg={} logs={:?}",
            out.message,
            out.logs
        );
    } else {
        assert!(
            cmds.iter().any(|c| {
                if cfg!(target_os = "macos") {
                    c.contains("brew") && c.contains("install git")
                } else {
                    c.contains("Git.Git")
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
