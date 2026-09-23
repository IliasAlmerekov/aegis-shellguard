//! Production source-target router (ADR-022 §6, L1 Iteration 4 slices 1-3).
//!
//! Detects analyzable source in an intercepted command, reusing the real
//! `aegis-parser` tokenizer and `Effective program` resolution instead of the
//! ad hoc helpers in the Iteration-0 `aegis_language::router` prototype
//! (`aegis-language` cannot depend on `aegis-parser` to reuse them directly —
//! ADR-022 §4's leaf-crate boundary, pinned by
//! `tests/aegis_language_boundary.rs`). This module lives in the root `aegis`
//! crate, which already depends on both.
//!
//! [`route`] is pure and performs no filesystem access — it only decides
//! *what* to analyze ([`RoutedTarget::Inline`] source it already has in hand,
//! or a [`RoutedTarget::ScriptFile`] path it has not read yet). Turning a
//! `ScriptFile` route into an actual [`aegis_language::SourceTarget`] is
//! [`resolve`]'s job, which defers to [`crate::analysis::source_reader`] for
//! the bounded, catch-only read (ADR-022 §6).
//!
//! Slice 1 (explicit interpreter, versioned-basename normalization,
//! trusted-alias resolution), slice 2 (script-file argv routing, verified
//! shebang, direct-exec-by-shebang), slice 3 (heredoc/here-string/
//! literal-producer stdin, via [`crate::analysis::heredoc`]), slice 4
//! (literal top-level `cd -- <path> &&` tracking), and the deferred
//! same-command heredoc-to-file reuse ([`heredoc_write_then_exec_reuse`],
//! narrowly scoped to `cat > PATH`/`tee PATH <<HEREDOC && <interp> PATH`) are
//! in scope. `aegis-config` budget/trusted-alias wiring lands in a later
//! slice per `docs/plans/2026-07-16-language-aware-analysis.md`.

use std::path::{Path, PathBuf};

use aegis_language::{SourceLanguage, SourceTarget};
use aegis_types::DegradationReason;

use super::AnalysisCwd;
use super::heredoc::{self, StdinRoute};
use super::source_reader::{self, SourceReadError};

mod segments;
use segments::{CwdState, route_list_segment};

/// A source-analysis route decided without (for `Inline`) or before (for
/// `ScriptFile`) any filesystem access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedTarget {
    /// An inline interpreter body (`-c` / `-e`) — the source is already in
    /// hand from the command string.
    Inline {
        /// The language the inline body should be parsed as.
        language: SourceLanguage,
        /// The inline source body.
        source: String,
    },
    /// A script file named in argv, or directly executed with a verified
    /// shebang. Not yet read — [`resolve`] performs the bounded read.
    ScriptFile {
        /// The language the file should be parsed as.
        language: SourceLanguage,
        /// The path as it appeared in the command (not yet canonicalized).
        path: PathBuf,
    },
    /// The interpreter has a source (stdin, a dynamic pipeline, …) that could
    /// not be statically recovered. Never claims safety — always resolves to
    /// typed degradation.
    Dynamic {
        /// The language that would have been analyzed.
        language: SourceLanguage,
        /// Why the source could not be recovered.
        reason: DegradationReason,
    },
    /// A route whose source language cannot be established without resolving
    /// an unsafe or unavailable prerequisite. It still carries degradation so
    /// routing never silently drops a source candidate.
    Unresolved {
        /// Why routing could not establish an analyzable target.
        reason: DegradationReason,
    },
    /// A path-like program token (`./script.py`, `/abs/path/script`) executed
    /// directly, with no known interpreter naming it. [`resolve`] reads the
    /// file and only treats it as a target if its first line is a verified
    /// shebang (ADR-022 §6) — no `PATH`/`--version`/content-guessing probes.
    DirectExec {
        /// The path as it appeared in the command.
        path: PathBuf,
    },
}

/// A route that did not resolve into an analyzable [`SourceTarget`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedTarget {
    /// The route language, when routing established one before resolution.
    ///
    /// A direct executable under a dynamic cwd has no language until its
    /// shebang can be read, so its degradation deliberately carries `None`.
    pub language: Option<SourceLanguage>,
    /// Why it did not resolve.
    pub reason: DegradationReason,
}

/// Resolution result used by live orchestration, including original-byte hash
/// metadata and degradations that do not have a known language yet.
pub(super) enum Resolution {
    Resolved {
        language: SourceLanguage,
        source: String,
        source_hash: Option<String>,
        source_byte_offset: usize,
    },
    Degraded(DegradationReason),
    NotApplicable,
}

pub(super) async fn resolve_for_analysis(
    target: RoutedTarget,
    command_cwd: AnalysisCwd<'_>,
    script_file_limit_bytes: u64,
) -> Resolution {
    match target {
        RoutedTarget::Inline { language, source } => Resolution::Resolved {
            language,
            source,
            source_hash: None,
            source_byte_offset: 0,
        },
        RoutedTarget::ScriptFile { language, path } => {
            let path = match resolve_command_path(&path, command_cwd) {
                Ok(path) => path,
                Err(reason) => return Resolution::Degraded(reason),
            };
            match source_reader::read_script_file(&path, script_file_limit_bytes).await {
                Ok(read) => Resolution::Resolved {
                    language,
                    source: read.source,
                    source_hash: Some(read.source_hash),
                    source_byte_offset: read.source_byte_offset,
                },
                Err(err) => Resolution::Degraded(degradation_reason(&err)),
            }
        }
        RoutedTarget::Dynamic { reason, .. } => Resolution::Degraded(reason),
        RoutedTarget::Unresolved { reason } => Resolution::Degraded(reason),
        RoutedTarget::DirectExec { path } => {
            let path = match resolve_command_path(&path, command_cwd) {
                Ok(path) => path,
                Err(reason) => return Resolution::Degraded(reason),
            };
            match source_reader::read_script_file(&path, script_file_limit_bytes).await {
                Ok(read) => {
                    let Some(language) = read
                        .source
                        .lines()
                        .next()
                        .and_then(verified_shebang_language)
                    else {
                        return Resolution::NotApplicable;
                    };
                    Resolution::Resolved {
                        language,
                        source: read.source,
                        source_hash: Some(read.source_hash),
                        source_byte_offset: read.source_byte_offset,
                    }
                }
                // A verified shebang is a short ASCII prefix, so a file too
                // large to read within budget or not valid UTF-8 at all
                // cannot carry one — conclusively "no verified shebang", the
                // same outcome as the `Ok` branch above with no `#!` match,
                // not suspicious unresolved content. Otherwise a directly
                // executed compiled binary (e.g. `actionlint`, #345) would
                // degrade to a language-aware confirmation it can never pass
                // non-interactively, despite being no more opaque to Aegis
                // than any other `PATH`-resolved binary.
                Err(SourceReadError::TooLarge { .. } | SourceReadError::InvalidUtf8) => {
                    Resolution::NotApplicable
                }
                Err(err) => Resolution::Degraded(degradation_reason(&err)),
            }
        }
    }
}

fn resolve_command_path(
    path: &Path,
    command_cwd: AnalysisCwd<'_>,
) -> Result<PathBuf, DegradationReason> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    match command_cwd {
        AnalysisCwd::Resolved(cwd) => Ok(cwd.join(path)),
        AnalysisCwd::Unavailable => Err(DegradationReason::DynamicSource),
    }
}

/// A known interpreter invocation shape.
struct Interpreter {
    /// The canonical registry program name (after basename/version normalization).
    program: &'static str,
    /// The inline-source flag, e.g. `-c` (Python/Bash) or `-e` (Node).
    inline_flag: &'static str,
    /// The language the interpreter's source should be parsed as.
    language: SourceLanguage,
}

/// The L1 foundation interpreters that expose inline or file source. Shell
/// `sh` is mapped onto the Bash grammar (the L1 Shell/Bash adapter, ADR-022
/// §9).
const INTERPRETERS: &[Interpreter] = &[
    Interpreter {
        program: "python3",
        inline_flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "python",
        inline_flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "bash",
        inline_flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "sh",
        inline_flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "node",
        inline_flag: "-e",
        language: SourceLanguage::JavaScript,
    },
];

/// Route analyzable source in `command`.
///
/// `trusted_aliases` maps a trusted global alias (e.g. a wrapper script name)
/// to the canonical registry `program` name it stands in for (e.g. `"py"` →
/// `"python3"`). It is a caller-supplied parameter rather than an
/// `aegis-config` read: config wiring for trusted aliases is a follow-up
/// slice.
///
/// Every top-level list segment of `command` is routed, not only the first
/// (issue #384: `true; python3 evil.py` used to route nothing because routing
/// only looked at the command's first effective token). [`aegis_parser::list_segments`]
/// is heredoc-aware, so a segment that owns a heredoc marker keeps its body
/// (and anything `&&`-chained on the marker's own line) glued to it rather
/// than splitting on a separator inside the body (ADR-022 §6).
#[must_use]
pub fn route(command: &str, trusted_aliases: &[(&str, &str)]) -> Vec<RoutedTarget> {
    let mut cwd = CwdState::Unset;
    let mut targets = Vec::new();
    for segment in aegis_parser::list_segments(command) {
        route_list_segment(&segment, trusted_aliases, &mut cwd, &mut targets);
    }
    targets
}

/// A path with no substitution, expansion, or glob syntax.
fn is_literal_path(path: &str) -> bool {
    !path.is_empty() && !path.contains(['$', '`', '*', '~', '?', '[', ']', '{', '}'])
}

/// A bare path-like program token (`./script.py`, `/abs/path/script`) is a
/// candidate direct-exec target; a plain name (no `/`) would require `PATH`
/// resolution, which routing never performs (ADR-022 §6).
fn direct_exec_route(program_token: &str) -> Option<RoutedTarget> {
    program_token
        .contains('/')
        .then(|| RoutedTarget::DirectExec {
            path: PathBuf::from(program_token),
        })
}

fn stdin_target(language: SourceLanguage, route: StdinRoute) -> RoutedTarget {
    match route {
        StdinRoute::Literal(source) => RoutedTarget::Inline { language, source },
        StdinRoute::Dynamic(reason) => RoutedTarget::Dynamic { language, reason },
    }
}

/// Detect the narrow `<write-cmd> <<HEREDOC && <interp> <path>` shape (the
/// `&&`-chained exec lives on the same physical line as the heredoc redirect
/// — real shell grammar reads the heredoc body starting on the *next* line,
/// terminated by a bare delimiter line, regardless of what follows the
/// redirect on the opening line) and reuse the already-in-hand heredoc body
/// instead of routing a `ScriptFile` that would re-read the identical content
/// from disk.
///
/// Recognized exactly: a write command of `cat > PATH` or `tee PATH` before
/// the heredoc marker, exactly one top-level `&&` after it (checked by
/// rejecting any further separator token in the exec part), and a second
/// segment that is exactly `<interpreter> PATH` (no flags, no other
/// arguments) naming the identical literal path. Any other shape — no `&&`
/// chain, `;`/`||` instead, a mismatched path, or extra exec-segment tokens —
/// is not recognized here and falls through to the existing routing above,
/// per `docs/plans/2026-07-16-language-aware-analysis.md` Iteration 4.
fn heredoc_write_then_exec_reuse(
    command: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<Vec<RoutedTarget>> {
    let first_line = command.lines().next()?;
    let (before_marker, after_marker) = aegis_parser::split_at_heredoc_marker(first_line)?;
    let write_path = heredoc_write_target(before_marker)?;

    let exec_part = after_marker.trim_start().strip_prefix("&&")?.trim_start();
    if exec_part.is_empty() {
        return None;
    }
    let exec_tokens = aegis_parser::split_tokens(exec_part);
    if exec_tokens
        .iter()
        .any(|tok| matches!(tok.as_str(), ";" | "&&" | "||" | "|"))
    {
        // A further top-level separator means this is not the narrow
        // exactly-two-segment shape this reuse is scoped to.
        return None;
    }
    let exec_refs: Vec<&str> = exec_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&exec_refs)
        .into_iter()
        .next()?;
    if slice.tokens.len() != 2 || Path::new(slice.tokens[1]) != write_path {
        return None;
    }
    let interp = resolve_interpreter(slice.program, trusted_aliases)?;

    let heredoc_body = aegis_parser::extract_heredoc_bodies(command)
        .into_iter()
        .next()?;
    let route = heredoc::classify(heredoc_body.body, heredoc_body.is_nowdoc);
    Some(vec![stdin_target(interp.language, route)])
}

/// Recognize a literal `cat > PATH` or `tee PATH` write target — the text
/// before the heredoc marker on its opening line.
fn heredoc_write_target(before_marker: &str) -> Option<PathBuf> {
    let tokens = aegis_parser::split_tokens(before_marker.trim());
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    match refs.as_slice() {
        ["cat", ">", path] | ["tee", path] => Some(PathBuf::from(*path)),
        _ => None,
    }
}

/// Recognize `printf '%s' <literal>` exactly — a narrowly-proven
/// literal-only producer (ADR-022 §6) — and return the literal.
fn printf_percent_s_literal(segment: &str) -> Option<String> {
    let tokens = aegis_parser::split_tokens(segment);
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&refs)
        .into_iter()
        .next()?;
    if slice.program != "printf" {
        return None;
    }
    let rest = &slice.tokens[1..];
    if rest.len() != 2 || rest[0] != "%s" {
        return None;
    }
    Some(rest[1].to_owned())
}

/// Resolve `program` to a registry [`Interpreter`], trying an exact match
/// first, then versioned-basename normalization, then `trusted_aliases`.
fn resolve_interpreter(
    program: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    if let Some(interp) = INTERPRETERS.iter().find(|i| i.program == program) {
        return Some(interp);
    }

    let base = strip_version_suffix(program);
    if base != program
        && let Some((_, canonical)) = BASENAME_FAMILIES.iter().find(|(family, _)| *family == base)
        && let Some(interp) = INTERPRETERS.iter().find(|i| i.program == *canonical)
    {
        return Some(interp);
    }

    let (_, canonical) = trusted_aliases
        .iter()
        .find(|(alias, _)| *alias == program)?;
    INTERPRETERS.iter().find(|i| i.program == *canonical)
}

/// Versioned-basename family prefixes mapped to their canonical registry
/// program name, e.g. `python3.11` and `python3` both normalize to `python3`.
const BASENAME_FAMILIES: &[(&str, &str)] = &[("python", "python3"), ("node", "node")];

/// Strip a trailing version suffix (digits and dots) from a program basename,
/// e.g. `python3.11` → `python`, `node20` → `node`. Returns `name` unchanged
/// if it has no trailing version suffix.
fn strip_version_suffix(name: &str) -> &str {
    let mut end = name.len();
    for (idx, ch) in name.char_indices().rev() {
        if ch.is_ascii_digit() || ch == '.' {
            end = idx;
        } else {
            break;
        }
    }
    &name[..end]
}

/// Extract the inline body for `flag` from token `tok`, returning `None` if
/// `tok` is not the flag. Handles both the standalone (`-c "code"`) and glued
/// (`-ccode`) forms.
fn inline_body(tok: &str, flag: &str, rest: &[&str], pos: usize) -> Option<String> {
    if tok == flag {
        // Standalone flag: the body is the next token, if any.
        return rest.get(pos + 1).map(|s| (*s).to_owned());
    }
    // Glued form: `-c<code>` (flag immediately followed by its body, no `-`
    // continuation, so a lookalike long flag like `-e-x` is not misread as
    // `-e` with body `-x`).
    let stripped = tok.strip_prefix(flag)?;
    if stripped.is_empty() || stripped.starts_with('-') {
        return None;
    }
    Some(stripped.to_owned())
}

/// Resolve every routed target into either an analyzable [`SourceTarget`] or
/// an [`UnresolvedTarget`] carrying typed degradation, performing the bounded
/// catch-only read for [`RoutedTarget::ScriptFile`] routes.
pub async fn resolve(
    routed: Vec<RoutedTarget>,
    script_file_limit_bytes: u64,
) -> Vec<Result<SourceTarget, UnresolvedTarget>> {
    let mut results = Vec::with_capacity(routed.len());
    for target in routed {
        if let Some(result) = resolve_one(target, script_file_limit_bytes).await {
            results.push(result);
        }
    }
    results
}

async fn resolve_one(
    target: RoutedTarget,
    script_file_limit_bytes: u64,
) -> Option<Result<SourceTarget, UnresolvedTarget>> {
    match target {
        RoutedTarget::Inline { language, source } => Some(Ok(SourceTarget { language, source })),
        RoutedTarget::ScriptFile { language, path } => {
            Some(
                match source_reader::read_script_file(&path, script_file_limit_bytes).await {
                    Ok(read) => Ok(SourceTarget {
                        language,
                        source: read.source,
                    }),
                    Err(err) => Err(UnresolvedTarget {
                        language: Some(language),
                        reason: degradation_reason(&err),
                    }),
                },
            )
        }
        RoutedTarget::Dynamic { language, reason } => Some(Err(UnresolvedTarget {
            language: Some(language),
            reason,
        })),
        RoutedTarget::Unresolved { reason } => Some(Err(UnresolvedTarget {
            language: None,
            reason,
        })),
        RoutedTarget::DirectExec { path } => {
            let read = source_reader::read_script_file(&path, script_file_limit_bytes)
                .await
                .ok()?;
            let first_line = read.source.lines().next().unwrap_or("");
            let language = verified_shebang_language(first_line)?;
            Some(Ok(SourceTarget {
                language,
                source: read.source,
            }))
        }
    }
}

fn degradation_reason(err: &SourceReadError) -> DegradationReason {
    match err {
        SourceReadError::NotFound
        | SourceReadError::NotRegularFile
        | SourceReadError::PermissionDenied
        | SourceReadError::Io(_) => DegradationReason::UnsafeSource,
        SourceReadError::TooLarge { .. } => DegradationReason::LimitExceeded,
        SourceReadError::InvalidUtf8 => DegradationReason::UnsupportedEncoding,
    }
}

/// Verify a file begins with a shebang naming a registry interpreter
/// (`#!/usr/bin/env python3`, `#!/usr/bin/python3`), without content-guessing
/// beyond that first line (ADR-022 §6: no `PATH`/`--version` probing).
#[must_use]
pub fn verified_shebang_language(first_line: &str) -> Option<SourceLanguage> {
    let rest = first_line.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let mut program = words.next()?;
    if program.ends_with("/env") || program == "env" {
        program = words.next()?;
    }
    let program = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    resolve_interpreter(program, &[]).map(|interp| interp.language)
}

#[cfg(test)]
mod tests;
