//! List-segment, pipeline-stage, and cwd-tracking routing (issue #384, ADR-022
//! §6). A child module of [`super`] (`router.rs`): every private item there is
//! visible here via `use super::*`, exactly as `router::tests` already relies
//! on for its own tests.
//!
//! [`super::route`] used to look only at a command's first effective token, so
//! `true; python3 evil.py` routed nothing. This module makes every top-level
//! list segment of a compound command ([`aegis_parser::list_segments`]) route
//! independently, and every stage of a multi-stage pipeline within a segment,
//! while tracking the one cwd change ADR-022 §6 allows (`cd -- <path> &&`) across
//! consecutive segments.

use super::*;

/// `true` when `command` contains a genuine heredoc marker (`<<WORD`,
/// `<<'WORD'`, `<<-WORD`, …) — never the unrelated single-line here-string
/// operator `<<<`, which `aegis_parser::extract_heredoc_bodies` already does
/// not match. Guarded by a cheap substring check first so the overwhelmingly
/// common heredoc-free command pays only that scan (route stays allocation-
/// light on the hot path, per `CONVENTION.md` §8).
pub(super) fn command_has_heredoc(command: &str) -> bool {
    command.contains("<<") && !aegis_parser::extract_heredoc_bodies(command).is_empty()
}

/// The cwd-tracking state threaded across top-level list segments while
/// routing a compound command (ADR-022 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CwdState {
    /// No `cd`/`pushd`/`popd` segment has been seen yet: relative targets are
    /// routed unchanged (resolved later against the real process/command cwd).
    Unset,
    /// A literal `cd -- <path>` segment was seen, directly `&&`-chained to
    /// every segment routed under this state.
    Literal(PathBuf),
    /// A `cd`/`pushd`/`popd` segment was seen whose effect on the cwd cannot
    /// be trusted (dynamic path, wrong operator, or a separator crossed
    /// since the last literal `cd`) — every later relative target degrades.
    Degraded,
}

/// A `cd`/`pushd`/`popd` command shape recognized while walking list segments.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CdKind {
    /// The exact literal `cd -- <path>` shape (no globs/expansions).
    Literal(PathBuf),
    /// Any other `cd`/`pushd`/`popd` invocation.
    Dynamic,
}

/// Recognize a whole segment (or pipeline stage) as a `cd`/`pushd`/`popd`
/// invocation, distinguishing the literal `cd -- <path>` shape ADR-022 §6
/// tracks from every other shape (dynamic path, missing `--`, `pushd`,
/// `popd`), which degrades instead.
fn parse_cd_like(normalized: &str) -> Option<CdKind> {
    let trimmed = normalized.trim();
    if let Some(after) = trimmed.strip_prefix("cd ") {
        if let Some(after_dashdash) = after.trim_start().strip_prefix("-- ") {
            let path = after_dashdash.trim();
            if !path.is_empty() && is_literal_path(path) {
                return Some(CdKind::Literal(PathBuf::from(path)));
            }
        }
        return Some(CdKind::Dynamic);
    }
    if trimmed == "cd" || trimmed == "pushd" || trimmed == "popd" {
        return Some(CdKind::Dynamic);
    }
    if trimmed.starts_with("pushd ") || trimmed.starts_with("popd ") {
        return Some(CdKind::Dynamic);
    }
    None
}

/// Fold a recognized `cd`/`pushd`/`popd` segment into the cwd state carried
/// forward to the next segment. Only a literal `cd -- <path>` immediately
/// followed by `&&` produces a trusted rebase target; every other shape, or
/// any other separator after it, degrades (ADR-022 §6: "a cd whose success is
/// not guaranteed ... leaves the cwd unknown").
fn apply_cd_kind(kind: CdKind, separator: Option<aegis_parser::ListSeparator>) -> CwdState {
    match kind {
        CdKind::Literal(path) if separator == Some(aegis_parser::ListSeparator::And) => {
            CwdState::Literal(path)
        }
        CdKind::Literal(_) | CdKind::Dynamic => CwdState::Degraded,
    }
}

/// Carry `state` across the list operator following the segment it was just
/// applied to. A trusted `Literal` cwd survives only an unbroken `&&` chain;
/// crossing any other operator (`;`, `||`, background `&`, newline — or a
/// pipeline's `|`, handled by the caller before this is reached) degrades it.
/// `Unset` and `Degraded` are unaffected by any separator.
fn advance_across_separator(
    state: CwdState,
    separator: Option<aegis_parser::ListSeparator>,
) -> CwdState {
    match state {
        CwdState::Literal(path) if separator == Some(aegis_parser::ListSeparator::And) => {
            CwdState::Literal(path)
        }
        CwdState::Literal(_) => CwdState::Degraded,
        other => other,
    }
}

/// Translate the router's cwd-tracking state into the [`CwdRoute`] the
/// existing [`apply_cwd`] already knows how to apply, passing an `Unset`
/// target through unchanged (no cwd has ever been tracked for it, so it
/// resolves later against the real command/process cwd, unaffected by this
/// routing stage).
fn apply_current_cwd(target: RoutedTarget, cwd: &CwdState) -> Option<RoutedTarget> {
    match cwd {
        CwdState::Unset => Some(target),
        CwdState::Literal(path) => apply_cwd(target, &CwdRoute::Literal(path.clone())),
        CwdState::Degraded => apply_cwd(target, &CwdRoute::Dynamic),
    }
}

/// Route one top-level [`aegis_parser::ListSegment`], mutating `cwd` for the
/// next segment and appending any produced targets (rebased/degraded per the
/// current cwd state) to `targets`, left to right, in order.
pub(super) fn route_list_segment(
    segment: &aegis_parser::ListSegment,
    trusted_aliases: &[(&str, &str)],
    cwd: &mut CwdState,
    targets: &mut Vec<RoutedTarget>,
) {
    let stages = &segment.pipeline.segments;

    if stages.len() == 1 {
        if let Some(cd_kind) = parse_cd_like(&stages[0].normalized) {
            *cwd = apply_cd_kind(cd_kind, segment.separator);
            return;
        }

        for target in route_single_stage(&stages[0].raw, trusted_aliases) {
            if let Some(routed) = apply_current_cwd(target, cwd) {
                push_unique(targets, routed);
            }
        }
        let current = std::mem::replace(cwd, CwdState::Unset);
        *cwd = advance_across_separator(current, segment.separator);
        return;
    }

    // A multi-stage pipeline: route every stage. A `cd`-like stage produces
    // no target of its own, but running in a pipeline subshell means its
    // outcome (and the pipeline's own cwd afterward) is never trustworthy
    // (ADR-022 §6) — every relative target in this same pipeline degrades,
    // and so does everything after it, same as crossing `;`/`||`/`&`.
    let mut pipeline_had_cd = false;
    let mut stage_targets = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if parse_cd_like(&stage.normalized).is_some() {
            pipeline_had_cd = true;
            continue;
        }

        let routed = route_single_stage(&stage.raw, trusted_aliases);
        if !routed.is_empty() {
            // A stage with its own script file, inline flag, or direct-exec
            // path routes exactly as it would standalone (ADR-022 §6),
            // whatever its position in the pipeline.
            stage_targets.extend(routed);
            continue;
        }
        if index == 0 {
            // No preceding stage to pipe stdin from.
            continue;
        }
        let Some(bare_interp) = bare_stage_interpreter(&stage.raw, trusted_aliases) else {
            continue;
        };
        // A bare interpreter reads whatever the previous stage writes to
        // stdout. Only the narrow, exactly-two-stage `printf '%s' <literal> |
        // <interp>` shape has a statically recoverable producer; every other
        // producer (including a longer chain) is honestly Dynamic rather than
        // evaluated or guessed at (ADR-022 §6).
        if index == 1
            && stages.len() == 2
            && let Some(literal) = printf_percent_s_literal(&stages[0].raw)
        {
            stage_targets.push(RoutedTarget::Inline {
                language: bare_interp.language,
                source: literal,
            });
            continue;
        }
        stage_targets.push(RoutedTarget::Dynamic {
            language: bare_interp.language,
            reason: DegradationReason::DynamicSource,
        });
    }

    if pipeline_had_cd {
        *cwd = CwdState::Degraded;
    }
    for target in stage_targets {
        if let Some(routed) = apply_current_cwd(target, cwd) {
            push_unique(targets, routed);
        }
    }
    let current = std::mem::replace(cwd, CwdState::Unset);
    *cwd = advance_across_separator(current, segment.separator);
}

/// Add `target` to `targets` unless it is already present — list segments are
/// routed independently, so an identical route (e.g. the same script invoked
/// twice) would otherwise be duplicated.
fn push_unique(targets: &mut Vec<RoutedTarget>, target: RoutedTarget) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

/// Resolve `stage` (one pipeline stage's raw text) to its own route: the
/// narrow heredoc-write-then-exec reuse shape first (when `stage` owns a
/// heredoc marker), then explicit interpreter inline/file/redirection argv
/// walk, then heredoc/here-string stdin fallback, then a bare path-like
/// direct-exec candidate. The 2-stage `producer | interp` fallback is the
/// caller's concern (see [`route_list_segment`]'s multi-stage branch).
fn route_single_stage(stage: &str, trusted_aliases: &[(&str, &str)]) -> Vec<RoutedTarget> {
    if command_has_heredoc(stage)
        && let Some(targets) = heredoc_write_then_exec_reuse(stage, trusted_aliases)
    {
        return targets;
    }

    let owned_tokens = aegis_parser::split_tokens(stage);
    if owned_tokens.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();

    let Some(slice) = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next()
    else {
        return Vec::new();
    };

    let Some(interp) = resolve_interpreter(slice.program, trusted_aliases) else {
        let effective_start = tokens.len() - slice.tokens.len();
        return direct_exec_route(tokens[effective_start])
            .into_iter()
            .collect();
    };

    let rest = &slice.tokens[1..];
    match walk_interpreter_argv(interp, rest) {
        ArgvWalk::Routed(target) => vec![target],
        ArgvWalk::NoSource => Vec::new(),
        ArgvWalk::NoMatch => {
            if let Some(stdin_route) =
                heredoc::heredoc_stdin(stage).or_else(|| heredoc::here_string_stdin(rest))
            {
                vec![stdin_target(interp.language, stdin_route)]
            } else {
                Vec::new()
            }
        }
    }
}

/// Resolve `stage` to its interpreter only if it is bare (program token
/// only, no flags or arguments of its own) — the shape that means "reads
/// piped stdin from the previous stage" rather than "has its own source".
fn bare_stage_interpreter(
    stage: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    let owned_tokens = aegis_parser::split_tokens(stage);
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next()?;
    if slice.tokens.len() > 1 {
        return None;
    }
    resolve_interpreter(slice.program, trusted_aliases)
}

/// The result of [`walk_interpreter_argv`] walking one interpreter
/// invocation's own argv.
pub(super) enum ArgvWalk {
    /// An inline body or a script-file argument was found.
    Routed(RoutedTarget),
    /// The interpreter's inline flag was present but carried no body — a
    /// definitive "not a source target", never falling back to stdin.
    NoSource,
    /// Nothing in argv itself routed; the caller decides its own stdin
    /// (heredoc/here-string) fallback.
    NoMatch,
}

/// Walk an interpreter's own argv (`rest`, the effective token slice after
/// the program token) exactly as the interpreter itself would: it keeps
/// consuming flags (including the inline `-c`/`-e` body, which wins
/// immediately) and shell redirections (which the shell strips before exec —
/// the interpreter never sees them) until it hits the first positional
/// (non-flag, non-redirection) token, which is the script file and ends
/// option parsing right there — any flag-shaped token *after* it belongs to
/// the script's own argv, not the interpreter, and must not be misread as the
/// interpreter's inline flag (ADR-022 §6).
///
/// The single interpreter-argv walk shared by every routing call site.
pub(super) fn walk_interpreter_argv(interp: &Interpreter, rest: &[&str]) -> ArgvWalk {
    // The tokenizer has no heredoc-boundary awareness, so tokens *after* a
    // `<<WORD`/`<<<` marker are the heredoc/here-string *body*, not further
    // command arguments. Both the inline-flag scan and the file-argument scan
    // below must stop at the marker, or a crafted heredoc body could be
    // misread as the interpreter's own flag/argument instead of being
    // classified as stdin.
    let marker_pos = rest.iter().position(|tok| tok.starts_with("<<"));
    let before_marker = marker_pos.map_or(rest, |idx| &rest[..idx]);

    let mut pos = 0;
    while pos < before_marker.len() {
        let tok = before_marker[pos];
        if let Some(source) = inline_body(tok, interp.inline_flag, before_marker, pos) {
            if source.is_empty() {
                // Flag present but no inline body to analyze — not a source target.
                return ArgvWalk::NoSource;
            }
            return ArgvWalk::Routed(RoutedTarget::Inline {
                language: interp.language,
                source,
            });
        }
        if is_redirection_operator(tok) {
            // A spaced-out redirection (`> file`, `2> file`, `>> file`) has
            // its target in the *next* token, which the interpreter never
            // sees either — skip both, not just the operator, or the target
            // filename would be misread as the script argument.
            pos += 2;
            continue;
        }
        if !tok.starts_with('-') && !tok.contains('<') && !tok.contains('>') {
            return ArgvWalk::Routed(RoutedTarget::ScriptFile {
                language: interp.language,
                path: PathBuf::from(tok),
            });
        }
        pos += 1;
    }

    ArgvWalk::NoMatch
}

/// A standalone shell redirection operator token (`>`, `>>`, `<`, `2>`, …) —
/// an optional leading file-descriptor number followed by nothing but `<`/`>`
/// characters. A glued form (`>out.txt`, `2>&1`) is not standalone — it
/// carries its own target in the same token and needs no extra token
/// skipped, so it is deliberately excluded here.
fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    // `&>`/`&>>` (bash's combined stdout+stderr redirection) carry one
    // leading `&` before the `<`/`>` run; a glued fd-duplication form like
    // `>&2`/`2>&1` has `&` *after* the `<`/`>` instead and is deliberately
    // left unmatched here — it carries its own target in the same token, so
    // the generic "contains `<`/`>`" fallback already skips just that one
    // token, which is correct.
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}
