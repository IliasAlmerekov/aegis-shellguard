//! Typed error for the hook, shell, and uninstall installers.
//!
//! Every `Display` text is the message the installers printed before the error
//! was typed, so `aegis install-hooks`, `aegis setup-shell`, and `aegis
//! uninstall` keep their exact output.

use std::path::PathBuf;

/// Everything that can go wrong installing or removing hooks and shell setup.
#[derive(thiserror::Error, Debug)]
pub(crate) enum InstallError {
    /// A file operation failed. `action` is the whole verb phrase, such as
    /// `failed to read` or `cannot replace`.
    #[error("{action} {}: {source}", path.display())]
    Io {
        /// The failed operation, worded for the message.
        action: &'static str,
        /// The path the operation acted on.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// An I/O error reported without a path.
    #[error("{0}")]
    Os(std::io::Error),

    /// A process-level lookup failed, such as the current directory or the
    /// running binary.
    #[error("{what}: {source}")]
    Resolve {
        /// The failed lookup, worded for the message.
        what: &'static str,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// An existing file is not valid JSON.
    #[error("failed to parse {} as JSON: {source}", path.display())]
    ParseJson {
        /// The offending file.
        path: PathBuf,
        /// The parse failure.
        source: serde_json::Error,
    },

    /// An existing file is not valid TOML.
    #[error("failed to parse {} as TOML: {source}", path.display())]
    ParseToml {
        /// The offending file.
        path: PathBuf,
        /// The parse failure.
        source: toml::de::Error,
    },

    /// A JSON value could not be serialized.
    #[error("{action} {}: {source}", path.display())]
    SerializeJson {
        /// The failed operation, worded for the message.
        action: &'static str,
        /// The file the value was meant for.
        path: PathBuf,
        /// The serialization failure.
        source: serde_json::Error,
    },

    /// A TOML value could not be serialized.
    #[error("failed to serialize TOML for {}: {source}", path.display())]
    SerializeToml {
        /// The file the value was meant for.
        path: PathBuf,
        /// The serialization failure.
        source: toml::ser::Error,
    },

    /// A settings file parsed, but its top level is not a JSON object.
    #[error("{} must contain a top-level JSON object", path.display())]
    NotJsonObject {
        /// The offending file.
        path: PathBuf,
    },

    /// A config file parsed, but its top level is not a TOML table.
    #[error("{} must contain a top-level TOML table", path.display())]
    NotTomlTable {
        /// The offending file.
        path: PathBuf,
    },

    /// A settings or config document has a value of the wrong shape.
    #[error("{0}")]
    InvalidStructure(String),

    /// The agent directory exists but is a file.
    #[error("{} exists but is not a directory", path.display())]
    NotADirectory {
        /// The agent directory path.
        path: PathBuf,
    },

    /// A path cannot be used as given.
    #[error("{} {reason}", path.display())]
    InvalidPath {
        /// The rejected path.
        path: PathBuf,
        /// Why it was rejected.
        reason: &'static str,
    },

    /// A path that must be UTF-8 is not.
    #[error("{0}")]
    NotUtf8(&'static str),

    /// `HOME` and `USERPROFILE` are both unset or empty.
    #[error("HOME is not set")]
    HomeNotSet,

    /// `HOME` resolves to something uninstall must not work from.
    #[error("HOME must be a directory other than the filesystem root")]
    HomeInvalid,

    /// The real shell resolves to the Aegis binary.
    #[error(
        "the resolved real shell is the Aegis binary itself, which would \
         cause infinite recursion; pass --shell /bin/zsh or set \
         AEGIS_REAL_SHELL to your real shell"
    )]
    RealShellIsAegis,

    /// No real shell came from the flag or the environment.
    #[error("cannot determine the real shell; pass --shell /bin/zsh or set AEGIS_REAL_SHELL")]
    RealShellUnknown,

    /// A path to be written into a shell rc file is empty.
    #[error("{label} cannot be empty")]
    EmptyShellPath {
        /// What the path is, such as `real shell path`.
        label: &'static str,
    },

    /// A path to be written into a shell rc file holds a control character.
    #[error("{label} contains unsafe control characters")]
    ControlCharsInPath {
        /// What the path is, such as `real shell path`.
        label: &'static str,
    },

    /// The shell name cannot be read from its path.
    #[error("cannot determine shell name from {}", path.display())]
    UnnamedShell {
        /// The real shell path.
        path: PathBuf,
    },

    /// Automatic rc-file choice supports only bash and zsh.
    #[error("automatic shell setup supports bash and zsh; pass --rc-file for {name}")]
    UnsupportedShell {
        /// The shell name found.
        name: String,
    },

    /// `--rc-file` is not an absolute path.
    #[error("--rc-file must be an absolute path")]
    RcFileNotAbsolute,

    /// `--rc-file` names no file.
    #[error("--rc-file {} must name a file", path.display())]
    RcFileUnnamed {
        /// The `--rc-file` value.
        path: PathBuf,
    },

    /// A symlink sits on the path uninstall would change.
    #[error("refusing to change {} through symlink {}. {hint}", path.display(), link.display())]
    SymlinkRefused {
        /// The file uninstall would change.
        path: PathBuf,
        /// The symlink on the way to it.
        link: PathBuf,
        /// How to finish the cleanup by hand.
        hint: String,
    },

    /// A managed shell block has a stray or nested marker.
    #[error("malformed managed shell block; repair it before uninstalling")]
    MalformedShellBlock,

    /// A managed shell block has no end marker.
    #[error("unterminated managed shell block; repair it before uninstalling")]
    UnterminatedShellBlock,
}

impl InstallError {
    /// Build an [`InstallError::Io`] mapper for `.map_err`.
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
