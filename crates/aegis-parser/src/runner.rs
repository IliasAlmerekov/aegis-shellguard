use crate::program_basename;

/// Supported command runners whose explicit child program can be identified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Runner {
    /// The `uv run` subcommand.
    Uv,
    /// The `uvx` tool runner.
    Uvx,
    /// The `poetry run` subcommand.
    Poetry,
    /// The `pipenv run` subcommand.
    Pipenv,
    /// The `pipx run` subcommand.
    Pipx,
    /// The `npx` package runner.
    Npx,
}

impl Runner {
    /// Recognize a runner by program basename, including path-qualified argv.
    pub fn from_program(program: &str) -> Option<Self> {
        match program_basename(program) {
            "uv" => Some(Self::Uv),
            "uvx" => Some(Self::Uvx),
            "poetry" => Some(Self::Poetry),
            "pipenv" => Some(Self::Pipenv),
            "pipx" => Some(Self::Pipx),
            "npx" => Some(Self::Npx),
            _ => None,
        }
    }

    /// `true` when `tokens` starts with `uv tool run`, the long form of `uvx` (#421, ADR-040).
    fn is_uv_tool_run(tokens: &[&str]) -> bool {
        tokens.get(1) == Some(&"tool") && tokens.get(2) == Some(&"run")
    }

    /// Return the length of a supported, unambiguous runner prefix.
    pub fn command_prefix_len(self, tokens: &[&str]) -> Option<usize> {
        match self {
            Self::Uv if tokens.get(1) == Some(&"run") => {
                Some(if matches!(tokens.get(2), Some(&"--script" | &"--")) {
                    3
                } else {
                    2
                })
            }
            Self::Uv if Self::is_uv_tool_run(tokens) => Some(3),
            Self::Poetry | Self::Pipenv | Self::Pipx if tokens.get(1) == Some(&"run") => Some(2),
            Self::Uvx | Self::Npx => Some(1),
            _ => None,
        }
    }

    /// `true` when this runner uses a literal `run` subcommand that options can precede (#421, ADR-040).
    pub fn has_run_subcommand(self) -> bool {
        matches!(self, Self::Uv | Self::Poetry | Self::Pipenv | Self::Pipx)
    }

    /// `true` when `tokens` invoke a package-provided executable (`uvx`, `npx`,
    /// `pipx run`, `uv tool run`), which never cancels Analysis degradation (#421, ADR-040).
    ///
    /// Callers check [`Self::runs_bare_python_script`] first for `pipx run ./x.py`.
    pub fn selects_package_executable(self, tokens: &[&str]) -> bool {
        match self {
            Self::Uvx | Self::Npx | Self::Pipx => self.command_prefix_len(tokens).is_some(),
            Self::Uv => Self::is_uv_tool_run(tokens),
            Self::Poetry | Self::Pipenv => false,
        }
    }

    /// `true` when `tokens` treat a bare `.py` operand as Python (`uv run`, `pipx run`) (#421, ADR-040).
    pub fn runs_bare_python_script(self, tokens: &[&str]) -> bool {
        matches!(self, Self::Uv | Self::Pipx) && tokens.get(1) == Some(&"run")
    }

    /// `true` when `tokens` force the operand after `run` to be treated as Python (`uv run --script`) (#421, ADR-040).
    pub fn forces_python_script(self, tokens: &[&str]) -> bool {
        matches!(self, Self::Uv)
            && tokens.get(1) == Some(&"run")
            && tokens.get(2) == Some(&"--script")
    }
}
