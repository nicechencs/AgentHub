use super::*;
use clap::CommandFactory;

fn collect_paths(cmd: &clap::Command, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sub in cmd.get_subcommands() {
        let name = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        if sub.get_subcommands().next().is_some() {
            out.extend(collect_paths(sub, &name));
        } else {
            out.push(name);
        }
    }
    out
}

#[test]
fn command_tree_covers_freeze_and_documented_extensions() {
    let mut paths = collect_paths(&Cli::command(), "");
    paths.sort();

    let expected = [
        "account add-apikey",
        "account delete",
        "account import",
        "account list",
        "account oauth-url",
        "account refresh",
        "account switch",
        "account undo",
        "agent capabilities",
        "agent install",
        "agent list",
        "agent outdated",
        "agent uninstall",
        "agent upgrade",
        "backup create",
        "backup delete",
        "backup list",
        "backup restore",
        "config get",
        "config path",
        "config set",
        "doctor",
        "env install",
        "env list",
        "provider import-live",
        "provider list",
        "provider presets",
        "provider show",
        "provider switch",
        "provider test-latency",
        "provider undo",
        "run",
        "skill disable",
        "skill enable",
        "skill import-private",
        "skill install",
        "skill list",
        "skill list-installed",
        "skill market",
        "skill project",
        "skill sync",
        "skill uninstall",
        "skill update",
        "usage collect",
        "usage health",
        "usage models",
        "usage stats",
    ];
    for cmd in expected {
        assert!(
            paths.iter().any(|p| p == cmd),
            "missing CLI command `{cmd}` in {paths:?}"
        );
    }
    assert!(
        !paths.iter().any(|p| p == "env uninstall"),
        "env uninstall must not exist"
    );
}

#[test]
fn unknown_command_is_usage_error() {
    assert!(Cli::try_parse_from(["agenthub", "not-a-command"]).is_err());
    assert!(Cli::try_parse_from(["agenthub", "env", "uninstall", "nodejs"]).is_err());
}

#[test]
fn provider_write_commands_parse_global_agent_and_yes() {
    for args in [
        vec![
            "agenthub",
            "provider",
            "import-live",
            "--agent",
            "claude",
            "-y",
        ],
        vec![
            "agenthub", "provider", "switch", "target", "--agent", "codex", "--yes",
        ],
        vec!["agenthub", "provider", "undo", "--agent", "claude", "-y"],
        vec![
            "agenthub",
            "provider",
            "test-latency",
            "relay",
            "--agent",
            "claude",
        ],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.agent.as_deref(), Some("claude" | "codex")));
        assert!(matches!(cli.command, Commands::Provider { .. }));
    }
}

#[test]
fn account_undo_and_backup_delete_parse() {
    let undo = Cli::try_parse_from(["agenthub", "account", "undo", "-a", "grok", "-y"]).unwrap();
    assert!(undo.yes);
    assert_eq!(undo.agent.as_deref(), Some("grok"));
    assert!(matches!(undo.command, Commands::Account { .. }));

    let delete = Cli::try_parse_from(["agenthub", "backup", "delete", "backup-1", "-y"]).unwrap();
    assert!(matches!(delete.command, Commands::Backup { .. }));
}

#[test]
fn map_exit_covers_stable_contract() {
    assert_eq!(
        map_exit(&AppError::InvalidArg("x".into())),
        ExitCode::from(2)
    );
    assert_eq!(
        map_exit(&AppError::EnvNotReady(r#"{"code":"env.not_ready"}"#.into())),
        ExitCode::from(3)
    );
    assert_eq!(
        map_exit(&AppError::Unsupported("no".into())),
        ExitCode::from(3)
    );
    assert_eq!(
        map_exit(&AppError::NotFound("gone".into())),
        ExitCode::from(3)
    );
    assert_eq!(
        map_exit(&AppError::message("confirmation_required", "need -y")),
        ExitCode::from(4)
    );
    assert_eq!(
        map_exit(&AppError::message("cancelled", "no")),
        ExitCode::from(4)
    );
    assert_eq!(
        map_exit(&AppError::message("partial", "some failed")),
        ExitCode::from(5)
    );
    assert_eq!(
        map_exit(&AppError::message("run.failed", "timeout")),
        ExitCode::from(3)
    );
    assert_eq!(
        map_exit(&AppError::message("install.failed", "npm")),
        ExitCode::from(1)
    );
}
