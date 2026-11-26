use std::ffi::{OsStr, OsString};

use crate::{PreparedSandboxCommand, SandboxConfig, SandboxError};

pub(crate) fn sandbox_available_for(_config: &SandboxConfig) -> bool {
    false
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
    _config: &SandboxConfig,
    _program: &OsStr,
    _args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    Err(SandboxError::Required)
}
