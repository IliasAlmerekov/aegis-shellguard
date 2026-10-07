//! Tests for the Claude Code plugin shipped under `plugins/aegis/` and listed
//! by the root `.claude-plugin/marketplace.json` (#500, ADR-047).
//!
//! The plugin hooks run as processes with a controlled `PATH`, `HOME`, and
//! `AEGIS_BIN`, the same way Claude Code runs them.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_path(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn assert_executable(path: &Path) {
    let mode = fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "{} must be executable (mode {mode:o})",
        path.display()
    );
}

#[test]
fn claude_plugin_hooks_match_templates() {
    for (plugin_hook, template) in [
        (
            "plugins/aegis/hooks/aegis-pre-tool-use.sh",
            "scripts/hooks/claude-code.sh",
        ),
        (
            "plugins/aegis/hooks/aegis-session-start.sh",
            "scripts/hooks/claude-session-start.sh",
        ),
    ] {
        let expected = read_repo_file(template).replace("__AEGIS_BIN__", "aegis");
        assert_eq!(
            read_repo_file(plugin_hook),
            expected,
            "{plugin_hook} drifted from {template}; regenerate it by replacing __AEGIS_BIN__ with aegis"
        );
        assert_executable(&repo_path(plugin_hook));
    }
}
