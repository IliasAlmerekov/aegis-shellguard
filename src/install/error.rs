use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub(crate) enum InstallError {
    #[error("{action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    Os(std::io::Error),

    #[error("{what}: {source}")]
    Resolve {
        what: &'static str,
        source: std::io::Error,
    },

    #[error("failed to parse {} as JSON: {source}", path.display())]
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to parse {} as TOML: {source}", path.display())]
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("{action} {}: {source}", path.display())]
    SerializeJson {
        action: &'static str,
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to serialize TOML for {}: {source}", path.display())]
    SerializeToml {
        path: PathBuf,
        source: toml::ser::Error,
    },

    #[error("{} must contain a top-level JSON object", path.display())]
    NotJsonObject { path: PathBuf },

    #[error("{} must contain a top-level TOML table", path.display())]
    NotTomlTable { path: PathBuf },

    #[error("{0}")]
    InvalidStructure(String),

    #[error("{} exists but is not a directory", path.display())]
    NotADirectory { path: PathBuf },

    #[error("{} {reason}", path.display())]
    InvalidPath { path: PathBuf, reason: &'static str },

    #[error("{0}")]
    NotUtf8(&'static str),

    #[error("HOME is not set")]
    HomeNotSet,

    #[error("HOME must be a directory other than the filesystem root")]
    HomeInvalid,

    #[error(
        "the resolved real shell is the Aegis binary itself, which would \
         cause infinite recursion; pass --shell /bin/zsh or set \
         AEGIS_REAL_SHELL to your real shell"
    )]
    RealShellIsAegis,

    #[error("cannot determine the real shell; pass --shell /bin/zsh or set AEGIS_REAL_SHELL")]
    RealShellUnknown,

    #[error("{label} cannot be empty")]
    EmptyShellPath { label: &'static str },

    #[error("{label} contains unsafe control characters")]
    ControlCharsInPath { label: &'static str },

    #[error("cannot determine shell name from {}", path.display())]
    UnnamedShell { path: PathBuf },

    #[error("automatic shell setup supports bash and zsh; pass --rc-file for {name}")]
    UnsupportedShell { name: String },

    #[error("--rc-file must be an absolute path")]
    RcFileNotAbsolute,

    #[error("--rc-file {} must name a file", path.display())]
    RcFileUnnamed { path: PathBuf },

    #[error("refusing to change {} through symlink {}. {hint}", path.display(), link.display())]
    SymlinkRefused {
        path: PathBuf,
        link: PathBuf,
        hint: String,
    },

    #[error("malformed managed shell block; repair it before uninstalling")]
    MalformedShellBlock,

    #[error("unterminated managed shell block; repair it before uninstalling")]
    UnterminatedShellBlock,
}

impl InstallError {
    pub(crate) fn io(
        action: &'static str,
        path: &std::path::Path,
    ) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.to_path_buf();
        move |source| Self::Io {
            action,
            path,
            source,
        }
    }
}
