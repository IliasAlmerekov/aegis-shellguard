use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::Value;

use super::{InstallError, is_aegis_managed_bash_command, is_aegis_managed_session_start_command};

/// Removal channel: which package channel's binary-removal command to print. Not update consent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum RemovalChannel {
    Npm,
    Homebrew,
    Cargo,
    Curl,
}

const PAYLOADS: &[&str] = &[
    ".claude/hooks/aegis-rewrite.sh",
    ".claude/hooks/aegis-pre-tool-use.sh",
    ".claude/hooks/aegis-session-start.sh",
    ".codex/hooks/aegis-pre-tool-use.sh",
    ".codex/hooks/aegis-session-start.sh",
    ".aegis/lib/toggle-state.sh",
];

/// Removes managed integrations and reports the separate binary-removal step.
pub(crate) fn run(args: &crate::UninstallArgs) -> i32 {
    match uninstall(args) {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(error) => {
            eprintln!("error: uninstall incomplete: {error}");
            crate::EXIT_INTERNAL
        }
    }
}

fn uninstall(args: &crate::UninstallArgs) -> Result<String, InstallError> {
    let home = super::home_dir().ok_or(InstallError::HomeNotSet)?;
    let home = fs::canonicalize(&home).map_err(InstallError::io("cannot resolve HOME", &home))?;
    if !home.is_dir() || home.parent().is_none() {
        return Err(InstallError::HomeInvalid);
    }
    let invoked = std::env::current_exe().map_err(|source| InstallError::Resolve {
        what: "cannot resolve the running binary",
        source,
    })?;
    let (binary, detected) = removal_target(&invoked, &home);
    let channel = args.channel.or(detected);
    let data = home.join(".aegis");
    // A kept data directory is never followed through a symlink, so payloads
    // inside it stay with the data. A purge still refuses the link below.
    let keep_linked_data = !args.purge_data
        && fs::symlink_metadata(&data).is_ok_and(|metadata| metadata.file_type().is_symlink());
    let payloads: Vec<_> = PAYLOADS
        .iter()
        .map(|path| home.join(path))
        .filter(|path| !(keep_linked_data && path.starts_with(&data)))
        .collect();
    let mut rc_paths = vec![home.join(".bashrc"), home.join(".zshrc")];
    if let Some(path) = &args.rc_file {
        if !path.is_absolute() {
            return Err(InstallError::RcFileNotAbsolute);
        }
        let path = resolve_rc_parent(path)?;
        if !rc_paths.contains(&path) {
            rc_paths.push(path);
        }
    }

    // Validate every known target before changing any integration.
    if args.purge_data {
        reject_symlinks(&data, "Delete it by hand if you mean to purge it.")?;
    }
    // Like startup files, a payload or settings file is checked for symlinks
    // only when uninstall will change it, so a dotfile manager's linked
    // `~/.claude` with nothing of Aegis inside does not block the rest.
    let payloads: Vec<_> = payloads
        .into_iter()
        .filter(|path| fs::symlink_metadata(path).is_ok())
        .collect();
    for path in &payloads {
        reject_symlinks(
            path,
            "Delete the file by hand, then rerun `aegis uninstall`.",
        )?;
    }
    let mut settings = Vec::new();
    for relative in [".claude/settings.json", ".codex/hooks.json"] {
        let path = home.join(relative);
        if path.try_exists().map_err(InstallError::Os)? {
            let mut value = super::load_settings(&path)?;
            if prune_registrations(&mut value)? {
                reject_symlinks(
                    &path,
                    "Remove the Aegis hook entries from it by hand, then rerun `aegis uninstall`.",
                )?;
                settings.push((path, value));
            }
        }
    }
    let mut shell_files = Vec::new();
    for path in &rc_paths {
        let Some(content) = read_optional(path)? else {
            continue;
        };
        if has_managed_block(&content)? {
            // Only a file that will be rewritten must not be a symlink, so a
            // dotfile manager's link without an Aegis block stays usable.
            reject_symlinks(
                path,
                "Remove the aegis shell setup block from it by hand, then rerun `aegis uninstall`.",
            )?;
            shell_files.push((path.clone(), strip_managed_blocks(&content)));
        }
    }
    for (path, value) in settings {
        let content =
            serde_json::to_string_pretty(&value).map_err(|source| InstallError::SerializeJson {
                action: "cannot serialize",
                path: path.clone(),
                source,
            })?;
        write_existing_file(&path, &format!("{content}\n"))?;
    }
    let shell_message = if shell_files.is_empty() {
        format!(
            "No managed shell block found in {}.",
            display_paths(rc_paths.iter())
        )
    } else {
        format!(
            "Removed managed shell blocks from {}.",
            display_paths(shell_files.iter().map(|(path, _)| path))
        )
    };
    for (path, content) in &shell_files {
        write_existing_file(path, content)?;
    }
    for path in payloads {
        remove_if_present(&path).map_err(InstallError::io("cannot remove", &path))?;
    }
    let data_message = if args.purge_data {
        match fs::remove_dir_all(&data) {
            Ok(()) => format!("Removed data directory {}.", data.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                format!("No data directory at {}.", data.display())
            }
            Err(error) => return Err(InstallError::io("cannot purge", &data)(error)),
        }
    } else {
        format!(
            "Data kept at {} if present. Use --purge-data to delete audit logs, snapshots, and update state.",
            data.display()
        )
    };
    // install-hooks turns this Codex flag on, but other Codex hooks may rely
    // on it, so uninstall reports it instead of turning it off.
    let codex_message = if codex_hooks_feature_enabled(&home.join(".codex/config.toml")) {
        "\nCodex features.hooks stays enabled in ~/.codex/config.toml; turn it off there if no other hook needs it."
    } else {
        ""
    };
    let channel_message = if args.channel.is_some() {
        "Using the explicitly selected removal channel."
    } else if channel.is_some() {
        "Removal channel inferred from the canonical binary path, not verified package ownership."
    } else {
        "No known removal channel matches the binary path."
    };
    // A block in a startup file uninstall did not check keeps exporting
    // SHELL as this binary, which breaks new terminals once it is removed.
    let shell_is_aegis = std::env::var_os("SHELL")
        .is_some_and(|shell| crate::shell_compat::same_file(Path::new(&shell), Some(&binary)));
    if shell_files.is_empty() && shell_is_aegis {
        eprintln!(
            "warning: $SHELL is this Aegis binary, but no checked startup file holds a managed shell block. If you ran `aegis setup-shell --rc-file <path>`, rerun `aegis uninstall --rc-file <path>`."
        );
    }
    Ok(format!(
        "{shell_message}\nRemoved Aegis agent hooks where present.{codex_message}\n{data_message}\nUser configuration and unrelated hooks were kept.\nProject-local hooks from `aegis install-hooks --local` were not touched. Projects where you ran it need manual cleanup: the hook shim denies every Bash command once the binary is gone.\n{channel_message}\nBinary kept at {}. Remove it separately:\n  {}\nOpen a new terminal after removal.",
        binary.display(),
        removal_command(channel, &binary)
    ))
}

fn codex_hooks_feature_enabled(config: &Path) -> bool {
    // Dropped on purpose: returning this error changes the output and exit code of `aegis uninstall`; tracked in #655.
    fs::read_to_string(config)
        .ok()
        .and_then(|raw| raw.parse::<toml::Table>().ok())
        .and_then(|table| table.get("features")?.get("hooks")?.as_bool())
        .unwrap_or(false)
}

fn display_paths<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> String {
    paths
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves symlinks in the directories above a custom startup file, so a
/// path through a linked directory (macOS `/var`, `/tmp`) names the real
/// file. The file itself stays unresolved: a symlinked startup file is still
/// refused before it is rewritten.
fn resolve_rc_parent(path: &Path) -> Result<PathBuf, InstallError> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(InstallError::RcFileUnnamed {
            path: path.to_path_buf(),
        });
    };
    match fs::canonicalize(parent) {
        Ok(parent) => Ok(parent.join(name)),
        // A missing directory holds no startup file to clean.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(InstallError::io("cannot resolve", parent)(error)),
    }
}

/// Refuses `path` when it or a directory above it is a symlink. `hint` tells
/// the operator how to finish that cleanup by hand.
fn reject_symlinks(path: &Path, hint: &str) -> Result<(), InstallError> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(InstallError::SymlinkRefused {
                    path: path.to_path_buf(),
                    link: ancestor.to_path_buf(),
                    hint: hint.to_string(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(InstallError::io("cannot inspect", ancestor)(error)),
        }
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<String>, InstallError> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(InstallError::io("cannot read", path)(error)),
    }
}

fn has_managed_block(content: &str) -> Result<bool, InstallError> {
    let mut inside = false;
    let mut found = false;
    for line in content.lines() {
        match line {
            super::shell::BEGIN_MARKER if !inside => {
                inside = true;
                found = true;
            }
            super::shell::END_MARKER if inside => inside = false,
            super::shell::BEGIN_MARKER | super::shell::END_MARKER => {
                return Err(InstallError::MalformedShellBlock);
            }
            _ => {}
        }
    }
    if inside {
        return Err(InstallError::UnterminatedShellBlock);
    }
    Ok(found)
}

/// Drops managed shell blocks and keeps every other line byte for byte,
/// including its CRLF or LF ending. Callers validate the markers first.
fn strip_managed_blocks(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut inside = false;
    for line in content.split_inclusive('\n') {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        let bare = bare.strip_suffix('\r').unwrap_or(bare);
        match bare {
            super::shell::BEGIN_MARKER => inside = true,
            super::shell::END_MARKER => inside = false,
            _ if !inside => output.push_str(line),
            _ => {}
        }
    }
    output
}

fn prune_registrations(settings: &mut Value) -> Result<bool, InstallError> {
    let Some(hooks) = settings.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| InstallError::InvalidStructure("hooks must be a JSON object".into()))?;
    let mut changed = false;
    for section in ["PreToolUse", "SessionStart"] {
        let Some(entries) = hooks.get_mut(section) else {
            continue;
        };
        let entries = entries.as_array_mut().ok_or_else(|| {
            InstallError::InvalidStructure(format!("hooks.{section} must be an array"))
        })?;
        entries.retain_mut(|entry| {
            let Some(commands) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = commands.len();
            commands.retain(|hook| {
                let Some(command) = hook.get("command").and_then(Value::as_str) else {
                    return true;
                };
                hook.get("type").and_then(Value::as_str) != Some("command")
                    || if section == "SessionStart" {
                        !is_aegis_managed_session_start_command(command)
                    } else {
                        !is_aegis_managed_bash_command(command)
                    }
            });
            changed |= before != commands.len();
            before == commands.len() || !commands.is_empty()
        });
    }
    Ok(changed)
}

fn write_existing_file(path: &Path, content: &str) -> Result<(), InstallError> {
    let parent = path.parent().ok_or_else(|| InstallError::InvalidPath {
        path: path.to_path_buf(),
        reason: "has no parent directory",
    })?;
    let temporary = super::temporary_settings_path(parent);
    // Only clean up a temporary file that this invocation actually created.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(InstallError::io("cannot create", &temporary))?;
    let result = (|| -> Result<(), InstallError> {
        use std::io::Write;
        file.set_permissions(fs::metadata(path).map_err(InstallError::Os)?.permissions())
            .map_err(InstallError::Os)?;
        file.write_all(content.as_bytes())
            .map_err(InstallError::Os)?;
        file.sync_all().map_err(InstallError::Os)?;
        fs::rename(&temporary, path).map_err(InstallError::io("cannot replace", path))
    })();
    let Err(original) = result else {
        return Ok(());
    };
    match remove_if_present(&temporary) {
        Ok(()) => Err(original),
        Err(source) => Err(InstallError::CleanupFailed {
            original: Box::new(original),
            path: temporary,
            source,
        }),
    }
}

/// Removes `path`, treating a file that is already gone as removed.
fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Returns the canonical binary path and the Removal channel inferred from it.
fn removal_target(invoked: &Path, home: &Path) -> (PathBuf, Option<RemovalChannel>) {
    // Resolve symlinks first: a Homebrew link in the curl bindir must match
    // the Cellar path, not the bindir. The canonical path is also the file
    // that a curl removal command has to delete.
    let binary = fs::canonicalize(invoked).unwrap_or_else(|_| invoked.to_path_buf());
    let channel = detect_channel(&binary, home);
    (binary, channel)
}

fn detect_channel(binary: &Path, home: &Path) -> Option<RemovalChannel> {
    let components: Vec<_> = binary
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect();
    if components
        .windows(4)
        .any(|parts| parts == ["lib", "node_modules", "@iliasalmerekov", "aegis"])
    {
        return Some(RemovalChannel::Npm);
    }
    if components
        .windows(2)
        .any(|parts| parts == ["Cellar", "aegis"])
    {
        return Some(RemovalChannel::Homebrew);
    }
    let cargo_root = cargo_root(home);
    if crate::shell_compat::same_file(binary, Some(&cargo_root.join("bin/aegis"))) {
        return Some(RemovalChannel::Cargo);
    }
    let bindir = std::env::var_os("AEGIS_BINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    crate::shell_compat::same_file(binary, Some(&bindir.join("aegis")))
        .then_some(RemovalChannel::Curl)
}

fn cargo_root(home: &Path) -> PathBuf {
    explicit_cargo_root()
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cargo"))
}

fn explicit_cargo_root() -> Option<std::ffi::OsString> {
    std::env::var_os("CARGO_INSTALL_ROOT")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("CARGO_HOME").filter(|value| !value.is_empty()))
}

fn removal_command(channel: Option<RemovalChannel>, binary: &Path) -> String {
    match channel {
        Some(RemovalChannel::Npm) => "npm uninstall -g @iliasalmerekov/aegis".into(),
        Some(RemovalChannel::Homebrew) => "brew uninstall aegis".into(),
        Some(RemovalChannel::Cargo) => {
            if let Some(root) = explicit_cargo_root() {
                format!(
                    "cargo uninstall --root {} aegis",
                    super::shell_quote(&root.to_string_lossy())
                )
            } else {
                "cargo uninstall aegis".into()
            }
        }
        Some(RemovalChannel::Curl) => {
            format!("rm -- {}", super::shell_quote(&binary.to_string_lossy()))
        }
        None => format!(
            "Removal channel unknown. Rerun with --channel npm, homebrew, cargo, or curl after checking the installer for {}.",
            binary.display()
        ),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn removal_target_follows_a_bindir_link_into_the_homebrew_cellar() {
        let temp = tempfile::TempDir::new().unwrap();
        let cellar_binary = temp.path().join("Cellar/aegis/1.0/bin/aegis");
        fs::create_dir_all(cellar_binary.parent().unwrap()).unwrap();
        fs::write(&cellar_binary, "binary").unwrap();
        let bindir = temp.path().join("bin");
        fs::create_dir_all(&bindir).unwrap();
        let link = bindir.join("aegis");
        std::os::unix::fs::symlink("../Cellar/aegis/1.0/bin/aegis", &link).unwrap();

        let (binary, channel) = removal_target(&link, temp.path());

        assert_eq!(binary, fs::canonicalize(&cellar_binary).unwrap());
        assert_eq!(channel, Some(RemovalChannel::Homebrew));
    }

    #[test]
    fn cleanup_failed_display_names_the_write_failure_and_the_leftover_file() {
        let original = InstallError::io("cannot replace", Path::new("/h/settings.json"))(
            std::io::Error::other("disk full"),
        );
        let error = InstallError::CleanupFailed {
            original: Box::new(original),
            path: PathBuf::from("/h/.aegis-1.tmp"),
            source: std::io::Error::other("permission denied"),
        };
        assert_eq!(
            error.to_string(),
            "cannot replace /h/settings.json: disk full; cannot clean up /h/.aegis-1.tmp: permission denied"
        );
    }

    #[test]
    fn resolve_rc_parent_keeps_a_path_whose_directory_is_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let rc = temp.path().join("missing/.bashrc");

        assert_eq!(resolve_rc_parent(&rc).unwrap(), rc);
    }

    #[test]
    fn resolve_rc_parent_reports_a_directory_it_cannot_resolve() {
        let temp = tempfile::TempDir::new().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, "").unwrap();

        let error = resolve_rc_parent(&file.join("sub/.bashrc")).unwrap_err();

        assert!(matches!(
            error,
            InstallError::Io {
                action: "cannot resolve",
                ..
            }
        ));
    }

    #[test]
    fn reject_symlinks_accepts_a_missing_path() {
        let temp = tempfile::TempDir::new().unwrap();
        // macOS puts the temp dir under the `/var` symlink.
        let root = fs::canonicalize(temp.path()).unwrap();

        assert!(reject_symlinks(&root.join("missing"), "hint").is_ok());
    }

    #[test]
    fn reject_symlinks_reports_an_ancestor_it_cannot_inspect() {
        let temp = tempfile::TempDir::new().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, "").unwrap();

        let error = reject_symlinks(&file.join("child"), "hint").unwrap_err();

        assert!(matches!(
            error,
            InstallError::Io {
                action: "cannot inspect",
                ..
            }
        ));
    }

    #[test]
    fn read_optional_reports_a_path_it_cannot_read() {
        let temp = tempfile::TempDir::new().unwrap();

        let error = read_optional(temp.path()).unwrap_err();

        assert!(matches!(
            error,
            InstallError::Io {
                action: "cannot read",
                ..
            }
        ));
    }

    #[test]
    fn has_managed_block_rejects_an_end_marker_without_a_begin_marker() {
        let content = format!("keep\n{}\n", super::super::shell::END_MARKER);

        assert!(matches!(
            has_managed_block(&content),
            Err(InstallError::MalformedShellBlock)
        ));
    }

    #[test]
    fn remove_if_present_reports_a_path_it_cannot_remove() {
        let temp = tempfile::TempDir::new().unwrap();

        assert!(remove_if_present(temp.path()).is_err());
    }
}
