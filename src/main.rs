#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

use std::process;

use aegis_audit::AuditTimestamp;
use aegis_types::Decision;
use aegis_types::RiskLevel;
use aegis_types::{
    EXIT_ABSENT, EXIT_BLOCKED, EXIT_DENIED, EXIT_INCONCLUSIVE, EXIT_INTERNAL, EXIT_USAGE,
};
use clap::{Args, Parser, Subcommand, ValueEnum};

mod cli_commands;
mod cli_dispatch;
mod diagnostics;
mod execution;
mod install;
mod policy_output;
mod prune;
mod rollback;
mod rollback_last;
mod shell_compat;
mod shell_flow;
mod shell_wrapper;
mod verify;

#[derive(Parser)]
#[command(
    name = "aegis",
    version,
    about = "A terminal proxy that intercepts AI agent commands"
)]
struct Cli {
    #[arg(
        help = "Command to intercept (shell wrapper mode)",
        short = 'c',
        long = "command"
    )]
    command: Option<String>,

    #[arg(help = "Shell-wrapper output format: text (default) or evaluation-only json", long, value_enum, default_value_t = CommandOutputFormat::Text)]
    output: CommandOutputFormat,

    #[arg(
        help = "Control Aegis text output detail: quiet, standard, or verbose",
        long,
        value_enum,
        default_value_t = OutputVerbosity::Standard,
        conflicts_with_all = ["quiet", "verbose"]
    )]
    verbosity: OutputVerbosity,

    #[arg(
        help = "Shorthand for `--verbosity quiet`",
        long,
        conflicts_with = "verbose"
    )]
    quiet: bool,

    #[arg(
        help = "Shorthand for `--verbosity verbose`",
        short = 'v',
        long = "verbose",
        conflicts_with = "quiet"
    )]
    verbose: bool,

    #[command(subcommand)]
    subcommand: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Read NDJSON command frames from stdin and stream results to stdout")]
    Watch,
    #[command(about = "View the audit log")]
    Audit(AuditArgs),
    #[command(about = "Enable Aegis by removing ~/.aegis/disabled")]
    On,
    #[command(about = "Disable Aegis by creating ~/.aegis/disabled")]
    Off,
    #[command(about = "Show the current toggle state and active config path")]
    Status,
    #[command(about = "Roll back a previously recorded snapshot")]
    Rollback(RollbackArgs),
    #[command(about = "Inspect recorded snapshots")]
    Snapshot(SnapshotArgs),
    #[command(about = "Manage aegis configuration")]
    Config(ConfigArgs),
    #[command(
        about = "Run as a Claude Code PreToolUse hook — rewrites Bash commands through aegis"
    )]
    Hook,
    #[command(
        about = "Install aegis hooks into Claude Code and Codex",
        alias = "install"
    )]
    InstallHooks(InstallArgs),
    #[command(
        about = "Configure Aegis as an explicit opt-in $SHELL proxy for new terminal sessions"
    )]
    SetupShell(SetupShellArgs),
    #[command(about = "Remove managed integrations and show the command to remove the binary")]
    Uninstall(UninstallArgs),
    #[command(about = "Manage the opt-in npm update notice")]
    Update(UpdateArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CommandOutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputVerbosity {
    Quiet,
    Standard,
    Verbose,
}

impl OutputVerbosity {
    fn from_cli(verbosity: Self, quiet: bool, verbose: bool) -> Self {
        if quiet {
            Self::Quiet
        } else if verbose {
            Self::Verbose
        } else {
            verbosity
        }
    }

    fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

#[derive(Args)]
struct AuditArgs {
    #[arg(help = "Show only the last N audit entries", long)]
    last: Option<usize>,

    #[arg(help = "Filter entries by risk level: safe, warn, danger, block", long, value_parser = parse_risk_level)]
    risk: Option<RiskLevel>,

    #[arg(help = "Show only entries at or after this RFC 3339 timestamp", long, value_parser = parse_audit_timestamp)]
    since: Option<AuditTimestamp>,

    #[arg(help = "Show only entries at or before this RFC 3339 timestamp", long, value_parser = parse_audit_timestamp)]
    until: Option<AuditTimestamp>,

    #[arg(
        help = "Filter to commands containing this case-sensitive substring",
        long
    )]
    command_contains: Option<String>,

    #[arg(help = "Filter entries by decision: approved, denied, auto-approved, blocked, pruned", long, value_parser = parse_decision)]
    decision: Option<Decision>,

    #[arg(help = "Output format: text (default), json, ndjson", long, value_enum, default_value_t = AuditOutputFormat::Text)]
    format: AuditOutputFormat,

    #[arg(
        help = "Show an aggregated summary instead of individual entries",
        long
    )]
    summary: bool,

    #[arg(
        help = "Verify the audit integrity chain across all audit segments",
        long
    )]
    verify_integrity: bool,
}

#[derive(Args)]
struct RollbackArgs {
    #[arg(
        help = "Snapshot ID copied from `aegis audit`",
        required_unless_present = "last",
        conflicts_with = "last"
    )]
    snapshot_id: Option<String>,

    #[arg(
        help = "Restore the Snapshots of the latest command recorded at this location",
        long
    )]
    last: bool,

    #[arg(
        help = "Attempt the restore without asking: with `--last`, skip the confirmation and restore the latest command; otherwise attempt it when the liveness check is inconclusive. It has no effect when a Snapshot is confirmed gone",
        long
    )]
    yes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AuditOutputFormat {
    Text,
    Json,
    Ndjson,
}

#[derive(Args)]
struct SnapshotArgs {
    #[command(subcommand)]
    command: SnapshotCommand,
}

#[derive(Subcommand)]
enum SnapshotCommand {
    #[command(about = "List recoverable snapshots recorded in the audit log")]
    List,
    #[command(about = "Remove snapshot artifacts outside the configured retention policy")]
    Prune(PruneArgs),
    #[command(about = "Check that each recorded snapshot's artifact still exists")]
    Verify(VerifyArgs),
}

#[derive(Args)]
struct VerifyArgs {
    #[arg(help = "Check only this snapshot ID; without it, every recorded snapshot")]
    snapshot_id: Option<String>,

    #[arg(help = "Output format: text (default) or json", long, value_enum, default_value_t = CommandOutputFormat::Text)]
    format: CommandOutputFormat,
}

#[derive(Args)]
struct PruneArgs {
    #[arg(
        help = "Actually delete artifacts. Without this flag, prune shows a dry-run preview",
        long,
        conflicts_with = "dry_run"
    )]
    yes: bool,

    #[arg(
        help = "Show what would be pruned without deleting anything",
        long,
        conflicts_with = "yes"
    )]
    dry_run: bool,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Args)]
struct InstallArgs {
    #[arg(
        help = "Patch ./.claude/settings.json instead of ~/.claude/settings.json when installing Claude Code hooks",
        long,
        conflicts_with = "codex"
    )]
    local: bool,

    #[arg(help = "Install hooks for Claude Code and Codex", long, conflicts_with_all = ["claude_code", "codex"])]
    all: bool,

    #[arg(help = "Install only Claude Code hooks", long = "claude-code", conflicts_with_all = ["all", "codex"])]
    claude_code: bool,

    #[arg(help = "Install only Codex hooks", long, conflicts_with_all = ["all", "claude_code"])]
    codex: bool,
}

#[derive(Args)]
struct SetupShellArgs {
    #[arg(
        help = "Remove the managed Aegis shell setup block instead of installing it",
        long
    )]
    remove: bool,

    #[arg(
        help = "Real shell to execute after Aegis approves a command, for example /bin/zsh",
        long
    )]
    shell: Option<std::path::PathBuf>,

    #[arg(
        help = "Shell startup file to modify, for example ~/.zshrc or ~/.bashrc",
        long = "rc-file"
    )]
    rc_file: Option<std::path::PathBuf>,

    #[arg(
        help = "Path to the Aegis binary that SHELL should point to",
        long = "aegis-bin"
    )]
    aegis_bin: Option<std::path::PathBuf>,
}

#[derive(Args)]
struct UninstallArgs {
    #[arg(
        help = "Also delete ~/.aegis, including audit logs and snapshots",
        long
    )]
    purge_data: bool,

    #[arg(
        help = "Override removal-channel detection for the printed binary-removal command",
        long,
        value_enum
    )]
    channel: Option<install::uninstall::RemovalChannel>,

    #[arg(help = "Also clean a custom shell startup file", long = "rc-file")]
    rc_file: Option<std::path::PathBuf>,
}

#[derive(Args)]
struct UpdateArgs {
    #[command(subcommand)]
    command: UpdateCommand,
}

#[derive(Subcommand)]
enum UpdateCommand {
    #[command(about = "Opt in to periodic update checks for an installation channel")]
    Enable(UpdateEnableArgs),
    #[command(about = "Opt out; the last known cache is kept but goes inert")]
    Disable,
    #[command(about = "Show consent, channel, and cache state")]
    Status,
    #[command(about = "Check the registry now and print the result")]
    Check,
}

#[derive(Args)]
struct UpdateEnableArgs {
    #[arg(help = "Installation channel to check for updates", long, value_enum, default_value_t = UpdateChannelArg::Npm)]
    channel: UpdateChannelArg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UpdateChannelArg {
    Npm,
}

impl From<UpdateChannelArg> for aegis::update::Channel {
    fn from(value: UpdateChannelArg) -> Self {
        match value {
            UpdateChannelArg::Npm => aegis::update::Channel::Npm,
        }
    }
}

#[derive(Subcommand)]
enum ConfigCommand {
    #[command(about = "Create a project-local .aegis.toml in the current directory")]
    Init,
    #[command(about = "Print the active config after applying search order and defaults")]
    Show,
    #[command(about = "Validate the active config and report errors/warnings")]
    Validate(ConfigValidateArgs),
}

#[derive(Args)]
struct ConfigValidateArgs {
    #[arg(help = "Validation output format", long, value_enum, default_value_t = ConfigValidateOutput::Text)]
    output: ConfigValidateOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ConfigValidateOutput {
    Text,
    Json,
}

fn main() {
    #[cfg(feature = "language-analysis")]
    if std::env::args().any(|a| a == aegis::analysis::INTERNAL_LANGUAGE_WORKER_FLAG) {
        process::exit(cli_dispatch::run_internal_language_worker());
    }

    #[cfg(target_os = "linux")]
    if std::env::args().any(|a| a == aegis_sandbox::INNER_LANDLOCK_FLAG) {
        process::exit(aegis_sandbox::run_inner_landlock_wrapper());
    }

    if std::env::args().any(|a| a == aegis::update::INTERNAL_UPDATE_CHECK_FLAG) {
        process::exit(aegis::update::run_internal_update_check_from_env());
    }

    let invocation = match shell_compat::parse_invocation_mode() {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("error: {message}");
            process::exit(EXIT_USAGE);
        }
    };

    diagnostics::init(&invocation);

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("error: failed to build tokio runtime: {err}");
            process::exit(EXIT_INTERNAL);
        }
    };
    let handle = rt.handle().clone();

    let exit_code = match invocation {
        shell_compat::InvocationMode::Cli(cli) => cli_dispatch::run_cli(cli, &rt, handle),
        shell_compat::InvocationMode::ShellCompatCommand { command, launch } => {
            shell_wrapper::run_shell_wrapper(
                &command,
                CommandOutputFormat::Text,
                OutputVerbosity::Standard,
                handle,
                &launch,
            )
        }
        shell_compat::InvocationMode::ShellCompatSession { launch } => {
            shell_compat::exec_shell_session(&launch)
        }
    };

    process::exit(exit_code);
}

fn parse_risk_level(value: &str) -> Result<RiskLevel, String> {
    value.parse()
}

fn parse_audit_timestamp(value: &str) -> Result<AuditTimestamp, String> {
    AuditTimestamp::parse_rfc3339(value)
}

fn parse_decision(value: &str) -> Result<Decision, String> {
    value.parse()
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

#[cfg(test)]
mod test_git;

#[cfg(test)]
#[path = "setup_shell_parse_tests.rs"]
mod setup_shell_parse_tests;
