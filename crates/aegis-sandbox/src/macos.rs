use std::ffi::{OsStr, OsString};

use crate::support::is_forced_sandbox_unavailable;
use crate::{PreparedSandboxCommand, SandboxConfig, SandboxError};

pub(crate) fn sandbox_available_for(config: &SandboxConfig) -> bool {
    !is_forced_sandbox_unavailable()
        && is_sandbox_exec_available()
        && build_seatbelt_profile(config)
            .map(|profile| exec_true_in_profile(&profile))
            .unwrap_or(false)
}

pub(crate) fn prepare_for_exec(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    prepare(config, program, args)
}

pub(crate) fn prepare_for_spawn(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    prepare(config, program, args)
}

fn prepare(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    let profile = build_seatbelt_profile(config)?;
    if is_forced_sandbox_unavailable() || !is_sandbox_exec_available() {
        if !is_forced_sandbox_unavailable() && seatbelt_unavailable_is_nested_refusal() {
            return Err(SandboxError::RequiredNestedUnderOuterSandbox);
        }
        return Err(SandboxError::Required);
    }
    if !exec_true_in_profile(&profile) {
        return Err(SandboxError::SetupFailed(
            "sandbox-exec rejected the configured profile".to_string(),
        ));
    }
    let path = std::path::Path::new(program);
    if path.components().count() > 1 && !path.exists() {
        return Err(SandboxError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("program not found: {}", path.display()),
        )));
    }
    let mut cmd = std::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(&profile).arg(program).args(args);
    Ok(PreparedSandboxCommand::active(cmd))
}

pub(crate) fn build_seatbelt_profile(config: &SandboxConfig) -> Result<String, SandboxError> {
    let mut profile = String::from("(version 1)\n");
    profile.push_str("(deny default)\n");
    profile.push_str("(allow file-read*)\n");
    profile.push_str("(allow process*)\n");
    profile.push_str("(allow sysctl-read)\n");
    profile.push_str("(allow file-write-data (literal \"/dev/null\"))\n");
    if config.allow_network {
        profile.push_str("(allow network*)\n");
    } else {
        profile.push_str("(deny network*)\n");
    }
    for canonical in crate::support::resolve_allow_write(config).paths {
        let escaped = escape_sbpl_path(&canonical)?;
        profile.push_str(&format!("(allow file-write* (subpath \"{escaped}\"))\n"));
    }
    Ok(profile)
}

fn escape_sbpl_path(path: &std::path::Path) -> Result<String, SandboxError> {
    let s = path.to_str().ok_or_else(|| {
        SandboxError::Execution(format!(
            "allow_write path contains non-UTF-8 bytes: {}",
            path.display()
        ))
    })?;
    Ok(s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(crate) fn is_sandbox_exec_available() -> bool {
    if !std::path::Path::new("/usr/bin/sandbox-exec").exists() {
        return false;
    }
    probe_seatbelt_works()
}

fn exec_true_in_profile(profile: &str) -> bool {
    std::process::Command::new("/usr/bin/sandbox-exec")
        .args(["-p", profile, "/usr/bin/true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

const MINIMAL_SEATBELT_PROBE_PROFILE: &str =
    "(version 1)\n(deny default)\n(allow process*)\n(allow file-read*)\n";

fn probe_seatbelt_works() -> bool {
    exec_true_in_profile(MINIMAL_SEATBELT_PROBE_PROFILE)
}

fn seatbelt_unavailable_is_nested_refusal() -> bool {
    let Ok(output) = std::process::Command::new("/usr/bin/sandbox-exec")
        .args(["-p", MINIMAL_SEATBELT_PROBE_PROFILE, "/usr/bin/true"])
        .output()
    else {
        return false;
    };
    if output.status.success() {
        return false;
    }
    stderr_indicates_nested_refusal(&String::from_utf8_lossy(&output.stderr))
}

fn stderr_indicates_nested_refusal(stderr: &str) -> bool {
    stderr.contains("sandbox_apply") && stderr.contains("Operation not permitted")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::support::set_force_sandbox_unavailable;
    use crate::support::test_helpers::ForceUnavailableGuard;
    use crate::{SandboxConfig, SandboxError, SandboxStatus, sandbox_available_for};

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_contains_deny_default() {
        let cfg = SandboxConfig::default();
        let profile =
            super::build_seatbelt_profile(&cfg).expect("default config must build cleanly");
        assert!(
            profile.contains("(deny default)"),
            "seatbelt profile must contain '(deny default)', got: {profile}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_contains_version_1() {
        let cfg = SandboxConfig::default();
        let profile =
            super::build_seatbelt_profile(&cfg).expect("default config must build cleanly");
        assert!(
            profile.contains("(version 1)"),
            "seatbelt profile must contain '(version 1)', got: {profile}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_allows_write_path_when_configured() {
        let cfg = SandboxConfig {
            allow_write: vec![crate::WriteRoot::new("/tmp")],
            trusted_roots: vec![crate::WriteRoot::new("/tmp")],
            ..Default::default()
        };
        let profile =
            super::build_seatbelt_profile(&cfg).expect("/tmp must exist and be canonicalizable");
        let canonical_tmp = PathBuf::from("/tmp")
            .canonicalize()
            .expect("/tmp must exist");
        let expected = format!(
            "(allow file-write* (subpath \"{}\"))",
            canonical_tmp.display()
        );
        assert!(
            profile.contains(&expected),
            "seatbelt profile must contain write-allow rule for {}, got: {profile}",
            canonical_tmp.display()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_omits_nonexistent_allow_write_path() {
        let cfg = SandboxConfig {
            allow_write: vec![crate::WriteRoot::new("/nonexistent_aegis_test_path_xyz")],
            trusted_roots: vec![crate::WriteRoot::new("/nonexistent_aegis_test_path_xyz")],
            ..Default::default()
        };

        let profile = super::build_seatbelt_profile(&cfg).expect("omitted root is not an error");

        assert!(!profile.contains("allow file-write*"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_network_allowed_when_configured() {
        let cfg = SandboxConfig {
            allow_network: true,
            ..Default::default()
        };
        let profile =
            super::build_seatbelt_profile(&cfg).expect("network-only config must build cleanly");
        assert!(
            profile.contains("(allow network*)"),
            "seatbelt profile must contain '(allow network*)' when allow_network=true, got: {profile}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_denies_network_by_default() {
        let cfg = SandboxConfig {
            allow_network: false,
            ..Default::default()
        };
        let profile =
            super::build_seatbelt_profile(&cfg).expect("default network config must build cleanly");
        assert!(
            profile.contains("(deny network*)"),
            "seatbelt profile must contain '(deny network*)' when allow_network=false, got: {profile}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_seatbelt_profile_no_write_rules_when_allow_write_empty() {
        let cfg = SandboxConfig {
            allow_write: vec![],
            ..Default::default()
        };
        let profile = super::build_seatbelt_profile(&cfg)
            .expect("empty allow_write config must build cleanly");
        assert!(
            !profile.contains("allow file-write*"),
            "seatbelt profile must NOT contain 'allow file-write*' when allow_write is empty, got: {profile}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_sandbox_available_for_returns_false_when_forced_unavailable_macos() {
        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;

        let _exec_present = super::is_sandbox_exec_available();
        assert!(
            !sandbox_available_for(&SandboxConfig::default()),
            "sandbox_available_for must return false when FORCE_SANDBOX_UNAVAILABLE is set"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_prepare_for_exec_macos_unavailable_returns_required_error() {
        use std::ffi::{OsStr, OsString};

        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;

        let cfg = SandboxConfig::default();
        let program = OsStr::new("echo");
        let args: Vec<OsString> = vec![OsString::from("hello")];

        let result = super::prepare_for_exec(&cfg, program, &args);
        assert!(
            matches!(result, Err(SandboxError::Required)),
            "prepare_for_exec must return Err(SandboxError::Required) when forced-unavailable, got: {result:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn escape_sbpl_path_escapes_double_quote() {
        let path = std::path::Path::new("/tmp/fo\"o");
        let escaped = super::escape_sbpl_path(path).expect("ASCII path must not fail");
        assert_eq!(
            escaped, r#"/tmp/fo\"o"#,
            "double-quote must be escaped as \\\""
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn escape_sbpl_path_escapes_backslash() {
        let path = std::path::Path::new("/tmp/fo\\o");
        let escaped = super::escape_sbpl_path(path).expect("ASCII path must not fail");
        assert_eq!(
            escaped, r#"/tmp/fo\\o"#,
            "backslash must be escaped as \\\\"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn escape_sbpl_path_escapes_backslash_before_quote_to_avoid_double_escaping() {
        let path = std::path::Path::new("/tmp/fo\\\"o");
        let escaped = super::escape_sbpl_path(path).expect("ASCII path must not fail");
        assert_eq!(escaped, r#"/tmp/fo\\\"o"#);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn static_default_profile_matches_dynamic_output() {
        let static_profile = include_str!("../profiles/default.sbpl").replace("\r\n", "\n");
        let dynamic = super::build_seatbelt_profile(&SandboxConfig::default())
            .expect("default config must build cleanly");
        assert_eq!(
            dynamic.trim(),
            static_profile.trim(),
            "profiles/default.sbpl has drifted from build_seatbelt_profile output"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn static_network_profile_matches_dynamic_output() {
        let static_profile = include_str!("../profiles/network.sbpl").replace("\r\n", "\n");
        let dynamic = super::build_seatbelt_profile(&SandboxConfig {
            allow_network: true,
            ..Default::default()
        })
        .expect("network config must build cleanly");
        assert_eq!(
            dynamic.trim(),
            static_profile.trim(),
            "profiles/network.sbpl has drifted from build_seatbelt_profile output"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_spawn_allows_write_under_allow_write_root() {
        use std::ffi::{OsStr, OsString};
        let dir = std::env::temp_dir();
        let cfg = SandboxConfig {
            allow_write: vec![crate::WriteRoot::new(dir.clone())],
            trusted_roots: vec![crate::WriteRoot::new(dir.clone())],
            ..Default::default()
        };
        if super::sandbox_available_for(&cfg) {
            let test_file = dir.join("aegis_sandbox_test_write_allowed.tmp");
            let args = vec![
                OsString::from("-c"),
                OsString::from(format!("touch {}", test_file.display())),
            ];
            let mut cmd = super::prepare_for_spawn(&cfg, OsStr::new("/bin/sh"), &args)
                .expect("prepare_for_spawn must succeed");
            let status = cmd.command.status().expect("spawned command must run");
            let _ = std::fs::remove_file(&test_file);
            assert!(
                status.success(),
                "write under an allow_write root must succeed"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_spawn_blocks_network_when_allow_network_is_false() {
        use std::ffi::{OsStr, OsString};
        let cfg = SandboxConfig {
            allow_network: false,
            ..Default::default()
        };
        if super::sandbox_available_for(&cfg) {
            let args = vec![
                OsString::from("-c"),
                OsString::from("ping -c 1 -t 1 127.0.0.1 2>/dev/null"),
            ];
            let mut cmd = super::prepare_for_spawn(&cfg, OsStr::new("/bin/sh"), &args)
                .expect("prepare_for_spawn must succeed");
            let status = cmd.command.status().expect("spawned command must run");
            assert!(
                !status.success(),
                "network operation must fail when allow_network=false"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_establishes_seatbelt_when_sandbox_exec_exists() {
        use std::ffi::OsStr;

        const KNOWN_GOOD_PROBE: &str =
            "(version 1)\n(deny default)\n(allow process*)\n(allow file-read*)\n";
        let seatbelt_available = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", KNOWN_GOOD_PROBE, "/usr/bin/true"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !seatbelt_available {
            return;
        }

        let mut prepared =
            super::prepare_for_exec(&SandboxConfig::default(), OsStr::new("/usr/bin/true"), &[])
                .expect("prepare_for_exec must prepare a valid Seatbelt command");

        assert_eq!(
            prepared.status,
            SandboxStatus::Active,
            "a present sandbox-exec must establish the Sandbox rather than silently fall back"
        );
        let status = prepared
            .command
            .status()
            .expect("prepared Seatbelt command must spawn");
        assert!(
            status.success(),
            "sandboxed /usr/bin/true must exit successfully"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_returns_sandbox_exec_command_when_available() {
        use std::ffi::OsStr;
        if !super::sandbox_available_for(&SandboxConfig::default()) {
            return;
        }
        let cmd =
            super::prepare_for_exec(&SandboxConfig::default(), OsStr::new("/usr/bin/true"), &[])
                .expect("prepare_for_exec must succeed when sandbox_available_for returned true");
        assert_eq!(
            cmd.command.get_program(),
            "/usr/bin/sandbox-exec",
            "returned command must be sandbox-exec when sandbox is available"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_command_runs_successfully_when_spawned() {
        use std::ffi::OsStr;
        if !super::sandbox_available_for(&SandboxConfig::default()) {
            return;
        }
        let mut cmd =
            super::prepare_for_exec(&SandboxConfig::default(), OsStr::new("/usr/bin/true"), &[])
                .expect("prepare_for_exec must succeed");
        let status = cmd.command.status().expect("spawned command must run");
        assert!(status.success(), "sandboxed /usr/bin/true must exit 0");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_runs_zsh_with_a_dev_null_redirect() {
        use std::ffi::{OsStr, OsString};
        if super::sandbox_available_for(&SandboxConfig::default()) {
            let args = vec![OsString::from("-c"), OsString::from("print ok >/dev/null")];
            let mut cmd =
                super::prepare_for_exec(&SandboxConfig::default(), OsStr::new("/bin/zsh"), &args)
                    .expect("prepare_for_exec must succeed");
            let status = cmd.command.status().expect("spawned command must run");
            assert!(
                status.success(),
                "sandboxed zsh with a /dev/null redirect must exit 0"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_blocks_write_outside_allow_write() {
        use std::ffi::{OsStr, OsString};
        if !super::sandbox_available_for(&SandboxConfig::default()) {
            return;
        }
        let args = vec![
            OsString::from("-c"),
            OsString::from("touch /tmp/aegis_pfe_blocked.tmp"),
        ];
        let mut cmd =
            super::prepare_for_exec(&SandboxConfig::default(), OsStr::new("/bin/sh"), &args)
                .expect("prepare_for_exec must succeed");
        let status = cmd.command.status().expect("spawned command must run");
        assert!(
            !status.success(),
            "write to /tmp must be blocked when allow_write is empty"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepare_for_exec_reports_a_missing_program_path_as_not_found() {
        use std::ffi::OsStr;

        let result = super::prepare_for_exec(
            &SandboxConfig::default(),
            OsStr::new("/nonexistent/shell/binary"),
            &[],
        );
        assert!(
            matches!(&result, Err(SandboxError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound),
            "a missing program path must fail before sandbox-exec runs, got: {result:?}"
        );
    }

    #[test]
    fn stderr_indicates_nested_refusal_matches_the_measured_kernel_message() {
        assert!(super::stderr_indicates_nested_refusal(
            "sandbox_apply: Operation not permitted\n"
        ));
    }

    #[test]
    fn stderr_indicates_nested_refusal_rejects_unrelated_failures() {
        assert!(!super::stderr_indicates_nested_refusal(
            "sandbox-exec: unknown profile syntax\n"
        ));
        assert!(!super::stderr_indicates_nested_refusal(""));
        assert!(!super::stderr_indicates_nested_refusal(
            "Operation not permitted"
        ));
        assert!(!super::stderr_indicates_nested_refusal(
            "sandbox_apply: failed"
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_prepare_for_exec_macos_forced_unavailable_never_reports_nested_cause() {
        use std::ffi::{OsStr, OsString};

        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;

        let cfg = SandboxConfig::default();
        let program = OsStr::new("echo");
        let args: Vec<OsString> = vec![OsString::from("hello")];

        assert!(
            matches!(
                super::prepare_for_exec(&cfg, program, &args),
                Err(SandboxError::Required)
            ),
            "forced-unavailable must report the generic cause, not a probed nested refusal"
        );
    }
}
