use std::path::{Path, PathBuf};

use super::{InstallError, shell_quote};

pub(super) const BEGIN_MARKER: &str = "# >>> aegis shell setup >>>";
pub(super) const END_MARKER: &str = "# <<< aegis shell setup <<<";

pub(crate) fn run_setup_shell(args: &crate::SetupShellArgs) -> i32 {
    match run_setup_shell_inner(args) {
        Ok(message) => {
            out!("{message}");
            0
        }
        Err(err) => {
            err!("error: {err}");
            crate::EXIT_INTERNAL
        }
    }
}

fn run_setup_shell_inner(args: &crate::SetupShellArgs) -> Result<String, InstallError> {
    let home = crate::install::home_dir().ok_or(InstallError::HomeNotSet)?;
    let aegis_bin = match &args.aegis_bin {
        Some(path) => path.clone(),
        None => std::env::current_exe().map_err(|source| InstallError::Resolve {
            what: "failed to resolve current Aegis binary",
            source,
        })?,
    };

    let real_shell = match &args.shell {
        Some(path) => path.clone(),
        None => detect_real_shell(&aegis_bin)?,
    };

    if crate::shell_compat::same_file(&real_shell, Some(&aegis_bin)) {
        return Err(InstallError::RealShellIsAegis);
    }

    validate_real_shell_path(&real_shell)?;

    let rc_file = resolve_rc_file(&home, &real_shell, args.rc_file.as_deref())?;

    if args.remove {
        remove_shell_setup_file(&rc_file)?;
        return Ok(format!(
            "Aegis shell proxy removed from {}.\nOpen a new terminal or run:\n  source {}",
            rc_file.display(),
            rc_file.display()
        ));
    }

    validate_aegis_binary_path(&aegis_bin)?;

    install_shell_setup_file(&rc_file, &real_shell, &aegis_bin)?;
    Ok(format!(
        "Aegis shell proxy enabled in {}.\nOpen a new terminal or run:\n  source {}\n\nWhat changed:\n  SHELL points to Aegis for tools that launch commands via $SHELL -c.\n  AEGIS_REAL_SHELL keeps your real shell ({}) for command execution.\n\nTo undo this change:\n  aegis setup-shell --remove",
        rc_file.display(),
        rc_file.display(),
        real_shell.display()
    ))
}

fn detect_real_shell(aegis_bin: &Path) -> Result<PathBuf, InstallError> {
    if let Some(value) = std::env::var_os("AEGIS_REAL_SHELL")
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }

    if let Some(value) = std::env::var_os("SHELL")
        && !value.is_empty()
    {
        let shell = PathBuf::from(value);
        if !crate::shell_compat::same_file(&shell, Some(aegis_bin)) {
            return Ok(shell);
        }
    }

    Err(InstallError::RealShellUnknown)
}

fn managed_block(real_shell: &Path, aegis_bin: &Path) -> String {
    format!(
        "{BEGIN_MARKER}\nexport AEGIS_REAL_SHELL={}\nexport SHELL={}\n{END_MARKER}\n",
        shell_quote(&real_shell.to_string_lossy()),
        shell_quote(&aegis_bin.to_string_lossy())
    )
}

fn remove_managed_block(input: &str) -> String {
    let mut output = String::new();
    let mut skipping = false;

    for line in input.lines() {
        if line == BEGIN_MARKER {
            skipping = true;
            continue;
        }
        if line == END_MARKER {
            skipping = false;
            continue;
        }
        if !skipping {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

fn validate_path_chars(path: &Path, label: &'static str) -> Result<(), InstallError> {
    let value = path.to_string_lossy();
    if value.is_empty() {
        return Err(InstallError::EmptyShellPath { label });
    }
    if value.chars().any(char::is_control) {
        return Err(InstallError::ControlCharsInPath { label });
    }
    Ok(())
}

fn validate_real_shell_path(path: &Path) -> Result<(), InstallError> {
    validate_path_chars(path, "real shell path")
}

fn validate_aegis_binary_path(path: &Path) -> Result<(), InstallError> {
    validate_path_chars(path, "aegis binary path")
}

fn resolve_rc_file(
    home: &Path,
    real_shell: &Path,
    override_path: Option<&Path>,
) -> Result<PathBuf, InstallError> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }

    let shell_name = real_shell
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| InstallError::UnnamedShell {
            path: real_shell.to_path_buf(),
        })?;

    match shell_name {
        "bash" => Ok(home.join(".bashrc")),
        "zsh" => Ok(home.join(".zshrc")),
        other => Err(InstallError::UnsupportedShell {
            name: other.to_string(),
        }),
    }
}

fn install_block_in_content(existing: &str, real_shell: &Path, aegis_bin: &Path) -> String {
    let mut cleaned = remove_managed_block(existing);
    if !cleaned.is_empty() && !cleaned.ends_with('\n') {
        cleaned.push('\n');
    }
    cleaned.push_str(&managed_block(real_shell, aegis_bin));
    cleaned
}

fn remove_block_from_content(existing: &str) -> String {
    remove_managed_block(existing)
}

fn read_text_if_exists(path: &Path) -> Result<String, InstallError> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(value),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(InstallError::Io {
            action: "failed to read",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_text_atomically(path: &Path, content: &str) -> Result<(), InstallError> {
    let parent = path.parent().ok_or_else(|| InstallError::InvalidPath {
        path: path.to_path_buf(),
        reason: "does not have a parent directory",
    })?;
    std::fs::create_dir_all(parent).map_err(InstallError::io("failed to create", parent))?;

    aegis_config::write_atomic(path, content.as_bytes(), None)
        .map_err(InstallError::io("failed to replace", path))
}

fn install_shell_setup_file(
    rc_file: &Path,
    real_shell: &Path,
    aegis_bin: &Path,
) -> Result<(), InstallError> {
    let existing = read_text_if_exists(rc_file)?;
    let updated = install_block_in_content(&existing, real_shell, aegis_bin);
    write_text_atomically(rc_file, &updated)
}

fn remove_shell_setup_file(rc_file: &Path) -> Result<(), InstallError> {
    let existing = read_text_if_exists(rc_file)?;
    let updated = remove_block_from_content(&existing);
    write_text_atomically(rc_file, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_text_if_exists_reports_a_path_it_cannot_read() {
        let temp = tempfile::TempDir::new().unwrap();

        let error = read_text_if_exists(temp.path()).unwrap_err();

        assert!(matches!(
            error,
            InstallError::Io {
                action: "failed to read",
                ..
            }
        ));
    }

    #[test]
    fn managed_block_contains_real_shell_and_aegis_bin() {
        let block = managed_block(Path::new("/bin/zsh"), Path::new("/usr/local/bin/aegis"));

        assert_eq!(
            block,
            "# >>> aegis shell setup >>>\nexport AEGIS_REAL_SHELL='/bin/zsh'\nexport SHELL='/usr/local/bin/aegis'\n# <<< aegis shell setup <<<\n"
        );
    }

    #[test]
    fn managed_block_single_quotes_scoped_npm_aegis_binary_path() {
        let scoped = "/home/u/.npm/node_modules/@iliasalmerekov/aegis/vendor/aegis";
        let block = managed_block(Path::new("/bin/zsh"), Path::new(scoped));

        assert!(block.contains(&format!("export SHELL='{scoped}'")));
    }

    #[test]
    fn shell_quote_neutralizes_injection_payload() {
        let quoted = shell_quote("/tmp/aegis\"; export EVIL=1; #");

        assert_eq!(quoted, "'/tmp/aegis\"; export EVIL=1; #'");
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        assert_eq!(shell_quote("foo'bar"), "'foo'\\''bar'");
    }

    #[test]
    fn remove_managed_block_keeps_user_content() {
        let input = "export PATH=\"$HOME/bin:$PATH\"\n# >>> aegis shell setup >>>\nexport AEGIS_REAL_SHELL='/bin/zsh'\nexport SHELL='/usr/local/bin/aegis'\n# <<< aegis shell setup <<<\nalias ll='ls -la'\n";

        let cleaned = remove_managed_block(input);

        assert_eq!(
            cleaned,
            "export PATH=\"$HOME/bin:$PATH\"\nalias ll='ls -la'\n"
        );
    }

    #[test]
    fn install_managed_block_is_idempotent() {
        let original = "export PATH=\"$HOME/bin:$PATH\"\n# >>> aegis shell setup >>>\nexport AEGIS_REAL_SHELL='/bin/bash'\nexport SHELL='/old/aegis'\n# <<< aegis shell setup <<<\n";
        let cleaned = remove_managed_block(original);
        let updated = format!(
            "{}{}",
            cleaned,
            managed_block(Path::new("/bin/zsh"), Path::new("/usr/local/bin/aegis"))
        );

        assert_eq!(updated.matches(BEGIN_MARKER).count(), 1);
        assert!(updated.contains("export AEGIS_REAL_SHELL='/bin/zsh'"));
        assert!(updated.contains("export SHELL='/usr/local/bin/aegis'"));
        assert!(!updated.contains("/old/aegis"));
    }

    #[test]
    fn validate_real_shell_path_rejects_newline_injection() {
        let err = validate_real_shell_path(Path::new("/bin/zsh\nexport EVIL=1")).unwrap_err();

        assert!(err.to_string().contains("control characters"));
        assert!(err.to_string().contains("real shell path"));
    }

    #[test]
    fn validate_aegis_binary_path_accepts_scoped_npm_path() {
        let scoped = Path::new("/home/u/.npm/node_modules/@iliasalmerekov/aegis/vendor/aegis");

        assert!(validate_aegis_binary_path(scoped).is_ok());
    }

    #[test]
    fn validate_aegis_binary_path_names_aegis_binary_on_control_char() {
        let err = validate_aegis_binary_path(Path::new("/tmp/aegis\nexport EVIL=1")).unwrap_err();

        assert!(err.to_string().contains("aegis binary path"));
        assert!(!err.to_string().contains("real shell path"));
    }

    #[test]
    fn resolve_rc_file_uses_zshrc_for_zsh() {
        let rc = resolve_rc_file(Path::new("/Users/aiperi"), Path::new("/bin/zsh"), None).unwrap();

        assert_eq!(rc, PathBuf::from("/Users/aiperi/.zshrc"));
    }

    #[test]
    fn resolve_rc_file_uses_bashrc_for_bash() {
        let rc = resolve_rc_file(Path::new("/Users/aiperi"), Path::new("/bin/bash"), None).unwrap();

        assert_eq!(rc, PathBuf::from("/Users/aiperi/.bashrc"));
    }

    #[test]
    fn resolve_rc_file_accepts_explicit_override_for_other_shells() {
        let rc = resolve_rc_file(
            Path::new("/Users/aiperi"),
            Path::new("/usr/local/bin/fish"),
            Some(Path::new("/tmp/aegis-fish-config")),
        )
        .unwrap();

        assert_eq!(rc, PathBuf::from("/tmp/aegis-fish-config"));
    }

    #[test]
    fn resolve_rc_file_rejects_unsupported_shell_without_override() {
        let err = resolve_rc_file(
            Path::new("/Users/aiperi"),
            Path::new("/usr/local/bin/fish"),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("supports bash and zsh"));
    }

    #[test]
    fn install_block_appends_after_existing_content() {
        let updated = install_block_in_content(
            "alias gs='git status'\n",
            Path::new("/bin/zsh"),
            Path::new("/usr/local/bin/aegis"),
        );

        assert!(updated.starts_with("alias gs='git status'\n"));
        assert!(updated.contains(BEGIN_MARKER));
        assert!(updated.ends_with("# <<< aegis shell setup <<<\n"));
    }

    #[test]
    fn remove_block_from_content_returns_user_content_only() {
        let existing = install_block_in_content(
            "alias gs='git status'\n",
            Path::new("/bin/zsh"),
            Path::new("/usr/local/bin/aegis"),
        );

        let cleaned = remove_block_from_content(&existing);

        assert_eq!(cleaned, "alias gs='git status'\n");
    }
}
