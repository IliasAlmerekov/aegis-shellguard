use std::ffi::{OsStr, OsString};

use crate::{PreparedSandboxCommand, SandboxConfig, SandboxError};

pub(crate) fn sandbox_available_for(config: &SandboxConfig) -> bool {
    crate::bwrap::available_bwrap_program(config).is_some()
}

pub(crate) fn prepare_for_exec(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    prepare(config, program, args, true)
}

pub(crate) fn prepare_for_spawn(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    prepare(config, program, args, false)
}

fn prepare(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
    apply_exec_restrictions: bool,
) -> Result<PreparedSandboxCommand, SandboxError> {
    let mut bwrap_args = crate::bwrap::build_bwrap_args(config)?;
    let Some(bwrap_program) = crate::bwrap::available_bwrap_program(config) else {
        return Err(SandboxError::Required);
    };

    let use_landlock_wrapper = apply_exec_restrictions;
    let mut cmd = std::process::Command::new(bwrap_program);

    if use_landlock_wrapper {
        bwrap_args.extend(crate::landlock::build_landlock_wrapper_args(program, args)?);

        let encoded = crate::landlock::serialize_allow_write(
            &crate::support::resolve_allow_write(config).paths,
        );
        if encoded.len() > crate::landlock::MAX_ALLOW_WRITE_ENV_LEN {
            return Err(SandboxError::Execution(
                "allow_write config too large to pass to the inner wrapper".into(),
            ));
        }
        use std::os::unix::ffi::OsStringExt;
        cmd.env(
            crate::landlock::AEGIS_LANDLOCK_ALLOW_WRITE,
            OsString::from_vec(encoded),
        );
    } else {
        bwrap_args.push(program.to_owned());
        bwrap_args.extend_from_slice(args);
    }

    cmd.args(&bwrap_args);
    Ok(PreparedSandboxCommand::active(cmd, use_landlock_wrapper))
}

#[cfg(test)]
mod tests {

    use crate::support::set_force_sandbox_unavailable;
    use crate::support::test_helpers::ForceUnavailableGuard;
    use crate::{SandboxConfig, SandboxError, sandbox_available_for};

    use super::prepare_for_exec;
    use crate::bwrap::sysctl_userns_available;

    #[cfg(target_os = "linux")]
    #[test]
    fn sandbox_available_for_returns_false_when_forced_unavailable() {
        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;
        assert!(!sandbox_available_for(&SandboxConfig::default()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sysctl_userns_available_returns_true_when_file_absent() {
        let file_present =
            std::path::Path::new("/proc/sys/kernel/unprivileged_userns_clone").exists();
        if !file_present {
            assert!(
                sysctl_userns_available(),
                "must return true when sysctl file is absent"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_sysctl_userns_available_returns_true_when_file_missing() {
        let file_present =
            std::path::Path::new("/proc/sys/kernel/unprivileged_userns_clone").exists();

        if file_present {
            let expected = std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
                .map(|v| v.trim() == "1")
                .unwrap_or(true);
            assert_eq!(sysctl_userns_available(), expected);
        } else {
            assert!(sysctl_userns_available());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prepare_for_exec_fails_closed_when_unavailable() {
        use std::ffi::OsStr;

        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;
        let cfg = SandboxConfig::default();
        let err = prepare_for_exec(&cfg, OsStr::new("/usr/bin/true"), &[])
            .expect_err("unavailability must fail closed with SandboxError::Required");

        assert!(matches!(err, SandboxError::Required));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prepare_for_spawn_fails_closed_when_unavailable() {
        use std::ffi::OsStr;

        set_force_sandbox_unavailable(true);
        let _guard = ForceUnavailableGuard;
        let cfg = SandboxConfig::default();

        let err = super::prepare_for_spawn(&cfg, OsStr::new("/usr/bin/true"), &[])
            .expect_err("unavailability must fail closed with SandboxError::Required");

        assert!(matches!(err, SandboxError::Required));
    }
}
