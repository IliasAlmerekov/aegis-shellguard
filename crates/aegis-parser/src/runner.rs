use crate::program_basename;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Runner {
    Uv,
    Uvx,
    Poetry,
    Pipenv,
    Pipx,
    Npx,
}

impl Runner {
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

    fn is_uv_tool_run(tokens: &[&str]) -> bool {
        tokens.get(1) == Some(&"tool") && tokens.get(2) == Some(&"run")
    }

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

    pub fn has_run_subcommand(self) -> bool {
        matches!(self, Self::Uv | Self::Poetry | Self::Pipenv | Self::Pipx)
    }

    pub fn selects_package_executable(self, tokens: &[&str]) -> bool {
        match self {
            Self::Uvx | Self::Npx | Self::Pipx => self.command_prefix_len(tokens).is_some(),
            Self::Uv => Self::is_uv_tool_run(tokens),
            Self::Poetry | Self::Pipenv => false,
        }
    }

    pub fn runs_bare_python_script(self, tokens: &[&str]) -> bool {
        matches!(self, Self::Uv | Self::Pipx) && tokens.get(1) == Some(&"run")
    }

    pub fn forces_python_script(self, tokens: &[&str]) -> bool {
        matches!(self, Self::Uv)
            && tokens.get(1) == Some(&"run")
            && tokens.get(2) == Some(&"--script")
    }
}
