use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::Value;

use super::{is_aegis_managed_bash_command, is_aegis_managed_session_start_command};

/// Removal channel: which package channel's binary-removal command to print. Not update consent.
#[derive(Clone, Copy, Debug, ValueEnum)]
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

fn uninstall(args: &crate::UninstallArgs) -> Result<String, String> {
    let home = super::home_dir().ok_or("HOME is not set")?;
    let home = fs::canonicalize(&home)
        .map_err(|error| format!("cannot resolve HOME {}: {error}", home.display()))?;
    if !home.is_dir() || home.parent().is_none() {
        return Err("HOME must be a directory other than the filesystem root".into());
    }
    let invoked = std::env::current_exe()
        .map_err(|error| format!("cannot resolve the running binary: {error}"))?;
    // Resolve symlinks first: a Homebrew link in the curl bindir must match
    // the Cellar path, not the bindir. The canonical path is also the file
    // that a curl removal command has to delete.
    let binary = fs::canonicalize(&invoked).unwrap_or(invoked);
    let channel = args.channel.or_else(|| detect_channel(&binary, &home));
    let data = home.join(".aegis");
    let payloads: Vec<_> = PAYLOADS.iter().map(|path| home.join(path)).collect();
    let mut rc_paths = vec![home.join(".bashrc"), home.join(".zshrc")];
    if let Some(path) = &args.rc_file {
        if !path.is_absolute() {
            return Err("--rc-file must be an absolute path".into());
        }
        if !rc_paths.contains(path) {
            rc_paths.push(path.clone());
        }
    }

    // Validate every known target before changing any integration.
    for path in payloads
        .iter()
        .chain(rc_paths.iter())
        .chain(std::iter::once(&data))
    {
        reject_symlinks(path)?;
    }
    let mut settings = Vec::new();
    for relative in [".claude/settings.json", ".codex/hooks.json"] {
        let path = home.join(relative);
        reject_symlinks(&path)?;
        if path.try_exists().map_err(|error| error.to_string())? {
            let mut value = super::load_settings(&path)?;
            if prune_registrations(&mut value)? {
                settings.push((path, value));
            }
        }
    }
    let mut shell_files = Vec::new();
    for path in rc_paths {
        let Some(content) = read_optional(&path)? else {
            continue;
        };
        if has_managed_block(&content)? {
            shell_files.push((path, super::shell::remove_managed_block(&content)));
        }
    }
    for (path, value) in settings {
        let content = serde_json::to_string_pretty(&value)
            .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?;
        write_existing_file(&path, &format!("{content}\n"))?;
    }
    for (path, content) in shell_files {
        write_existing_file(&path, &content)?;
    }
    for path in payloads {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot remove {}: {error}", path.display())),
        }
    }
    let data_message = if args.purge_data {
        match fs::remove_dir_all(&data) {
            Ok(()) => format!("Removed data directory {}.", data.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                format!("No data directory at {}.", data.display())
            }
            Err(error) => return Err(format!("cannot purge {}: {error}", data.display())),
        }
    } else {
        format!(
            "Data kept at {} if present. Use --purge-data to delete audit logs, snapshots, and update state.",
            data.display()
        )
    };
    let channel_message = if args.channel.is_some() {
        "Using the explicitly selected removal channel."
    } else if channel.is_some() {
        "Removal channel inferred from the canonical binary path, not verified package ownership."
    } else {
        "No known removal channel matches the binary path."
    };
    Ok(format!(
        "Removed managed shell blocks and agent hooks where present.\n{data_message}\nUser configuration and unrelated hooks were kept.\nProject-local hooks from `aegis install-hooks --local` were not touched. Projects where you ran it need manual cleanup: the hook shim denies every Bash command once the binary is gone.\n{channel_message}\nBinary kept at {}. Remove it separately:\n  {}\nOpen a new terminal after removal.",
        binary.display(),
        removal_command(channel, &binary)
    ))
}

fn reject_symlinks(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("refusing to modify symlink {}", ancestor.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot inspect {}: {error}", ancestor.display())),
        }
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn has_managed_block(content: &str) -> Result<bool, String> {
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
                return Err("malformed managed shell block; repair it before uninstalling".into());
            }
            _ => {}
        }
    }
    if inside {
        return Err("unterminated managed shell block; repair it before uninstalling".into());
    }
    Ok(found)
}

fn prune_registrations(settings: &mut Value) -> Result<bool, String> {
    let Some(hooks) = settings.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks.as_object_mut().ok_or("hooks must be a JSON object")?;
    let mut changed = false;
    for section in ["PreToolUse", "SessionStart"] {
        let Some(entries) = hooks.get_mut(section) else {
            continue;
        };
        let entries = entries
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{section} must be an array"))?;
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

fn write_existing_file(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("shell startup file has no parent")?;
    let temporary = super::temporary_settings_path(parent);
    // Only clean up a temporary file that this invocation actually created.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    let result = (|| {
        use std::io::Write;
        file.set_permissions(
            fs::metadata(path)
                .map_err(|error| error.to_string())?
                .permissions(),
        )
        .map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("cannot replace {}: {error}", path.display()))
    })();
    if let Err(original) = &result {
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "{original}; cannot clean up {}: {error}",
                    temporary.display()
                ));
            }
        }
    }
    result
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
