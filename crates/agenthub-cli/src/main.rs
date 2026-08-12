//! agenthub CLI — thin shell over agenthub-core.

mod commands;
mod output;

use std::path::PathBuf;
use std::process::ExitCode;

use agenthub_core::error::AppError;
use agenthub_core::AgentHub;
use clap::{Parser, Subcommand};

use commands::{
    account, agent, backup, config, doctor, env as env_cmd, provider, run as run_cmd, skill, usage,
};
use output::{print_error, OutputFormat};

#[derive(Debug, Parser)]
#[command(name = "agenthub", version, about = "Multi-agent manager (CLI)")]
struct Cli {
    /// Override data directory (else AGENTHUB_HOME or ~/.agenthub)
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Default agent id filter where applicable
    #[arg(short = 'a', long = "agent", global = true)]
    agent: Option<String>,

    /// Output format
    #[arg(short = 'o', long = "output", global = true, default_value = "table")]
    output: OutputFormat,

    /// Skip confirmations
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,

    /// Verbose diagnostics on stderr
    #[arg(short = 'v', long = "verbose", global = true)]
    verbose: bool,

    /// Quiet (same as -o quiet)
    #[arg(short = 'q', long = "quiet", global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Health overview: runtimes, agents, paths, db
    Doctor,
    /// Shared runtime environment
    Env {
        #[command(subcommand)]
        action: EnvCommands,
    },
    /// Agent install state
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },
    /// AgentHub own settings / paths
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
    /// Run a prompt on one or more agents (parallel by default)
    Run {
        /// Prompt text (non-interactive headless mode per agent)
        prompt: String,

        /// Comma-separated agent ids (e.g. claude,codex,pi)
        #[arg(long)]
        agents: Option<String>,

        /// Target all registered agents (missing ones are skipped)
        #[arg(long)]
        all: bool,

        /// parallel | sequential
        #[arg(long, default_value = "parallel")]
        mode: String,

        /// Per-agent timeout in seconds
        #[arg(long, default_value_t = agenthub_core::catalog::limits::DEFAULT_RUN_TIMEOUT_SECS)]
        timeout: u64,

        /// Working directory for agent processes
        #[arg(long)]
        cwd: Option<PathBuf>,

        /// Print commands only; do not spawn agents
        #[arg(long)]
        dry_run: bool,

        /// Opt-in dangerous auto-approve / sandbox bypass flags per agent
        #[arg(long)]
        allow_dangerous: bool,
    },
    /// Provider pool and built-in presets
    Provider {
        #[command(subcommand)]
        action: ProviderCommands,
    },
    /// Live configuration backups
    Backup {
        #[command(subcommand)]
        action: BackupCommands,
    },
    /// Shared skill source and per-agent projections
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },
    /// Account pool and live credential switching
    Account {
        #[command(subcommand)]
        action: AccountCommands,
    },
    /// Token usage from local agent session logs (not an official model store)
    Usage {
        #[command(subcommand)]
        action: UsageCommands,
    },
}

#[derive(Debug, Subcommand)]
enum UsageCommands {
    /// Incrementally collect usage from agent session logs into the local DB
    Collect,
    /// Aggregate token / cost stats from collected rows
    Stats {
        /// Look-back window in days (default 7)
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Optional model name filter
        #[arg(long)]
        model: Option<String>,
    },
    /// List distinct model names from usage_records (dedup only)
    Models,
    /// Parser health per agent
    Health,
}

#[derive(Debug, Subcommand)]
enum AccountCommands {
    /// List accounts (secrets redacted)
    List,
    /// Import --agent's current live file credentials
    Import {
        /// Optional display label
        #[arg(long)]
        name: Option<String>,
    },
    /// Add an API key account (`--key -` reads stdin)
    AddApikey {
        /// Display label
        #[arg(long)]
        label: Option<String>,
        /// API key value, or `-` to read from stdin
        #[arg(long)]
        key: String,
    },
    /// Switch live credentials to a saved account
    Switch {
        /// Account id or exact label
        id_or_label: String,
    },
    /// Delete an account from the pool
    Delete {
        /// Account id or exact label
        id_or_label: String,
    },
    /// Print OAuth authorize URL (PKCE) for --agent without completing the flow
    OauthUrl,
    /// Refresh OAuth tokens for a saved account (uses refresh_token)
    Refresh {
        /// Account id or exact label
        id_or_label: String,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    /// List shared skills and their per-agent projection states
    List,
    /// List skills from all skill roots (shared + agent-private)
    ListInstalled,
    /// Sync every shared skill to --agent or --all (copy mode)
    Sync {
        #[arg(long)]
        all: bool,
        /// Replace conflicting projections
        #[arg(long)]
        force: bool,
    },
    /// Enable one shared skill for --agent (copy)
    Enable {
        skill: String,
        /// Replace a conflicting projection
        #[arg(long)]
        force: bool,
    },
    /// Remove one projection for --agent, preserving the shared source
    Disable { skill: String },
    /// Install a skill into the shared source root (local path / zip / git URL)
    Install {
        /// Local path, .zip, or git URL
        source: String,
        /// Replace an existing same-id skill
        #[arg(long)]
        overwrite: bool,
    },
    /// Copy an agent-private skill into the shared source (keeps the private copy)
    ImportPrivate {
        skill: String,
        /// Replace an existing same-id skill in the shared source
        #[arg(long)]
        overwrite: bool,
    },
    /// Uninstall a shared source skill (removes projections + source)
    Uninstall {
        skill: String,
        /// Also remove an agent-private skill: --agent required for private-only
        #[arg(long)]
        private: bool,
    },
    /// Update a shared skill from its recorded source
    Update { skill: String },
    /// Project a shared skill onto --agent as link or copy
    Project {
        skill: String,
        /// link | copy (default: link)
        #[arg(long, default_value = "link")]
        mode: String,
    },
    /// Search skill market (skills.sh / skillhub.cn per settings; empty query = leaderboard)
    Market {
        #[arg(long, default_value = "")]
        query: String,
    },
}

#[derive(Debug, Subcommand)]
enum BackupCommands {
    /// List indexed backups
    List,
    /// Create a manual backup for --agent
    Create {
        /// Optional description stored with the backup
        #[arg(long)]
        note: Option<String>,
    },
    /// Restore a backup, first snapshotting current live files
    Restore { backup_id: String },
    /// Permanently delete a backup snapshot and its index row
    Delete { backup_id: String },
}

#[derive(Debug, Subcommand)]
enum ProviderCommands {
    /// List persisted L1 providers (marks is_current)
    List,
    /// Show one provider by id or unambiguous name (secrets redacted)
    Show {
        /// Provider id or exact name
        id_or_name: String,
    },
    /// List built-in L3 provider presets (read-only)
    Presets,
    /// Import --agent's current live config as a provider
    ImportLive {
        /// Optional display name for the imported provider
        #[arg(long)]
        name: Option<String>,
    },
    /// Safely apply a saved provider to --agent
    Switch {
        /// Provider id or exact name
        id_or_name: String,
    },
}

#[derive(Debug, Subcommand)]
enum EnvCommands {
    /// List runtime detection results
    List,
    /// Install a shared runtime (Homebrew on macOS, winget on Windows)
    Install {
        /// Runtime id: nodejs | npm | powershell | git
        runtime: String,
        /// Install channel (default: platform-native package manager)
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// List detected agents
    List,
    /// Print the static capability matrix
    Capabilities {
        /// Emit a markdown table (overrides -o)
        #[arg(long)]
        markdown: bool,
    },
    /// Install an agent (ensure env first; optional --install-deps)
    Install {
        agent: String,
        #[arg(long)]
        channel: Option<String>,
        /// Auto-install missing runtimes (e.g. Node via winget) before agent install
        #[arg(long)]
        install_deps: bool,
    },
    /// Upgrade an installed agent
    Upgrade { agent: String },
    /// Check whether installed agents have newer versions (npm dist-tags)
    Outdated {
        /// Optional agent id (default: all)
        agent: Option<String>,
        /// Bypass disk cache and re-query registry
        #[arg(long)]
        force: bool,
    },
    /// Uninstall agent (npm channel) or purge config
    Uninstall {
        agent: String,
        /// Also delete agent config directory (requires -y)
        #[arg(long)]
        purge_config: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    /// Print data_dir / db / backups paths
    Path,
    /// Get a settings key (or all)
    Get { key: Option<String> },
    /// Set a whitelisted settings key
    Set { key: String, value: String },
}

fn main() -> ExitCode {
    let mut cli = Cli::parse();
    if cli.quiet {
        cli.output = OutputFormat::Quiet;
    }

    if let Err(e) = agenthub_core::logging::init_for_app(
        cli.data_dir.as_deref(),
        "cli",
        cli.verbose,
        env!("CARGO_PKG_VERSION"),
    ) {
        // Fall back so CLI still runs if logging init fails.
        eprintln!("warning: logging init failed: {e}");
        init_tracing(cli.verbose);
    }

    if cli.verbose {
        agenthub_core::logging::log_debug(
            agenthub_core::logging::targets::CLI,
            "start",
            &format!("verbose=true data_dir_override={:?}", cli.data_dir),
        );
    }

    let hub = match AgentHub::open(cli.data_dir.as_deref()) {
        Ok(h) => h,
        Err(e) => {
            agenthub_core::logging::log_app_error(agenthub_core::logging::targets::CLI, "open", &e);
            print_error(&e, cli.output);
            return map_exit(&e);
        }
    };

    let result = match cli.command {
        Commands::Doctor => doctor::run(&hub, cli.output),
        Commands::Env { action } => match action {
            EnvCommands::List => env_cmd::list(&hub, cli.output),
            EnvCommands::Install { runtime, channel } => {
                env_cmd::install(&hub, &runtime, channel.as_deref().unwrap_or(""), cli.output)
            }
        },
        Commands::Agent { action } => match action {
            AgentCommands::List => agent::list(&hub, cli.output, cli.agent.as_deref()),
            AgentCommands::Capabilities { markdown } => {
                agent::capabilities(&hub, cli.output, cli.agent.as_deref(), markdown)
            }
            AgentCommands::Install {
                agent: agent_id,
                channel,
                install_deps,
            } => agent::install(
                &hub,
                &agent_id,
                channel.as_deref(),
                install_deps,
                cli.output,
            ),
            AgentCommands::Upgrade { agent: agent_id } => {
                agent::upgrade(&hub, &agent_id, cli.output)
            }
            AgentCommands::Outdated { agent, force } => {
                agent::outdated(&hub, agent.as_deref(), force, cli.output)
            }
            AgentCommands::Uninstall {
                agent: agent_id,
                purge_config,
            } => agent::uninstall(&hub, &agent_id, purge_config, cli.yes, cli.output),
        },
        Commands::Config { action } => match action {
            ConfigCommands::Path => config::path(&hub, cli.output),
            ConfigCommands::Get { key } => config::get(&hub, key.as_deref(), cli.output),
            ConfigCommands::Set { key, value } => config::set(&hub, &key, &value, cli.output),
        },
        Commands::Run {
            prompt,
            agents,
            all,
            mode,
            timeout,
            cwd,
            dry_run,
            allow_dangerous,
        } => run_cmd::run(
            &hub,
            run_cmd::RunArgs {
                prompt,
                agents,
                all,
                global_agent: cli.agent.clone(),
                mode,
                timeout_secs: timeout,
                cwd,
                dry_run,
                allow_dangerous,
            },
            cli.output,
        ),
        Commands::Provider { action } => match action {
            ProviderCommands::List => provider::list(&hub, cli.output, cli.agent.as_deref()),
            ProviderCommands::Show { id_or_name } => {
                provider::show(&hub, &id_or_name, cli.output, cli.agent.as_deref())
            }
            ProviderCommands::Presets => provider::presets(cli.output, cli.agent.as_deref()),
            ProviderCommands::ImportLive { name } => provider::import_live(
                &hub,
                name.as_deref(),
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            ProviderCommands::Switch { id_or_name } => {
                provider::switch(&hub, &id_or_name, cli.output, cli.agent.as_deref(), cli.yes)
            }
        },
        Commands::Backup { action } => match action {
            BackupCommands::List => backup::list(&hub, cli.output, cli.agent.as_deref()),
            BackupCommands::Create { note } => {
                backup::create(&hub, cli.output, cli.agent.as_deref(), note.as_deref())
            }
            BackupCommands::Restore { backup_id } => {
                backup::restore(&hub, &backup_id, cli.output, cli.yes)
            }
            BackupCommands::Delete { backup_id } => {
                backup::delete(&hub, &backup_id, cli.output, cli.yes)
            }
        },
        Commands::Skill { action } => match action {
            SkillCommands::List => skill::list(&hub, cli.output, cli.agent.as_deref()),
            SkillCommands::ListInstalled => skill::list_installed(&hub, cli.output),
            SkillCommands::Sync { all, force } => {
                skill::sync(&hub, cli.output, cli.agent.as_deref(), all, force, cli.yes)
            }
            SkillCommands::Enable {
                skill: skill_id,
                force,
            } => skill::enable(
                &hub,
                &skill_id,
                cli.output,
                cli.agent.as_deref(),
                force,
                cli.yes,
            ),
            SkillCommands::Disable { skill: skill_id } => {
                skill::disable(&hub, &skill_id, cli.output, cli.agent.as_deref(), cli.yes)
            }
            SkillCommands::Install { source, overwrite } => {
                skill::install(&hub, &source, overwrite, cli.output)
            }
            SkillCommands::ImportPrivate {
                skill: skill_id,
                overwrite,
            } => skill::import_private(
                &hub,
                &skill_id,
                overwrite,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            SkillCommands::Uninstall {
                skill: skill_id,
                private,
            } => skill::uninstall(
                &hub,
                &skill_id,
                private,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            SkillCommands::Update { skill: skill_id } => skill::update(&hub, &skill_id, cli.output),
            SkillCommands::Project {
                skill: skill_id,
                mode,
            } => skill::project(&hub, &skill_id, &mode, cli.output, cli.agent.as_deref()),
            SkillCommands::Market { query } => skill::market(&hub, &query, cli.output),
        },
        Commands::Account { action } => match action {
            AccountCommands::List => account::list(&hub, cli.output, cli.agent.as_deref()),
            AccountCommands::Import { name } => account::import(
                &hub,
                name.as_deref(),
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            AccountCommands::AddApikey { label, key } => account::add_apikey(
                &hub,
                label.as_deref(),
                &key,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            AccountCommands::Switch { id_or_label } => account::switch(
                &hub,
                &id_or_label,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            AccountCommands::Delete { id_or_label } => account::delete(
                &hub,
                &id_or_label,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
            AccountCommands::OauthUrl => account::oauth_url(&hub, cli.output, cli.agent.as_deref()),
            AccountCommands::Refresh { id_or_label } => account::refresh(
                &hub,
                &id_or_label,
                cli.output,
                cli.agent.as_deref(),
                cli.yes,
            ),
        },
        Commands::Usage { action } => match action {
            UsageCommands::Collect => usage::collect(&hub, cli.output, cli.agent.as_deref()),
            UsageCommands::Stats { days, model } => usage::parse_days(days).and_then(|days| {
                usage::stats(
                    &hub,
                    days,
                    cli.output,
                    cli.agent.as_deref(),
                    model.as_deref(),
                )
            }),
            UsageCommands::Models => usage::models(&hub, cli.output, cli.agent.as_deref()),
            UsageCommands::Health => usage::health(&hub, cli.output),
        },
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Business modules own ERROR; CLI records a greppable breadcrumb only.
            agenthub_core::logging::log_debug(
                agenthub_core::logging::targets::CLI,
                "command",
                &format!("command error code={}", e.code()),
            );
            print_error(&e, cli.output);
            map_exit(&e)
        }
    }
}

fn init_tracing(verbose: bool) {
    let filter = if verbose { "debug" } else { "warn" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn map_exit(err: &AppError) -> ExitCode {
    match err {
        AppError::InvalidArg(_) => ExitCode::from(2),
        AppError::EnvNotReady(_) | AppError::Unsupported(_) | AppError::NotFound(_) => {
            ExitCode::from(3)
        }
        AppError::Message { code, .. } if *code == "usage" => ExitCode::from(2),
        AppError::Message { code, .. } if *code == "cancelled" => ExitCode::from(4),
        AppError::Message { code, .. } if *code == "confirmation_required" => ExitCode::from(4),
        AppError::Message { code, .. } if *code == "partial" => ExitCode::from(5),
        AppError::Message { code, .. } if *code == "run.failed" => ExitCode::from(3),
        _ => ExitCode::from(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert!(cli.yes);
            assert!(matches!(cli.agent.as_deref(), Some("claude" | "codex")));
            assert!(matches!(cli.command, Commands::Provider { .. }));
        }
    }
}
