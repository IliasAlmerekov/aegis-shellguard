use std::ffi::OsStr;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AmbientEnv {
    pub(crate) git_env: bool,
    pub(crate) cdpath_set: bool,
    pub(crate) physical: bool,
    pub(crate) cd_function: bool,
    pub(crate) lastpipe: bool,
    pub(crate) zsh: bool,
}

impl AmbientEnv {
    pub(super) fn from_process() -> Self {
        Self {
            git_env: ["GIT_DIR", "GIT_WORK_TREE"]
                .iter()
                .any(|name| std::env::var_os(name).is_some()),
            cdpath_set: std::env::var_os("CDPATH").is_some(),
            physical: env_text("SHELLOPTS").contains("physical"),
            cd_function: std::env::var_os("BASH_FUNC_cd%%").is_some(),
            lastpipe: env_text("BASHOPTS").contains("lastpipe"),
            zsh: shell_is_zsh(
                std::env::var_os("AEGIS_REAL_SHELL").as_deref(),
                std::env::var_os("SHELL").as_deref(),
            ),
        }
    }
}

pub(crate) fn shell_is_zsh(real_shell: Option<&OsStr>, shell: Option<&OsStr>) -> bool {
    [real_shell, shell]
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .is_some_and(|path| Path::new(path).file_name() == Some(OsStr::new("zsh")))
}

fn env_text(name: &str) -> String {
    std::env::var_os(name)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}
