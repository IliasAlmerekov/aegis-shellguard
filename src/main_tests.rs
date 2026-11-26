#[path = "../tests/support/isolated_workspace.rs"]
mod isolated_workspace;

use std::fs;
use std::path::Path;

use aegis::planning::{CwdState, PlanningOutcome, PreparedPlanner};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use tempfile::TempDir;
use tokio::runtime::Handle;

use super::*;
use crate::cli_commands::config_load_error_lines;
use crate::shell_compat::{InvocationMode, ShellLaunchOptions, parse_shell_compat_invocation};
use crate::test_git::git_init;
use aegis::error::AegisError;
use aegis_types::{AllowlistOverrideLevel, Mode, SnapshotPolicy};

#[test]
fn shell_compat_parser_handles_dash_lc_command() {
    let args = vec![
        std::ffi::OsString::from("-lc"),
        std::ffi::OsString::from("printf compat"),
    ];

    let parsed = parse_shell_compat_invocation(&args).unwrap();
    let Some(InvocationMode::ShellCompatCommand { command, launch }) = parsed else {
        panic!("expected shell compatibility command invocation");
    };

    assert_eq!(command, "printf compat");
    assert!(launch.login);
    assert!(!launch.interactive);
    assert!(launch.positional_args.is_empty());
}

#[test]
fn shell_compat_parser_handles_command_flag_before_interactive_flag() {
    let args = vec![
        std::ffi::OsString::from("-ci"),
        std::ffi::OsString::from("printf compat"),
    ];

    let parsed = parse_shell_compat_invocation(&args).unwrap();
    let Some(InvocationMode::ShellCompatCommand { command, launch }) = parsed else {
        panic!("expected shell compatibility command invocation");
    };

    assert_eq!(command, "printf compat");
    assert!(!launch.login);
    assert!(launch.interactive);
    assert!(launch.positional_args.is_empty());
}

#[test]
fn shell_compat_parser_handles_repeated_command_flag_bundle() {
    let args = vec![
        std::ffi::OsString::from("-cc"),
        std::ffi::OsString::from("printf compat"),
    ];

    let parsed = parse_shell_compat_invocation(&args).unwrap();
    let Some(InvocationMode::ShellCompatCommand { command, launch }) = parsed else {
        panic!("expected shell compatibility command invocation");
    };

    assert_eq!(command, "printf compat");
    assert!(!launch.login);
    assert!(!launch.interactive);
    assert!(launch.positional_args.is_empty());
}

#[test]
fn shell_compat_parser_handles_separate_login_and_command_flags() {
    let args = vec![
        std::ffi::OsString::from("-l"),
        std::ffi::OsString::from("-c"),
        std::ffi::OsString::from("printf compat"),
    ];

    let parsed = parse_shell_compat_invocation(&args).unwrap();
    let Some(InvocationMode::ShellCompatCommand { command, launch }) = parsed else {
        panic!("expected shell compatibility command invocation");
    };

    assert_eq!(command, "printf compat");
    assert!(launch.login);
    assert!(!launch.interactive);
    assert!(launch.positional_args.is_empty());
}

#[test]
fn shell_compat_parser_handles_long_login_flag() {
    let args = vec![
        std::ffi::OsString::from("--login"),
        std::ffi::OsString::from("-c"),
        std::ffi::OsString::from("printf compat"),
    ];

    let parsed = parse_shell_compat_invocation(&args).unwrap();
    let Some(InvocationMode::ShellCompatCommand { command, launch }) = parsed else {
        panic!("expected shell compatibility command invocation");
    };

    assert_eq!(command, "printf compat");
    assert!(launch.login);
    assert!(!launch.interactive);
    assert!(launch.positional_args.is_empty());
}

#[test]
fn shell_compat_parser_does_not_capture_native_aegis_command_mode() {
    let args = vec![
        std::ffi::OsString::from("-c"),
        std::ffi::OsString::from("printf compat"),
        std::ffi::OsString::from("--output"),
        std::ffi::OsString::from("json"),
    ];

    assert!(parse_shell_compat_invocation(&args).unwrap().is_none());
}

#[test]
fn shell_launch_options_drop_login_flag_for_posix_sh() {
    let launch = ShellLaunchOptions {
        login: true,
        interactive: false,
        positional_args: Vec::new(),
    };

    assert_eq!(launch.command_flag(Path::new("/bin/sh")), "-c");
    assert_eq!(launch.command_flag(Path::new("/bin/bash")), "-lc");
    assert_eq!(
        launch.session_flags(Path::new("/bin/sh")),
        Vec::<&str>::new()
    );
    assert_eq!(launch.session_flags(Path::new("/bin/zsh")), vec!["-l"]);
}

#[test]
fn config_load_error_lines_include_fix_hint_only_for_config_errors() {
    let lines = config_load_error_lines(&AegisError::Config(
        aegis_config::error::ConfigError::Config("bad config".to_string()),
    ));
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("failed to load config"));
    assert!(lines[1].contains("Fix or remove the invalid config file"));
}

#[test]
fn config_load_error_lines_omit_fix_hint_for_non_config_errors() {
    let lines = config_load_error_lines(&AegisError::Io(std::io::Error::other("disk")));
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("failed to load config"));
}

#[tokio::test]
async fn watch_mode_safe_command_emits_result_frame() {
    use aegis::watch::{InputFrame, MAX_FRAME_BYTES, ReadLineResult, read_bounded_line};
    use tokio::io::BufReader;

    let input = b"{\"cmd\":\"echo hello\",\"id\":\"t1\"}\n";
    let mut reader = BufReader::new(input.as_ref());

    let result = read_bounded_line(&mut reader, MAX_FRAME_BYTES)
        .await
        .unwrap();
    let line = match result {
        ReadLineResult::Line(l) => l,
        _ => panic!("expected Line"),
    };

    let frame: InputFrame = serde_json::from_str(&line).unwrap();
    assert_eq!(frame.cmd, "echo hello");
    assert_eq!(frame.id.as_deref(), Some("t1"));
}

#[test]
fn parse_risk_level_accepts_case_insensitive_values() {
    assert_eq!(parse_risk_level("WARN"), Ok(RiskLevel::Warn));
}

#[test]
fn parse_risk_level_rejects_unknown_values() {
    let error = parse_risk_level("critical").unwrap_err();
    assert!(error.contains("invalid risk level 'critical'"));
}

#[test]
fn cli_rejects_quiet_and_verbose_together() {
    let error = Cli::try_parse_from(["aegis", "--quiet", "--verbose", "-c", "echo hello"])
        .err()
        .expect("quiet and verbose must conflict");
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn cli_rejects_verbosity_with_quiet() {
    let error = Cli::try_parse_from([
        "aegis",
        "--verbosity",
        "verbose",
        "--quiet",
        "-c",
        "echo hello",
    ])
    .err()
    .expect("verbosity and quiet must conflict");
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn cli_rejects_verbosity_with_verbose() {
    let error = Cli::try_parse_from([
        "aegis",
        "--verbosity",
        "quiet",
        "--verbose",
        "-c",
        "echo hello",
    ])
    .err()
    .expect("verbosity and verbose must conflict");
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn cli_parses_install_hooks_subcommand_with_all_flag() {
    let cli = Cli::try_parse_from(["aegis", "install-hooks", "--all"]).unwrap();
    let Some(Commands::InstallHooks(args)) = cli.subcommand else {
        panic!("expected install-hooks subcommand");
    };

    assert!(args.all);
    assert!(!args.local);
    assert!(!args.claude_code);
    assert!(!args.codex);
}

#[test]
fn cli_parses_install_hooks_subcommand_with_claude_code_flag() {
    let cli = Cli::try_parse_from(["aegis", "install-hooks", "--claude-code"]).unwrap();
    let Some(Commands::InstallHooks(args)) = cli.subcommand else {
        panic!("expected install-hooks subcommand");
    };

    assert!(!args.all);
    assert!(!args.local);
    assert!(args.claude_code);
    assert!(!args.codex);
}

#[test]
fn cli_parses_install_hooks_subcommand_with_codex_flag() {
    let cli = Cli::try_parse_from(["aegis", "install-hooks", "--codex"]).unwrap();
    let Some(Commands::InstallHooks(args)) = cli.subcommand else {
        panic!("expected install-hooks subcommand");
    };

    assert!(!args.all);
    assert!(!args.local);
    assert!(!args.claude_code);
    assert!(args.codex);
}

#[test]
fn cli_rejects_install_hooks_codex_with_local_flag() {
    let error = Cli::try_parse_from(["aegis", "install-hooks", "--codex", "--local"])
        .err()
        .expect("codex and local must conflict");

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn cli_parses_hook_subcommand() {
    let cli = Cli::try_parse_from(["aegis", "hook"]).unwrap();
    assert!(matches!(cli.subcommand, Some(Commands::Hook)));
}

#[test]
fn cli_parses_install_subcommand_with_local_flag() {
    let cli = Cli::try_parse_from(["aegis", "install", "--local"]).unwrap();
    let Some(Commands::InstallHooks(args)) = cli.subcommand else {
        panic!("expected install subcommand");
    };

    assert!(args.local);
    assert!(!args.all);
    assert!(!args.claude_code);
    assert!(!args.codex);
}

fn test_handle() -> Handle {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();
    std::mem::forget(rt);
    handle
}

fn config_from_toml(src: &str) -> AegisConfig {
    toml::from_str(src).unwrap()
}

#[test]
fn unavailable_cwd_shell_plan_requires_recovery_without_plugins() {
    isolated_workspace::run(git_init, |_| {
        let config = config_from_toml(
            r#"
snapshot_policy = "Selective"
auto_snapshot_git = true
auto_snapshot_docker = false
"#,
        );
        let context = RuntimeContext::new(config, test_handle()).unwrap();
        let prepared = PreparedPlanner::Ready(Box::new(context));
        let outcome = prepared.plan(aegis::planning::PlanningRequest {
            command: "terraform destroy -target=module.prod.api",
            cwd_state: CwdState::Unavailable,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        });

        let PlanningOutcome::Planned(plan) = outcome else {
            panic!("expected planned outcome");
        };

        assert_eq!(
            plan.snapshot_plan(),
            aegis::planning::SnapshotPlan::Required {
                applicable_plugins: vec![],
                git_targets: aegis::planning::GitTargetPlan::default(),
            }
        );
    });
}

#[test]
fn project_audit_only_attack_config_is_ratchet_back_to_protective_defaults() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(".aegis.toml"),
        r#"
mode = "Audit"
allowlist_override_level = "Danger"
snapshot_policy = "None"
"#,
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(config.mode, Mode::Protect);
    assert_eq!(
        config.allowlist_override_level,
        AllowlistOverrideLevel::Warn
    );
    assert_eq!(config.snapshot_policy, SnapshotPolicy::Selective);
}

#[test]
fn scanner_skips_every_global_cli_option_before_a_subcommand() {
    use clap::CommandFactory;

    let command = Cli::command();
    for argument in command.get_arguments() {
        let id = argument.get_id().as_str();
        if matches!(id, "help" | "version") || argument.is_positional() {
            continue;
        }
        let takes_value = argument.get_action().takes_values();
        let mut spellings = Vec::new();
        if let Some(long) = argument.get_long() {
            spellings.push(format!("--{long}"));
        }
        if let Some(short) = argument.get_short() {
            spellings.push(format!("-{short}"));
        }
        assert!(!spellings.is_empty(), "{id} has no option spelling");
        for spelling in spellings {
            let mut tokens = vec!["aegis", spelling.as_str()];
            if takes_value {
                tokens.push("value");
            }
            tokens.push("off");
            assert_eq!(
                aegis_parser::aegis_option_subcommand_start(&tokens),
                Some(tokens.len() - 1),
                "the scanner must skip global option {spelling}"
            );
        }
    }
}
