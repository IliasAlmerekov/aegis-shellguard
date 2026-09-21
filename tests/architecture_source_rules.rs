//! Architecture source-rule tests.
//!
//! Source-grep checks for the rules in `ARCHITECTURE.md` §2 and §4 that a
//! `Cargo.toml` dependency check cannot see: edges between modules of one
//! crate, and forbidden patterns inside a crate's production code. Crate-level
//! edges live in `tests/architecture_boundaries.rs`.
//!
//! Conventions:
//! - Every check walks production code only. `production_rs_files` skips files
//!   that a parent module declares under `#[cfg(test)]`, and `strip_test_code`
//!   removes `#[cfg(test)]` items inside a file.
//! - A walk over a missing or empty directory panics, so a moved directory
//!   cannot turn a check into a silent no-op.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

/// Read a source file and strip `#[cfg(test)]`-gated items so that boundary
/// checks only see production code.
fn read_production(relative: &str) -> String {
    strip_test_code(&read_file(relative))
}

/// Strip `#[cfg(test)]`-gated items and `mod tests { … }` blocks.
///
/// Not a full Rust parser — it operates line-by-line and is good enough for
/// the idioms used in this repo:
/// - `#[cfg(test)] use …;` → single-line use
/// - `#[cfg(test)] fn … { … }` / `impl … { … }` / `mod tests { … }` → brace-balanced
/// - `mod tests {` at column 0 without a preceding `#[cfg(test)]` → still stripped
///   because the convention in this repo is that `mod tests` is always test-only.
fn strip_test_code(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        let is_cfg_test = trimmed.starts_with("#[cfg(test)]");
        let is_mod_tests = trimmed.starts_with("mod tests {")
            || trimmed.starts_with("mod tests{")
            || trimmed == "mod tests {"
            || trimmed == "mod tests{";

        if !is_cfg_test && !is_mod_tests {
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
            continue;
        }

        if is_cfg_test {
            // Skip the attribute (and any following attribute lines).
            i += 1;
            while i < lines.len() && lines[i].trim_start().starts_with("#[") {
                i += 1;
            }
            if i >= lines.len() {
                break;
            }
        }

        // Look ahead to decide mode: does this item have a body (`{…}`) or is
        // it a declaration ending at `;`? Multi-line function signatures put
        // the opening `{` several lines below the `fn` keyword, so we cannot
        // decide from the first line alone.
        let mut look = i;
        let mut has_brace = false;
        while look < lines.len() {
            let l = lines[look];
            if let (Some(b), Some(s)) = (l.find('{'), l.find(';')) {
                has_brace = b < s;
                break;
            }
            if l.contains('{') {
                has_brace = true;
                break;
            }
            if l.contains(';') {
                break;
            }
            look += 1;
        }

        if has_brace {
            // Brace-balanced skip. Naive: does not ignore braces in strings or
            // comments, but acceptable for this repo's code. `seen_open`
            // avoids the false exit when the `fn` signature spans multiple
            // lines and the opening `{` is not on the first line.
            let mut depth: i32 = 0;
            let mut seen_open = false;
            loop {
                if i >= lines.len() {
                    break;
                }
                for c in lines[i].chars() {
                    if c == '{' {
                        depth += 1;
                        seen_open = true;
                    } else if c == '}' {
                        depth -= 1;
                    }
                }
                i += 1;
                if seen_open && depth <= 0 {
                    break;
                }
            }
            continue;
        }

        // Single-line item ending at ';'.
        while i < lines.len() && !lines[i].contains(';') {
            i += 1;
        }
        i += 1;
    }

    out
}

/// A `mod name;` declaration: the files it may resolve to, and whether it sits
/// under `#[cfg(test)]`.
#[derive(Debug, PartialEq, Eq)]
struct ModDecl {
    candidates: Vec<PathBuf>,
    test_gated: bool,
}

/// Parse the out-of-line `mod name;` and `include!("path");` declarations of
/// `parent`.
///
/// Line-based, like `strip_test_code`. It does not track inline `mod x { … }`
/// nesting, so a `mod y;` inside an inline module resolves as if it were at
/// file level.
fn mod_declarations(parent: &Path, content: &str) -> Vec<ModDecl> {
    let dir = parent.parent().unwrap_or_else(|| Path::new(""));
    let is_module_root = matches!(
        parent.file_name().and_then(|n| n.to_str()),
        Some("mod.rs" | "lib.rs" | "main.rs")
    );
    let base = if is_module_root {
        dir.to_path_buf()
    } else {
        dir.join(parent.file_stem().unwrap_or_default())
    };

    let mut decls = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    for line in content.lines() {
        let mut rest = line.trim();
        while rest.starts_with("#[") {
            let Some(end) = rest.find(']') else { break };
            attrs.push(rest[..=end].to_string());
            rest = rest[end + 1..].trim_start();
        }
        if rest.is_empty() || rest.starts_with("//") {
            continue;
        }
        let name = mod_decl_name(rest);
        let included = include_target(rest);
        let attrs = std::mem::take(&mut attrs);
        let test_gated = attrs.iter().any(|a| a.contains("cfg(test)"));
        if let Some(target) = included {
            decls.push(ModDecl {
                candidates: vec![dir.join(target)],
                test_gated,
            });
            continue;
        }
        let Some(name) = name else { continue };

        let path_attr = attrs.iter().find_map(|a| {
            let value = a.strip_prefix("#[path")?.trim_start().strip_prefix('=')?;
            Some(
                value
                    .trim_end_matches(']')
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            )
        });
        let candidates = match path_attr {
            Some(p) => vec![dir.join(p)],
            None => vec![
                base.join(format!("{name}.rs")),
                base.join(&name).join("mod.rs"),
            ],
        };
        decls.push(ModDecl {
            candidates,
            test_gated,
        });
    }
    decls
}

/// `include!("path");` → `path`, relative to the including file's directory.
fn include_target(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("include!(")?;
    rest.strip_prefix('"')?.split('"').next()
}

/// `mod name;` (optionally `pub`, `pub(crate)`, …) → `name`.
fn mod_decl_name(line: &str) -> Option<String> {
    let mut rest = line;
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    let name = rest.strip_prefix("mod ")?.trim().strip_suffix(';')?.trim();
    let is_ident = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    is_ident.then(|| name.to_string())
}

/// Files that only compile under `#[cfg(test)]`: declared under it by a parent
/// module, or reachable only through such a file.
fn test_only_files(files: &[(PathBuf, String)]) -> BTreeSet<PathBuf> {
    let known: BTreeSet<&PathBuf> = files.iter().map(|(p, _)| p).collect();
    let mut declared_by: BTreeMap<&PathBuf, Vec<(&PathBuf, bool)>> = BTreeMap::new();
    for (parent, content) in files {
        for decl in mod_declarations(parent, content) {
            for candidate in decl.candidates {
                if let Some(child) = known.get(&candidate) {
                    declared_by
                        .entry(child)
                        .or_default()
                        .push((parent, decl.test_gated));
                }
            }
        }
    }

    let mut test_only: BTreeSet<PathBuf> = BTreeSet::new();
    loop {
        let before = test_only.len();
        for (child, decls) in &declared_by {
            if !test_only.contains(*child)
                && decls
                    .iter()
                    .all(|(parent, gated)| *gated || test_only.contains(*parent))
            {
                test_only.insert((*child).clone());
            }
        }
        if test_only.len() == before {
            return test_only;
        }
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}")) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Production `.rs` files under `relative`, as repo-relative `/`-separated
/// paths paired with their production-only content. Panics when the directory
/// is missing or holds no production file.
fn production_rs_files(relative: &str) -> Vec<(String, String)> {
    let root = repo_root().join(relative);
    assert!(root.is_dir(), "{relative}: directory does not exist");
    let mut paths = Vec::new();
    collect_rs_files(&root, &mut paths);
    paths.sort();
    let files: Vec<(PathBuf, String)> = paths
        .into_iter()
        .map(|p| {
            let content = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
            (p, content)
        })
        .collect();
    let test_only = test_only_files(&files);
    let production: Vec<(String, String)> = files
        .into_iter()
        .filter(|(p, _)| !test_only.contains(p))
        .map(|(p, content)| {
            let rel = p.strip_prefix(repo_root()).unwrap_or(&p);
            let rel = rel.to_string_lossy().replace('\\', "/");
            (rel, strip_test_code(&content))
        })
        .collect();
    assert!(
        !production.is_empty(),
        "{relative}: no production .rs files found"
    );
    production
}

fn assert_absent(source: &str, needle: &str, file: &str, rule: &str) {
    assert!(
        !source.contains(needle),
        "{file}: forbidden pattern {needle:?} — rule: {rule}"
    );
}

/// True when `ident` appears in `source` as a whole identifier, so
/// `evaluate_policy_rules` does not match `evaluate_policy`.
fn contains_ident(source: &str, ident: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';
    source.match_indices(ident).any(|(start, _)| {
        let before = source[..start].chars().next_back();
        let after = source[start + ident.len()..].chars().next();
        !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
    })
}

// ── §2 Diagnostic stream — no raw command or env value in a tracing field ────

/// CONVENTION.md §2: a `tracing` field must never carry a raw command string
/// or an environment variable value. Source-grep floor, not a proof
/// (ADR-033) — it cannot see a path that arrives through `Display` on an
/// error type such as `SnapshotError`.
#[test]
fn tracing_fields_never_carry_raw_command_or_env_value() {
    let files = production_rs_files("src")
        .into_iter()
        .chain(production_rs_files("crates"));

    for (rel, src) in files {
        for forbidden in ["%cmd", "%command", "%raw_command", "%env_value", "%env_var"] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "CONVENTION.md §2: tracing fields must never carry a raw command \
                 string or an environment variable value",
            );
        }
    }
}

// ── §4 Forbidden edges — Policy engine is pure ────────────────────────────────

/// I1 + §4: the policy engine is a pure function. No I/O, no process spawning,
/// no tokio, no filesystem, no logging. The engine lives in `aegis-policy`;
/// `src/decision` only re-exports it.
#[test]
fn decision_engine_is_pure_no_io() {
    for (rel, src) in production_rs_files("crates/aegis-policy/src") {
        for forbidden in [
            "std::fs",
            "std::process",
            "tokio::",
            "std::io",
            "std::env",
            "tracing::",
            "eprintln!",
            "println!",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "I1: policy engine must be a pure function — no I/O",
            );
        }
    }
}

// ── §4 Forbidden edges — Interceptor is a leaf ────────────────────────────────

/// §4: `interceptor/**` may not depend on `audit`, `snapshot`, `ui`, or
/// `runtime`. Scanner is transport-agnostic and has no recovery/logging/UI
/// concerns. `src/interceptor` holds real root-crate code (the scanner cache
/// and `assess()`), so a `Cargo.toml` check cannot see these module edges.
#[test]
fn interceptor_has_no_downstream_dependencies() {
    for (rel, src) in production_rs_files("src/interceptor") {
        for forbidden in [
            "use crate::audit",
            "use crate::snapshot",
            "use crate::ui",
            "use crate::runtime",
            "use crate::planning",
            "use crate::decision",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "§4: interceptor must not depend on audit/snapshot/ui/runtime/planning/decision",
            );
        }
    }
}

// ── §4 Forbidden edges — UI is rendering only ─────────────────────────────────

/// §4: `ui/**` may not write audit entries, run snapshot business logic, or
/// depend on runtime/planning. The documented allow-leak: importing the
/// `SnapshotRecord` display type from `snapshot`. Calling
/// `SnapshotRegistry::*`, `.snapshot_all(`, or `.rollback(` is forbidden.
#[test]
fn ui_does_not_call_audit_or_snapshot_business_logic() {
    // Check both the shim layer (src/ui) and the real implementation (crates/aegis-tui/src).
    let files = production_rs_files("src/ui")
        .into_iter()
        .chain(production_rs_files("crates/aegis-tui/src"));

    for (rel, src) in files {
        // No audit coupling at all (binary-crate or workspace-crate form).
        assert_absent(
            &src,
            "use crate::audit",
            &rel,
            "§4: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "use aegis_audit",
            &rel,
            "§4: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "AuditLogger",
            &rel,
            "§4: UI must not reference AuditLogger",
        );

        // No runtime/planning orchestration leaks.
        assert_absent(
            &src,
            "use crate::runtime",
            &rel,
            "§4: UI must not depend on runtime",
        );
        assert_absent(
            &src,
            "use crate::planning",
            &rel,
            "§4: UI must not depend on planning",
        );

        // Snapshot business logic is forbidden; only SnapshotRecord
        // (data type used for display) is allowed.
        for forbidden in [
            "SnapshotRegistry",
            ".snapshot_all(",
            ".rollback(",
            "snapshot_registry",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "§4: UI may import SnapshotRecord but must not invoke snapshot business logic",
            );
        }
    }
}

// ── §4 Forbidden edges — Transports go through planning ───────────────────────

/// Production paths allowed to name `evaluate_policy`, each with the reason.
const EVALUATE_POLICY_ALLOWED: &[(&str, &str)] = &[
    (
        "src/planning/",
        "the sanctioned consumer: planning is the one caller of the policy engine",
    ),
    (
        "src/decision/mod.rs",
        "re-exports the engine from aegis-policy for the rest of the crate",
    ),
];

/// I4 + §4: no module of the root crate outside `planning` may call
/// `evaluate_policy`; transports (`shell_flow`, `watch`, `install`) must go
/// through `planning::*`.
#[test]
fn transports_route_policy_through_planning_module() {
    for (rel, src) in production_rs_files("src") {
        if EVALUATE_POLICY_ALLOWED
            .iter()
            .any(|(allowed, _)| rel.starts_with(allowed))
        {
            continue;
        }
        assert!(
            !contains_ident(&src, "evaluate_policy"),
            "{rel}: I4: only src/planning may call evaluate_policy — route through planning::*"
        );
    }
}

/// Watch persists across commands, so synchronous platform preparation probes
/// must run off the Tokio worker that owns the control loop.
#[test]
fn watch_sandbox_preparation_does_not_block_the_async_control_loop() {
    let source = read_production("src/watch/sandbox.rs");

    assert!(
        source.contains("async fn prepare_watch_command"),
        "Watch Sandbox preparation must expose an async boundary"
    );
    assert!(
        source.contains("tokio::task::spawn_blocking"),
        "Watch Sandbox preparation must move synchronous platform probes to the blocking pool"
    );
}

// ── Self-tests for the resolver and the walker ────────────────────────────────

#[cfg(test)]
mod resolver_self_tests {
    use super::*;

    fn file(path: &str, content: &str) -> (PathBuf, String) {
        (PathBuf::from(path), content.to_string())
    }

    fn test_only(files: &[(PathBuf, String)]) -> Vec<String> {
        let mut out: Vec<String> = test_only_files(files)
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    fn paths(dir: &str) -> Vec<String> {
        production_rs_files(dir)
            .into_iter()
            .map(|(rel, _)| rel)
            .collect()
    }

    #[test]
    fn a_file_gated_at_the_parent_mod_line_is_test_only() {
        let files = [
            file("c/src/lib.rs", "mod real;\n#[cfg(test)]\nmod tests;\n"),
            file("c/src/real.rs", ""),
            file("c/src/tests.rs", ""),
        ];
        assert_eq!(test_only(&files), ["c/src/tests.rs"]);
    }

    #[test]
    fn everything_below_a_test_only_file_is_test_only() {
        let files = [
            file("c/src/engine.rs", "#[cfg(test)]\nmod tests;\n"),
            file("c/src/engine/tests.rs", "mod recovery;\n"),
            file("c/src/engine/tests/recovery.rs", ""),
        ];
        assert_eq!(
            test_only(&files),
            ["c/src/engine/tests.rs", "c/src/engine/tests/recovery.rs"]
        );
    }

    #[test]
    fn a_parent_named_mod_rs_resolves_children_in_its_directory() {
        let files = [
            file("c/src/model/mod.rs", "#[cfg(test)]\nmod tests;\n"),
            file("c/src/model/tests.rs", ""),
        ];
        assert_eq!(test_only(&files), ["c/src/model/tests.rs"]);
    }

    #[test]
    fn a_child_directory_module_resolves_through_its_mod_rs() {
        let files = [
            file("c/src/lib.rs", "#[cfg(test)]\nmod tests;\n"),
            file("c/src/tests/mod.rs", ""),
        ];
        assert_eq!(test_only(&files), ["c/src/tests/mod.rs"]);
    }

    #[test]
    fn a_path_attribute_redirects_the_child_file() {
        let files = [
            file(
                "c/src/lib.rs",
                "#[cfg(test)]\n#[path = \"support/helpers.rs\"]\nmod helpers;\n",
            ),
            file("c/src/support/helpers.rs", ""),
        ];
        assert_eq!(test_only(&files), ["c/src/support/helpers.rs"]);
    }

    #[test]
    fn an_ungated_declaration_keeps_the_file() {
        let files = [
            file("c/src/lib.rs", "pub mod real;\npub(crate) mod other;\n"),
            file("c/src/real.rs", ""),
            file("c/src/other.rs", ""),
        ];
        assert!(test_only(&files).is_empty());
    }

    #[test]
    fn a_file_declared_both_gated_and_ungated_is_kept() {
        let files = [
            file("c/src/lib.rs", "#[cfg(test)]\nmod shared;\n"),
            file("c/src/main.rs", "mod shared;\n"),
            file("c/src/shared.rs", ""),
        ];
        assert!(test_only(&files).is_empty());
    }

    #[test]
    fn a_cfg_test_attribute_on_the_same_line_gates_the_module() {
        let files = [
            file("c/src/lib.rs", "#[cfg(test)] mod tests;\n"),
            file("c/src/tests.rs", ""),
        ];
        assert_eq!(test_only(&files), ["c/src/tests.rs"]);
    }

    #[test]
    fn an_unrelated_attribute_does_not_gate_the_module() {
        let files = [
            file("c/src/lib.rs", "#[allow(dead_code)]\nmod real;\n"),
            file("c/src/real.rs", ""),
        ];
        assert!(test_only(&files).is_empty());
    }

    #[test]
    fn a_file_included_by_a_test_only_file_is_test_only() {
        let files = [
            file("c/src/lib.rs", "#[cfg(test)]\nmod tests;\n"),
            file("c/src/tests.rs", "include!(\"tests/fragment.rs\");\n"),
            file("c/src/tests/fragment.rs", ""),
        ];
        assert_eq!(
            test_only(&files),
            ["c/src/tests.rs", "c/src/tests/fragment.rs"]
        );
    }

    #[test]
    fn a_file_included_by_production_code_is_kept() {
        let files = [
            file("c/src/lib.rs", "include!(\"data.rs\");\n"),
            file("c/src/data.rs", ""),
        ];
        assert!(test_only(&files).is_empty());
    }

    #[test]
    fn an_undeclared_file_is_kept() {
        let files = [file("c/src/bin/tool.rs", "fn main() {}\n")];
        assert!(test_only(&files).is_empty());
    }

    #[test]
    #[should_panic(expected = "directory does not exist")]
    fn the_walker_panics_on_a_missing_directory() {
        production_rs_files("no/such/directory");
    }

    #[test]
    #[should_panic(expected = "no production .rs files found")]
    fn the_walker_panics_on_a_directory_without_rs_files() {
        production_rs_files("docs");
    }

    #[test]
    fn the_walker_skips_files_gated_in_this_repo() {
        let src = paths("src");
        assert!(src.contains(&"src/shell_flow.rs".to_string()));
        assert!(!src.contains(&"src/shell_flow/test_support.rs".to_string()));
        assert!(
            paths("crates/aegis-config/src")
                .iter()
                .all(|p| !p.contains("/model/tests/"))
        );
        assert!(
            paths("crates/aegis-scanner/src")
                .iter()
                .all(|p| !p.contains("/scanner/tests/"))
        );
    }

    #[test]
    fn contains_ident_matches_whole_identifiers_only() {
        assert!(contains_ident(
            "let x = evaluate_policy(a);",
            "evaluate_policy"
        ));
        assert!(contains_ident("use a::evaluate_policy;", "evaluate_policy"));
        assert!(!contains_ident(
            "evaluate_policy_rules(a)",
            "evaluate_policy"
        ));
        assert!(!contains_ident("my_evaluate_policy(a)", "evaluate_policy"));
        assert!(contains_ident(
            "evaluate_policy_rules(a); evaluate_policy(b)",
            "evaluate_policy"
        ));
    }
}

#[cfg(test)]
mod stripper_self_tests {
    use super::strip_test_code;

    #[test]
    fn strips_cfg_test_mod_block() {
        let input = "\
fn prod() { 1 }

#[cfg(test)]
mod tests {
    fn inner() { 2 }
}

fn more() { 3 }
";
        let out = strip_test_code(input);
        assert!(out.contains("fn prod()"));
        assert!(out.contains("fn more()"));
        assert!(!out.contains("fn inner()"));
        assert!(!out.contains("#[cfg(test)]"));
    }

    #[test]
    fn strips_cfg_test_use_line() {
        let input = "\
use crate::real;

#[cfg(test)]
use crate::only_in_tests;

fn prod() {}
";
        let out = strip_test_code(input);
        assert!(out.contains("use crate::real"));
        assert!(!out.contains("only_in_tests"));
    }

    #[test]
    fn strips_cfg_test_free_fn() {
        let input = "\
fn prod() {}

#[cfg(test)]
fn test_only_helper() {
    let _x = 1;
}

fn more() {}
";
        let out = strip_test_code(input);
        assert!(out.contains("fn prod()"));
        assert!(out.contains("fn more()"));
        assert!(!out.contains("test_only_helper"));
    }

    #[test]
    fn strips_bare_mod_tests() {
        let input = "\
fn prod() {}

mod tests {
    fn inner() {}
}
";
        let out = strip_test_code(input);
        assert!(out.contains("fn prod()"));
        assert!(!out.contains("fn inner()"));
    }

    #[test]
    fn preserves_production_code_with_no_tests() {
        let input = "\
use std::fs;

fn prod() { 1 }
";
        let out = strip_test_code(input);
        assert!(out.contains("use std::fs"));
        assert!(out.contains("fn prod()"));
    }
}
